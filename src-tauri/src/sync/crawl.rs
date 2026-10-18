//! Keeping the index current: one flat crawl of the whole shared drive, then
//! incremental `changes.list` polls. Changeset manifests that show up in the
//! feed are downloaded and recorded so pulls can group files atomically.

use std::path::Path;

use crate::drive::{DriveApi, RemoteFile};
use crate::error::Result;
use crate::index::{Index, Manifest};

pub const MANIFEST_PROP: &str = "cadsyncManifest";
const PAGE_TOKEN: &str = "page_token";

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct CrawlStats {
    pub upserted: usize,
    pub removed: usize,
    pub manifests: usize,
}

/// Full rebuild of the remote index. With 5k+ files this is a handful of
/// 1000-item pages instead of one request per folder.
pub async fn full_crawl<D: DriveApi>(drive: &D, index: &Index, drive_id: &str, scratch: &Path) -> Result<CrawlStats> {
    // Take the change token *before* listing so nothing between the two is missed.
    let token = drive.start_page_token(drive_id).await?;
    let mut all = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let page = drive.list_drive_files(drive_id, page_token.as_deref()).await?;
        all.extend(page.files);
        match page.next_page_token {
            Some(t) => page_token = Some(t),
            None => break,
        }
    }
    index.clear_remote()?;
    index.apply_remote(&all, &[])?;
    let manifests = ingest_manifests(drive, index, &all, scratch).await?;
    index.set_meta(PAGE_TOKEN, &token)?;
    Ok(CrawlStats { upserted: all.len(), removed: 0, manifests })
}

/// Apply everything that changed since the last poll.
pub async fn poll_changes<D: DriveApi>(drive: &D, index: &Index, drive_id: &str, scratch: &Path) -> Result<CrawlStats> {
    let Some(mut token) = index.meta(PAGE_TOKEN)? else {
        return full_crawl(drive, index, drive_id, scratch).await;
    };
    let mut stats = CrawlStats::default();
    loop {
        let page = drive.list_changes(drive_id, &token).await?;
        let mut upserts = Vec::new();
        let mut removals = Vec::new();
        for c in page.changes {
            match c.file {
                Some(f) if !c.removed => upserts.push(f),
                _ => removals.push(c.file_id),
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
        let s = full_crawl(&drive, &ix, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 4, "all pages read");
        assert_eq!(ix.file_count().unwrap(), 4);

        drive.external_edit(&gear, b"g2");
        let new = drive.seed_file(&root, "Bumper.SLDPRT", b"b");
        let s = poll_changes(&drive, &ix, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 2);
        assert!(ix.file(&new).unwrap().is_some());
        assert_eq!(ix.file(&gear).unwrap().unwrap().head_revision_id.as_deref(), Some(format!("{gear}-r2").as_str()));

        let s = poll_changes(&drive, &ix, &root, scratch.path()).await.unwrap();
        assert_eq!(s.upserted, 0, "nothing new");
    }
}
