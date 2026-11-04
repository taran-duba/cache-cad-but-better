//! Renaming and moving files and folders without breaking the assemblies that use them.
//!
//! SolidWorks stores full paths to every part an assembly or drawing uses, so a rename in
//! Explorer or on Drive breaks them. Here a rename is one push:
//!  1. Find every assembly/drawing in the vault that uses the file(s) being moved.
//!  2. Check out the file and those documents (stop if a teammate has any of them).
//!  3. Rename on this PC, then have SolidWorks repoint each document's references.
//!  4. Push the documents together with the move. Teammates get the move and the updated
//!     documents in the same changeset, so nobody ever has one without the other.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::drive::{DriveApi, RemoteFile};
use crate::error::{Error, Result};
use crate::index::PathMap;
use crate::names::{cad_kind, sanitize, CadKind};
use crate::refs::{resolve_ref, RefReader, RefWriter, Resolved};
use crate::sync::push::PushRequest;
use crate::sync::{localfs, Vault};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameReport {
    pub new_path: PathBuf,
    /// Assemblies and drawings updated to use the new path.
    pub updated: Vec<PathBuf>,
    pub changeset: String,
}

/// A document that uses moved files: (stored reference, new path of what it points at).
struct User {
    id: String,
    rel: PathBuf,
    refs: Vec<(String, PathBuf)>,
}

fn lower(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// `rel` after moving `old` to `new` (unchanged if it isn't `old` or inside it).
fn relocate(rel: &Path, old: &Path, new: &Path) -> PathBuf {
    match rel.strip_prefix(old) {
        Ok(rest) if rest.as_os_str().is_empty() => new.to_path_buf(),
        Ok(rest) => new.join(rest),
        Err(_) => rel.to_path_buf(),
    }
}

fn is_cad(rel: &Path) -> bool {
    cad_kind(&rel.to_string_lossy()) != CadKind::Other
}

impl<D: DriveApi> Vault<D> {
    /// Rename or move the file or folder `id` to `new_rel` (a path inside the vault), updating
    /// every assembly and drawing that uses it. `writer` is `None` when SolidWorks isn't
    /// available, in which case only files no SolidWorks document can use may be moved.
    pub async fn rename(
        &self,
        reader: &impl RefReader,
        writer: Option<&impl RefWriter>,
        id: &str,
        new_rel: &Path,
    ) -> Result<RenameReport> {
        let all = self.index.all_files()?;
        let map = PathMap::build(&self.root_id, &all);
        let by_id: HashMap<&str, &RemoteFile> = all.iter().map(|f| (f.id.as_str(), f)).collect();
        let f = *by_id.get(id).ok_or_else(|| Error::NotFound(id.to_string()))?;
        let old_rel = map.path(id).ok_or_else(|| Error::NotFound(f.name.clone()))?.to_path_buf();
        self.check_new_path(f, &old_rel, new_rel, &map)?;

        // Files that move: the file itself, or everything in the folder.
        let moved: Vec<(String, PathBuf)> = if f.is_folder() {
            self.index
                .descendants(id)?
                .into_iter()
                .filter(|d| !d.is_folder() && map.contains(&d.id))
                .filter_map(|d| Some((d.id.clone(), map.path(&d.id)?.to_path_buf())))
                .collect()
        } else {
            vec![(id.to_string(), old_rel.clone())]
        };
        if writer.is_none() && moved.iter().any(|(_, p)| is_cad(p)) {
            return Err(Error::Conflict(
                "renaming or moving SolidWorks files needs SolidWorks reference checking, on a Windows PC with SolidWorks"
                    .into(),
            ));
        }
        let mut blocked = Vec::new();
        for (mid, rel) in &moved {
            self.check_free(by_id[mid.as_str()], rel, &mut blocked)?;
        }
        if !blocked.is_empty() {
            return Err(Error::Conflict(blocked.join("; ")));
        }

        // Who uses them.
        let targets: HashMap<String, PathBuf> =
            moved.iter().map(|(_, rel)| (lower(rel), relocate(rel, &old_rel, new_rel))).collect();
        let users = match writer {
            Some(_) => self.where_used(reader, &all, &map, &targets).await?,
            None => Vec::new(),
        };
        for u in &users {
            self.check_free(by_id[u.id.as_str()], &u.rel, &mut blocked)?;
        }
        if !blocked.is_empty() {
            return Err(Error::Conflict(format!("can't update everything that uses it: {}", blocked.join("; "))));
        }

        let verb = if old_rel.parent() == new_rel.parent() { "Renamed" } else { "Moved" };
        let message = format!("{verb} {} to {}", old_rel.display(), new_rel.display());
        let journals_before = self.index.journal()?.len();
        let mut locked_now = Vec::new();
        let mut renamed_locally = false;
        let result = self
            .do_rename(writer, f, &old_rel, new_rel, &users, &mut locked_now, &mut renamed_locally, message)
            .await;
        match result {
            Ok(changeset) => Ok(RenameReport {
                new_path: new_rel.to_path_buf(),
                updated: users.iter().map(|u| relocate(&u.rel, &old_rel, new_rel)).collect(),
                changeset,
            }),
            Err(e) if self.index.journal()?.len() > journals_before => Err(Error::Conflict(format!(
                "the rename started but didn't finish ({e}). Permafrost finishes it automatically when it can reach Drive."
            ))),
            Err(e) => {
                self.undo_rename(f, &old_rel, new_rel, &users, &locked_now, renamed_locally).await;
                Err(e)
            }
        }
    }

    fn check_new_path(&self, f: &RemoteFile, old_rel: &Path, new_rel: &Path, map: &PathMap) -> Result<()> {
        let bad = |why: &str| Err(Error::Conflict(format!("can't use {}: {why}", new_rel.display())));
        if new_rel.as_os_str().is_empty() || !new_rel.components().all(|c| matches!(c, Component::Normal(_))) {
            return bad("give a path inside the CAD folder, like Arm/Gear.SLDPRT");
        }
        for c in new_rel.components() {
            let name = c.as_os_str().to_string_lossy();
            if sanitize(&name) != name {
                return bad("Windows doesn't allow that name (no \\ / : * ? \" < > | or a trailing dot or space)");
            }
        }
        if new_rel == old_rel {
            return bad("that's where it already is");
        }
        if f.is_folder() && new_rel.starts_with(old_rel) {
            return bad("a folder can't move inside itself");
        }
        if !f.is_folder() && cad_kind(&new_rel.to_string_lossy()) != cad_kind(&old_rel.to_string_lossy()) {
            return bad("keep the same file type (extension)");
        }
        let case_only = lower(new_rel) == lower(old_rel);
        if !case_only && (map.id_for(new_rel).is_some() || self.abs(new_rel).exists()) {
            return bad("something with that name is already there");
        }
        Ok(())
    }

    /// Note why `f` can't be touched right now, if it can't.
    fn check_free(&self, f: &RemoteFile, rel: &Path, blocked: &mut Vec<String>) -> Result<()> {
        if let Some(l) = f.lock() {
            if !self.i_hold_lock(f) {
                blocked.push(format!("{} is checked out by {}", rel.display(), l.display_owner()));
                return Ok(());
            }
        }
        if let Some(l) = self.index.local(&f.id)? {
            let abs = self.abs(l.rel_path.as_ref());
            if abs.exists() {
                let fp = l.synced_size.zip(l.synced_mtime);
                if localfs::is_modified(&abs, l.synced_md5.as_deref(), fp)? {
                    blocked.push(format!("{} has edits that aren't pushed (push or undo them first)", rel.display()));
                } else if localfs::is_open_in_cad(&abs) {
                    blocked.push(format!("{} is open in SolidWorks (close it first)", rel.display()));
                }
            }
        }
        Ok(())
    }

    /// Every assembly and drawing in the vault that uses one of `targets` (lowercased old path →
    /// new path). References are read once per Drive revision and saved, so later renames are fast.
    async fn where_used(
        &self,
        reader: &impl RefReader,
        all: &[RemoteFile],
        map: &PathMap,
        targets: &HashMap<String, PathBuf>,
    ) -> Result<Vec<User>> {
        let mut users = Vec::new();
        for doc in all {
            let Some(rel) = map.path(&doc.id) else { continue };
            if doc.is_folder() || !matches!(cad_kind(&doc.name), CadKind::Assembly | CadKind::Drawing) {
                continue;
            }
            let refs: Vec<(String, PathBuf)> = self
                .refs_at_head(reader, doc, rel)
                .await?
                .into_iter()
                .filter_map(|stored| {
                    let p = match resolve_ref(&self.root, all, map, &stored) {
                        Resolved::Exact(p) | Resolved::ByName(p) => p,
                        Resolved::Outside(_) => return None,
                    };
                    targets.get(&lower(&p)).map(|new| (stored, new.clone()))
                })
                .collect();
            if !refs.is_empty() {
                users.push(User { id: doc.id.clone(), rel: rel.to_path_buf(), refs });
            }
        }
        Ok(users)
    }

    /// The references stored in the Drive head revision of `doc`.
    async fn refs_at_head(&self, reader: &impl RefReader, doc: &RemoteFile, rel: &Path) -> Result<Vec<String>> {
        let head = doc.head_revision_id.as_deref();
        if head.is_some() && self.index.refs_rev(&doc.id)?.as_deref() == head {
            return self.index.refs(&doc.id);
        }
        let abs = self.abs(rel);
        let local_is_head = match self.index.local(&doc.id)? {
            Some(l) if l.synced_rev.as_deref() == head && abs.exists() => {
                !localfs::is_modified(&abs, l.synced_md5.as_deref(), l.synced_size.zip(l.synced_mtime))?
            }
            _ => false,
        };
        let scratch = self.scratch_dir().join(ulid::Ulid::new().to_string());
        let path = if local_is_head {
            abs
        } else {
            std::fs::create_dir_all(&scratch)?;
            let tmp = scratch.join(&doc.name);
            self.drive.download_file(&doc.id, &tmp).await?;
            tmp
        };
        let read = reader.refs(&path).await;
        let _ = std::fs::remove_dir_all(&scratch);
        let refs = read.map_err(|e| {
            Error::Refs(format!(
                "couldn't read what {} uses ({e}), so the rename was stopped before anything changed",
                rel.display()
            ))
        })?;
        let paths: Vec<String> = refs.into_iter().filter(|r| !r.is_virtual).map(|r| r.path).collect();
        self.index.set_refs_at(&doc.id, head, &paths)?;
        Ok(paths)
    }

    #[allow(clippy::too_many_arguments)]
    async fn do_rename(
        &self,
        writer: Option<&impl RefWriter>,
        f: &RemoteFile,
        old_rel: &Path,
        new_rel: &Path,
        users: &[User],
        locked_now: &mut Vec<String>,
        renamed_locally: &mut bool,
        message: String,
    ) -> Result<String> {
        let mut to_lock: Vec<&str> = users.iter().map(|u| u.id.as_str()).collect();
        if !f.is_folder() {
            to_lock.push(&f.id);
        }
        for lid in to_lock {
            let mine = self.index.file(lid)?.is_some_and(|x| self.i_hold_lock(&x));
            if !mine {
                self.check_out(lid).await?;
                locked_now.push(lid.to_string());
            }
        }

        let (from, to) = (self.abs(old_rel), self.abs(new_rel));
        if from.exists() {
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::rename(&from, &to)?;
            *renamed_locally = true;
        }
        self.relocate_local_states(old_rel, new_rel)?;

        if let Some(w) = writer {
            for u in users {
                let doc = self.abs(&relocate(&u.rel, old_rel, new_rel));
                for (stored, target) in &u.refs {
                    w.replace_ref(&doc, stored, &self.abs(target)).await?;
                }
            }
        }

        let out = self
            .push(PushRequest {
                files: users.iter().map(|u| u.id.clone()).collect(),
                new_files: vec![],
                message,
                keep_locked: false,
                moves: vec![(f.id.clone(), new_rel.to_path_buf())],
            })
            .await?;
        Ok(out.changeset)
    }

    fn relocate_local_states(&self, old_rel: &Path, new_rel: &Path) -> Result<()> {
        for mut l in self.index.all_local()? {
            let rel = PathBuf::from(&l.rel_path);
            let moved = relocate(&rel, old_rel, new_rel);
            if moved != rel {
                l.rel_path = moved.to_string_lossy().to_string();
                self.index.set_local(&l)?;
            }
        }
        Ok(())
    }

    /// Put things back after a rename failed before anything reached Drive. Best effort: each
    /// step is tried even if an earlier one failed.
    async fn undo_rename(
        &self,
        f: &RemoteFile,
        old_rel: &Path,
        new_rel: &Path,
        users: &[User],
        locked_now: &[String],
        renamed_locally: bool,
    ) {
        if renamed_locally {
            if let Err(e) = std::fs::rename(self.abs(new_rel), self.abs(old_rel)) {
                tracing::warn!(%e, "rename rollback: couldn't move the file back");
            }
        }
        if let Err(e) = self.relocate_local_states(new_rel, old_rel) {
            tracing::warn!(%e, "rename rollback: couldn't restore local paths");
        }
        for u in users {
            // Throw away the rewritten copy and get the pushed one back.
            let r = if locked_now.contains(&u.id) {
                self.undo_check_out(&u.id, true).await
            } else {
                self.restore_head(&u.id, &u.rel).await
            };
            if let Err(e) = r {
                tracing::warn!(%e, doc = %u.rel.display(), "rename rollback: couldn't restore document");
            }
        }
        if locked_now.contains(&f.id) {
            if let Err(e) = self.undo_check_out(&f.id, false).await {
                tracing::warn!(%e, "rename rollback: couldn't release the file");
            }
        }
    }

    /// Replace the local copy of a file I have checked out with the latest pushed version.
    pub(crate) async fn restore_head(&self, id: &str, rel: &Path) -> Result<()> {
        std::fs::create_dir_all(self.staging_dir())?;
        let tmp = self.staging_dir().join(ulid::Ulid::new().to_string());
        self.drive.download_file(id, &tmp).await?;
        let dest = self.abs(rel);
        localfs::replace(&tmp, &dest)?;
        if let Some(mut l) = self.index.local(id)? {
            let fp = localfs::fingerprint(&dest);
            l.synced_size = fp.map(|x| x.0);
            l.synced_mtime = fp.map(|x| x.1);
            self.index.set_local(&l)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
