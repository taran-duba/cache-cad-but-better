//! Keeping the index current. The vault is one folder (anywhere in Drive), so
//! we list that folder's tree breadth-first, a few dozen folders per request,
//! then follow Drive's change feed and keep only changes inside the tree.
//! Changeset manifests that show up in the feed are downloaded and recorded so
//! pulls can group files atomically.

use std::collections::HashSet;
use std::path::Path;

use crate::drive::{DriveApi, RemoteFile};
use crate::error::Result;
use crate::index::{Index, Manifest};

pub const MANIFEST_PROP: &str = "cadsyncManifest";
const PAGE_TOKEN: &str = "page_token";
/// Which feed `PAGE_TOKEN` belongs to ("drive:<id>" or "user"). A change token
/// from one feed is meaningless in the other, e.g. after the user is added to
/// or removed from the shared drive.
const SCOPE_KEY: &str = "page_token_scope";

fn scope_label(scope: Option<&str>) -> String {
    scope.map_or("user".to_string(), |d| format!("drive:{d}"))
}
/// Folders per listing request (each adds a `'id' in parents` term to the query).
const FOLDERS_PER_QUERY: usize = 30;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct CrawlStats {
    pub upserted: usize,
    pub removed: usize,
    pub manifests: usize,
}

/// Everything inside `roots` (not the roots themselves), at any depth.
pub async fn list_tree<D: DriveApi>(drive: &D, scope: Option<&str>, roots: &[String]) -> Result<Vec<RemoteFile>> {
    let mut all = Vec::new();
    let mut frontier: Vec<String> = roots.to_vec();
    let mut seen: HashSet<String> = roots.iter().cloned().collect();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for chunk in frontier.chunks(FOLDERS_PER_QUERY) {
            let mut page_token: Option<String> = None;
            loop {
                let page = drive.list_children(scope, chunk, page_token.as_deref()).await?;
                for f in page.files {
                    // Guards against shortcuts/odd structures looping forever.
                    if f.is_folder() && seen.insert(f.id.clone()) {
                        next.push(f.id.clone());
                    }
                    all.push(f);
                }
                match page.next_page_token {
                    Some(t) => page_token = Some(t),
                    None => break,
                }
            }
        }
        frontier = next;
    }
    Ok(all)
}

/// Full rebuild of the remote index for the vault folder.
pub async fn full_crawl<D: DriveApi>(
    drive: &D,
    index: &Index,
    scope: Option<&str>,
    root_id: &str,
    scratch: &Path,
) -> Result<CrawlStats> {
    // Take the change token *before* listing so nothing between the two is missed.
    let token = drive.start_page_token(scope).await?;
    let all = list_tree(drive, scope, &[root_id.to_string()]).await?;
    index.clear_remote()?;
    index.apply_remote(&all, &[])?;
    let manifests = ingest_manifests(drive, index, &all, scratch).await?;
    index.set_meta(SCOPE_KEY, &scope_label(scope))?;
    index.set_meta(PAGE_TOKEN, &token)?;
    Ok(CrawlStats { upserted: all.len(), removed: 0, manifests })
}

/// Apply everything that changed inside the vault folder since the last poll.
pub async fn poll_changes<D: DriveApi>(
    drive: &D,
    index: &Index,
    scope: Option<&str>,
    root_id: &str,
    scratch: &Path,
) -> Result<CrawlStats> {
    let same_feed = index.meta(SCOPE_KEY)?.as_deref() == Some(scope_label(scope).as_str());
    let token = index.meta(PAGE_TOKEN)?.filter(|_| same_feed);
    let Some(mut token) = token else {
        return full_crawl(drive, index, scope, root_id, scratch).await;
    };
    let mut stats = CrawlStats::default();
    loop {
        tracing::debug!(%token, "changes: asking Drive what changed");
        let page = drive.list_changes(scope, &token).await?;
        tracing::debug!(count = page.changes.len(), "changes: Drive answered");
        let (upserts, removals, new_folders) = filter_changes(index, root_id, page.changes)?;

        // A folder that just appeared in the tree (dragged in from elsewhere)
        // brings its contents with it, and Drive sends no change events for
        // those, so list them.
        let mut upserts = upserts;
        if !new_folders.is_empty() {
            let known: HashSet<String> = upserts.iter().map(|f| f.id.clone()).collect();
            for f in list_tree(drive, scope, &new_folders).await? {
                if !known.contains(&f.id) {
                    upserts.push(f);
                }
            }
        }

        stats.upserted += upserts.len();
        stats.removed += removals.len();
        index.apply_remote(&upserts, &removals)?;
        stats.manifests += ingest_manifests(drive, index, &upserts, scratch).await?;

        if let Some(next) = page.next_page_token {
            token = next;
            index.set_meta(PAGE_TOKEN, &token)?;
            continue;
        }
        if let Some(new_start) = page.new_start_page_token {
            index.set_meta(PAGE_TOKEN, &new_start)?;
        }
        return Ok(stats);
    }
}

/// Keep only changes that belong to the vault tree.
///
/// Returns (items to store, ids to drop, folders newly inside the tree).
/// A change belongs to the tree if its parent is the root or an already-known
/// folder; a page can list a child before its new parent folder, so this
/// repeats until nothing more joins. Known items that were trashed or moved
/// out of the tree are dropped.
fn filter_changes(
    index: &Index,
    root_id: &str,
    changes: Vec<crate::drive::Change>,
) -> Result<(Vec<RemoteFile>, Vec<String>, Vec<String>)> {
    let known: HashSet<String> = index.all_files()?.into_iter().map(|f| f.id).collect();
    let mut in_tree: HashSet<String> = known.clone();
    in_tree.insert(root_id.to_string());

    let mut candidates: Vec<RemoteFile> = Vec::new();
    let mut removals: HashSet<String> = HashSet::new();
    for c in changes {
        match c.file {
            Some(f) if !c.removed && !f.trashed => candidates.push(f),
            _ => {
                if known.contains(&c.file_id) {
                    removals.insert(c.file_id);
                }
            }
        }
    }

    let mut accepted: Vec<RemoteFile> = Vec::new();
    loop {
        let before = accepted.len();
        candidates.retain(|f| {
            if f.parent.as_ref().is_some_and(|p| in_tree.contains(p)) {
                in_tree.insert(f.id.clone());
                accepted.push(f.clone());
                false
            } else {
                true
            }
        });
        if accepted.len() == before {
            break;
        }
    }
    // What's left isn't in the tree. If we knew it, it was moved out.
    for f in candidates {
        if known.contains(&f.id) {
            removals.insert(f.id);
        }
    }

    let new_folders = accepted.iter().filter(|f| f.is_folder() && !known.contains(&f.id)).map(|f| f.id.clone()).collect();
    // A moved-out folder takes its children with it; drop those we know of.
    let mut removals: Vec<String> = removals.into_iter().collect();
    let drop_children = descendants_of(index, &removals)?;
    removals.extend(drop_children.into_iter().filter(|id| !accepted.iter().any(|f| &f.id == id)));
    removals.sort();
    removals.dedup();
    Ok((accepted, removals, new_folders))
}

/// Known items inside any of `folders`, at any depth.
fn descendants_of(index: &Index, folders: &[String]) -> Result<Vec<String>> {
    if folders.is_empty() {
        return Ok(vec![]);
    }
    let all = index.all_files()?;
    let mut out = Vec::new();
    let mut frontier: Vec<&str> = folders.iter().map(String::as_str).collect();
    while !frontier.is_empty() {
        let next: Vec<&RemoteFile> =
            all.iter().filter(|f| f.parent.as_deref().is_some_and(|p| frontier.contains(&p))).collect();
        out.extend(next.iter().map(|f| f.id.clone()));
        frontier = next.iter().filter(|f| f.is_folder()).map(|f| f.id.as_str()).collect();
    }
    Ok(out)
}

async fn ingest_manifests<D: DriveApi>(drive: &D, index: &Index, files: &[RemoteFile], scratch: &Path) -> Result<usize> {
    let mut n = 0;
    for f in files {
        let Some(id) = f.prop(MANIFEST_PROP) else { continue };
        if index.has_changeset(id)? {
            continue;
        }
        std::fs::create_dir_all(scratch)?;
        let tmp = scratch.join(format!("manifest-{}.json", f.id));
        drive.download_file(&f.id, &tmp).await?;
        let bytes = std::fs::read(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        match serde_json::from_slice::<Manifest>(&bytes) {
            Ok(m) => {
                index.insert_changeset(&m)?;
                n += 1;
            }
            Err(e) => tracing::warn!(file = %f.id, %e, "ignoring unreadable changeset manifest"),
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;

    #[tokio::test]
    async fn crawls_then_polls_incrementally() {
        let drive = FakeDrive::new("a@team.org");
        let root = drive.drive_id.clone();
        let dt = drive.seed_folder(&root, "Drivetrain");
        let gear = drive.seed_file(&dt, "Gear.SLDPRT", b"g1");
        drive.seed_file(&dt, "Plate.SLDPRT", b"p1");
        drive.seed_file(&root, "Robot.SLDASM", b"r1");

        let ix = Index::in_memory().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let s = full_crawl(&drive, &ix, None, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 4, "all pages and levels read");
        assert_eq!(ix.file_count().unwrap(), 4);

        drive.external_edit(&gear, b"g2");
        let new = drive.seed_file(&root, "Bumper.SLDPRT", b"b");
        let s = poll_changes(&drive, &ix, None, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 2);
        assert!(ix.file(&new).unwrap().is_some());
        assert_eq!(ix.file(&gear).unwrap().unwrap().head_revision_id.as_deref(), Some(format!("{gear}-r2").as_str()));

        let s = poll_changes(&drive, &ix, None, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 0, "nothing new");
    }

    /// The vault is one folder inside a bigger drive: everything else is ignored.
    #[tokio::test]
    async fn only_the_chosen_folder_is_indexed() {
        let drive = FakeDrive::new("a@team.org");
        let top = drive.drive_id.clone();
        let cache = drive.seed_folder(&top, "CacheCAD");
        let robot = drive.seed_folder(&cache, "Robot");
        drive.seed_file(&robot, "Arm.SLDASM", b"a");
        let other = drive.seed_folder(&top, "Marketing");
        let logo = drive.seed_file(&other, "logo.png", b"l");
        drive.seed_file(&top, "notes.txt", b"n");

        let ix = Index::in_memory().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let s = full_crawl(&drive, &ix, None, &cache, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 2, "Robot and Arm only");

        // Changes elsewhere in the drive are ignored; changes inside are kept.
        drive.external_edit(&logo, b"l2");
        let new_top = drive.seed_file(&top, "other.txt", b"o");
        let new_in = drive.seed_file(&robot, "Wrist.SLDPRT", b"w");
        let s = poll_changes(&drive, &ix, None, &cache, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 1);
        assert!(ix.file(&new_in).unwrap().is_some());
        assert!(ix.file(&logo).unwrap().is_none() && ix.file(&new_top).unwrap().is_none());
    }

    /// Being added to (or removed from) the shared drive changes which change
    /// feed applies; the old position must not be reused.
    #[tokio::test]
    async fn rebuilds_when_the_feed_scope_changes() {
        let drive = FakeDrive::new("a@team.org");
        let root = drive.drive_id.clone();
        drive.seed_file(&root, "A.SLDPRT", b"a");
        let ix = Index::in_memory().unwrap();
        let scratch = tempfile::tempdir().unwrap();

        full_crawl(&drive, &ix, None, &root, scratch.path()).await.unwrap();
        // Same scope: incremental, nothing to do.
        let s = poll_changes(&drive, &ix, None, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 0);

        // The user joins the drive: scope changes, so the whole folder is re-read.
        let s = poll_changes(&drive, &ix, Some("shared-drive"), &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 1, "full crawl happened");
        // ...and now incremental again under the new scope.
        let s = poll_changes(&drive, &ix, Some("shared-drive"), &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 0);
    }

    #[tokio::test]
    async fn follows_items_moved_into_and_out_of_the_folder() {
        let drive = FakeDrive::new("a@team.org");
        let top = drive.drive_id.clone();
        let cache = drive.seed_folder(&top, "CacheCAD");
        let keep = drive.seed_file(&cache, "Keep.SLDPRT", b"k");
        let leave = drive.seed_folder(&cache, "Leaving");
        let leave_child = drive.seed_file(&leave, "Child.SLDPRT", b"c");
        let outside = drive.seed_folder(&top, "Elsewhere");
        let outside_child = drive.seed_file(&outside, "Inside.SLDPRT", b"i");

        let ix = Index::in_memory().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        full_crawl(&drive, &ix, None, &cache, scratch.path()).await.unwrap();
        assert_eq!(ix.file_count().unwrap(), 3);

        // Drag a folder (with contents) in, and another folder (with contents) out.
        drive.move_item(&outside, &cache);
        drive.move_item(&leave, &top);
        let s = poll_changes(&drive, &ix, None, &cache, scratch.path()).await.unwrap();
        assert!(ix.file(&outside).unwrap().is_some(), "folder moved in");
        assert!(ix.file(&outside_child).unwrap().is_some(), "its contents came with it");
        assert!(ix.file(&leave).unwrap().is_none(), "folder moved out");
        assert!(ix.file(&leave_child).unwrap().is_none(), "its contents left with it");
        assert!(ix.file(&keep).unwrap().is_some());
        assert!(s.removed >= 2);
    }
}
