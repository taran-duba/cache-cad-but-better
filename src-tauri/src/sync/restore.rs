//! Bringing back old versions from History.
//!
//! Every push keeps its exact revisions forever, so restoring is: check the files out, put
//! those revisions on disk, and push them as a new changeset. History only ever grows, an
//! assembly and its parts come back together, and teammates get the restore like any push.

use std::path::PathBuf;

use serde::Serialize;

use super::push::PushRequest;
use super::{localfs, shown, Vault};
use crate::drive::DriveApi;
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub changeset: String,
    pub restored: Vec<String>,
    /// Files that were already at that version.
    pub unchanged: usize,
}

impl<D: DriveApi> Vault<D> {
    /// Put the files of push `changeset_id` (or just `only` of them) back to the versions that
    /// push made, as a new push. Stops before changing anything if a file can't be restored.
    pub async fn restore(&self, changeset_id: &str, only: Option<&[String]>) -> Result<RestoreReport> {
        let m = self
            .index
            .changeset(changeset_id)?
            .ok_or_else(|| Error::NotFound("that push isn't in this PC's history".into()))?;
        let map = self.paths()?;
        let (mut todo, mut blocked, mut unchanged) = (Vec::new(), Vec::new(), 0);
        for (id, rev) in &m.files {
            if only.is_some_and(|o| !o.contains(id)) {
                continue;
            }
            let f = match self.drive.get_file(id).await {
                Ok(f) if !f.trashed => f,
                _ => {
                    blocked.push(format!("{} was deleted", self.index.file(id)?.map_or(id.clone(), |f| f.name)));
                    continue;
                }
            };
            self.index.apply_remote(std::slice::from_ref(&f), &[])?;
            if f.is_folder() {
                continue;
            }
            let Some(rel) = map.path(id).map(PathBuf::from) else {
                blocked.push(format!("{} is no longer in the CAD folder", f.name));
                continue;
            };
            if f.head_revision_id.as_deref() == Some(rev.as_str()) {
                unchanged += 1;
                continue;
            }
            if let Some(l) = f.lock() {
                if !self.i_hold_lock(&f) {
                    blocked.push(format!("{} is checked out by {}", shown(&rel), l.display_owner()));
                    continue;
                }
            }
            if let Some(l) = self.index.local(id)? {
                let abs = self.abs(l.rel_path.as_ref());
                if abs.exists() && localfs::is_modified(&abs, l.synced_md5.as_deref(), l.synced_size.zip(l.synced_mtime))? {
                    blocked.push(format!("{} has edits that aren't pushed (push or undo them first)", shown(&rel)));
                    continue;
                }
                if localfs::is_open_in_cad(&abs) {
                    blocked.push(format!("{} is open in SolidWorks (close it first)", shown(&rel)));
                    continue;
                }
            }
            todo.push(Staged { id: id.clone(), rev: rev.clone(), rel, head_md5: f.md5.clone(), tmp: PathBuf::new() });
        }
        if !blocked.is_empty() {
            return Err(Error::Conflict(blocked.join("; ")));
        }

        // Download the old versions first (no locks needed yet), and skip files whose current
        // content already matches, e.g. because they were restored before.
        let stage = self.staging_dir().join(ulid::Ulid::new().to_string());
        std::fs::create_dir_all(&stage)?;
        let result = async {
            let mut changed = Vec::new();
            for (n, mut t) in todo.into_iter().enumerate() {
                t.tmp = stage.join(n.to_string());
                self.drive.download_revision(&t.id, &t.rev, &t.tmp).await?;
                if t.head_md5.as_deref() == Some(localfs::md5_file(&t.tmp)?.as_str()) {
                    unchanged += 1;
                } else {
                    changed.push(t);
                }
            }
            if changed.is_empty() {
                return Err(Error::Conflict("everything is already at that version".into()));
            }
            let when = m.created_time.get(..10).unwrap_or(&m.created_time);
            let message = format!("Restored \"{}\" ({when})", m.message);
            let journals_before = self.index.journal()?.len();
            let mut locked_now = Vec::new();
            match self.swap_in_and_push(&changed, &mut locked_now, message).await {
                Ok(changeset) => Ok(RestoreReport { changeset, restored: changed.iter().map(|t| shown(&t.rel)).collect(), unchanged }),
                Err(e) if self.index.journal()?.len() > journals_before => Err(Error::Conflict(format!(
                    "the restore started but didn't finish ({e}). Permafrost finishes it automatically when it can reach Drive."
                ))),
                Err(e) => {
                    for t in &changed {
                        let r = if locked_now.contains(&t.id) {
                            self.undo_check_out(&t.id, true).await
                        } else {
                            self.restore_head(&t.id, &t.rel).await
                        };
                        if let Err(e) = r {
                            tracing::warn!(%e, file = %t.rel.display(), "restore rollback failed");
                        }
                    }
                    Err(e)
                }
            }
        }
        .await;
        let _ = std::fs::remove_dir_all(&stage);
        result
    }

    async fn swap_in_and_push(&self, todo: &[Staged], locked_now: &mut Vec<String>, message: String) -> Result<String> {
        for t in todo {
            let mine = self.index.file(&t.id)?.is_some_and(|f| self.i_hold_lock(&f));
            if !mine {
                self.check_out(&t.id).await?;
                locked_now.push(t.id.clone());
            }
        }
        for t in todo {
            localfs::replace(&t.tmp, &self.abs(&t.rel))?;
        }
        let out = self
            .push(PushRequest {
                files: todo.iter().map(|t| t.id.clone()).collect(),
                new_files: vec![],
                message,
                keep_locked: false,
                moves: vec![],
            })
            .await?;
        Ok(out.changeset)
    }
}

/// One file to restore: which revision, where it goes, and where it was downloaded to.
struct Staged {
    id: String,
    rev: String,
    rel: PathBuf,
    head_md5: Option<String>,
    tmp: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::testutil::vault;
    use crate::drive::fake::FakeDrive;

    #[tokio::test]
    async fn restores_an_assembly_and_its_parts_together_as_a_new_push() {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let gear = drive.seed_file(&root, "Gear.SLDPRT", b"gear v1");
        let robot = drive.seed_file(&root, "Robot.SLDASM", b"robot v1");
        let a = vault(&drive, "alex@team.org", "pc-a");
        let b = vault(&drive, "blair@team.org", "pc-b");
        for v in [&a.vault, &b.vault] {
            v.sync_down().await.unwrap();
        }
        let av = &a.vault;
        let push = |files: Vec<String>, msg: &str| PushRequest { files, message: msg.into(), ..Default::default() };
        for (g, r, msg) in [(b"gear v2", b"robot v2", "good"), (b"gear v3", b"robot v3", "oops")] {
            av.check_out(&gear).await.unwrap();
            av.check_out(&robot).await.unwrap();
            std::fs::write(av.root.join("Gear.SLDPRT"), g).unwrap();
            std::fs::write(av.root.join("Robot.SLDASM"), r).unwrap();
            av.push(push(vec![gear.clone(), robot.clone()], msg)).await.unwrap();
        }
        let good = av.index.recent_changesets(10).unwrap().into_iter().find(|m| m.message == "good").unwrap();

        let rep = av.restore(&good.id, None).await.unwrap();
        assert_eq!((rep.restored.len(), rep.unchanged), (2, 0));
        assert_eq!(drive.content(&gear).unwrap(), b"gear v2");
        assert_eq!(drive.content(&robot).unwrap(), b"robot v2");
        assert!(drive.get_file(&gear).await.unwrap().lock().is_none(), "released afterwards");
        assert_eq!(av.index.recent_changesets(1).unwrap()[0].message, format!("Restored \"good\" ({})", &good.created_time[..10]));

        b.vault.sync_down().await.unwrap();
        assert_eq!(std::fs::read(b.vault.root.join("Robot.SLDASM")).unwrap(), b"robot v2");

        // Again: already there. One file only, and someone else holding it, are both handled.
        assert!(av.restore(&good.id, None).await.unwrap_err().to_string().contains("already at that version"));
        let oops = av.index.recent_changesets(10).unwrap().into_iter().find(|m| m.message == "oops").unwrap();
        b.vault.check_out(&robot).await.unwrap();
        let e = av.restore(&oops.id, None).await.unwrap_err();
        assert!(e.to_string().contains("checked out by"), "{e}");
        assert_eq!(drive.content(&gear).unwrap(), b"gear v2", "nothing restored when one file is blocked");
        let rep = av.restore(&oops.id, Some(std::slice::from_ref(&gear))).await.unwrap();
        assert_eq!(rep.restored, ["Gear.SLDPRT"]);
        assert_eq!(drive.content(&gear).unwrap(), b"gear v3");
    }
}
