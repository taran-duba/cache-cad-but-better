//! Check-out locks.
//!
//! A lock is two things on the Drive file: appProperties naming the owner
//! (which every cadsync client respects) and a read-only content restriction
//! (which also stops uploads from the Drive website). Drive has no
//! compare-and-swap, so after writing a lock we wait briefly and re-read it;
//! if someone else's lock id is there, they won the race. Check-in verifies
//! the lock and base revision again, so a lost race can never overwrite work.

use super::localfs;
use super::Vault;
use crate::drive::{props, DriveApi, LockInfo, MetadataPatch, RemoteFile};
use crate::error::{Error, Result};
use crate::index::LocalState;

impl<D: DriveApi> Vault<D> {
    pub(crate) fn lock_patch(&self, lock: &LockInfo) -> MetadataPatch {
        MetadataPatch::default()
            .set(props::LOCK_OWNER, &lock.owner)
            .set(props::LOCK_ID, &lock.lock_id)
            .set(props::LOCKED_AT, &lock.locked_at)
            .set(props::LOCK_MACHINE, &lock.machine)
            .read_only(true, Some(format!("Checked out by {} in cadsync", lock.owner)))
    }

    pub(crate) fn unlock_patch() -> MetadataPatch {
        MetadataPatch::default()
            .unset(props::LOCK_OWNER)
            .unset(props::LOCK_ID)
            .unset(props::LOCKED_AT)
            .unset(props::LOCK_MACHINE)
            .read_only(false, None)
    }

    /// Check out a file for editing. Gets the latest version first, so you
    /// never start editing a stale copy.
    pub async fn check_out(&self, file_id: &str) -> Result<LockInfo> {
        let f = self.drive.get_file(file_id).await?;
        self.index.apply_remote(std::slice::from_ref(&f), &[])?;
        if f.is_folder() {
            return Err(Error::Conflict("folders can't be checked out".into()));
        }
        if let Some(l) = f.lock() {
            if self.i_hold_lock(&f) {
                return Ok(l);
            }
            return Err(Error::Locked { name: f.name.clone(), by: l.owner });
        }
        let rel = self
            .paths()?
            .path(file_id)
            .ok_or_else(|| Error::NotFound(f.name.clone()))?
            .to_path_buf();
        self.ensure_latest(&f, &rel).await?;

        let lock = LockInfo {
            owner: self.me.clone(),
            lock_id: ulid::Ulid::new().to_string(),
            locked_at: chrono::Utc::now().to_rfc3339(),
            machine: self.machine.clone(),
        };
        let updated = self.drive.update_metadata(file_id, self.lock_patch(&lock)).await?;
        self.index.apply_remote(std::slice::from_ref(&updated), &[])?;

        tokio::time::sleep(self.lock_verify_delay).await;
        let check = self.drive.get_file(file_id).await?;
        self.index.apply_remote(std::slice::from_ref(&check), &[])?;
        match check.lock() {
            Some(l) if l.lock_id == lock.lock_id => {}
            Some(l) => return Err(Error::Locked { name: f.name, by: l.owner }),
            None => return Err(Error::Conflict(format!("lock on {} was removed", f.name))),
        }

        localfs::set_read_only(&self.abs(&rel), false)?;
        Ok(lock)
    }

    /// Make sure the local copy is the head revision and unmodified.
    async fn ensure_latest(&self, f: &RemoteFile, rel: &std::path::Path) -> Result<()> {
        let dest = self.abs(rel);
        let local = self.index.local(&f.id)?;
        if let Some(l) = &local {
            let fp = l.synced_size.zip(l.synced_mtime);
            if localfs::is_modified(&dest, l.synced_md5.as_deref(), fp)? {
                return Err(Error::Conflict(format!(
                    "{} has local changes made without checking out; save them elsewhere first",
                    f.name
                )));
            }
            if l.synced_rev == f.head_revision_id && dest.exists() {
                return Ok(());
            }
        } else if dest.exists() && f.md5.as_deref() != Some(localfs::md5_file(&dest)?.as_str()) {
            // An untracked file is sitting where this one goes; keep it.
            localfs::set_read_only(&dest, false)?;
            std::fs::rename(&dest, localfs::conflict_name(&dest, &self.me))?;
        }
        if localfs::is_open_in_cad(&dest) {
            return Err(Error::Conflict(format!(
                "close {} in SolidWorks so the latest version can be downloaded",
                f.name
            )));
        }
        let Some(rev) = &f.head_revision_id else { return Ok(()) };
        std::fs::create_dir_all(self.staging_dir())?;
        let tmp = self.staging_dir().join(format!("co-{}", f.id));
        self.drive.download_revision(&f.id, rev, &tmp).await?;
        localfs::replace(&tmp, &dest)?;
        let fp = localfs::fingerprint(&dest);
        self.index.set_local(&LocalState {
            file_id: f.id.clone(),
            rel_path: rel.to_string_lossy().to_string(),
            synced_rev: Some(rev.clone()),
            synced_md5: f.md5.clone(),
            synced_size: fp.map(|x| x.0),
            synced_mtime: fp.map(|x| x.1),
        })?;
        Ok(())
    }

    /// Give up a check-out without checking in. With `discard_changes`, local
    /// edits move to the app trash and the latest version is restored.
    pub async fn undo_check_out(&self, file_id: &str, discard_changes: bool) -> Result<()> {
        let f = self.drive.get_file(file_id).await?;
        if !self.i_hold_lock(&f) {
            return Err(Error::NotCheckedOut(f.name));
        }
        let updated = self.drive.update_metadata(file_id, Self::unlock_patch()).await?;
        self.index.apply_remote(std::slice::from_ref(&updated), &[])?;
        if let Some(l) = self.index.local(file_id)? {
            let p = self.abs(l.rel_path.as_ref());
            if !p.exists() {
                return Ok(());
            }
            let fp = l.synced_size.zip(l.synced_mtime);
            if discard_changes && localfs::is_modified(&p, l.synced_md5.as_deref(), fp)? {
                localfs::move_to_trash(&p, &self.trash_dir())?;
                self.index.remove_local(file_id)?;
                self.ensure_latest(&updated, l.rel_path.as_ref()).await?;
            }
            if p.exists() {
                localfs::set_read_only(&p, true)?;
            }
        }
        Ok(())
    }

    /// Remove someone else's lock (e.g. they left for the season with files
    /// checked out). Their unsaved work stays on their machine.
    pub async fn break_lock(&self, file_id: &str) -> Result<RemoteFile> {
        let patch = Self::unlock_patch().unset(props::PENDING_CHANGESET);
        let updated = self.drive.update_metadata(file_id, patch).await?;
        self.index.apply_remote(std::slice::from_ref(&updated), &[])?;
        tracing::warn!(file = %file_id, by = %self.me, "lock broken");
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::sync::testutil::vault;

    async fn setup() -> (FakeDrive, String) {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let gear = drive.seed_file(&root, "Gear.SLDPRT", b"g1");
        (drive, gear)
    }

    #[tokio::test]
    async fn only_one_person_can_check_out() {
        let (drive, gear) = setup().await;
        let a = vault(&drive, "alex@team.org", "pc-a");
        let b = vault(&drive, "blair@team.org", "pc-b");
        for v in [&a.vault, &b.vault] {
            v.index.set_selection(&v.drive_id, Some(true)).unwrap();
            v.sync_down().await.unwrap();
        }

        let lock = a.vault.check_out(&gear).await.unwrap();
        assert_eq!(lock.owner, "alex@team.org");
        assert!(!localfs::is_read_only(&a.vault.root.join("Gear.SLDPRT")));
        assert!(drive.get_file(&gear).await.unwrap().read_only, "Drive content restriction set");

        match b.vault.check_out(&gear).await {
            Err(Error::Locked { by, .. }) => assert_eq!(by, "alex@team.org"),
            other => panic!("expected Locked, got {other:?}"),
        }
        // Checking out again is a no-op for the holder.
        assert_eq!(a.vault.check_out(&gear).await.unwrap().lock_id, lock.lock_id);

        a.vault.undo_check_out(&gear, false).await.unwrap();
        assert!(localfs::is_read_only(&a.vault.root.join("Gear.SLDPRT")));
        b.vault.check_out(&gear).await.unwrap();
    }

    #[tokio::test]
    async fn check_out_gets_latest_first() {
        let (drive, gear) = setup().await;
        let a = vault(&drive, "alex@team.org", "pc-a");
        a.vault.index.set_selection(&a.vault.drive_id, Some(true)).unwrap();
        a.vault.sync_down().await.unwrap();
        drive.external_edit(&gear, b"g2");
        a.vault.check_out(&gear).await.unwrap();
        assert_eq!(std::fs::read(a.vault.root.join("Gear.SLDPRT")).unwrap(), b"g2");
    }

    #[tokio::test]
    async fn race_loser_is_detected() {
        let (drive, gear) = setup().await;
        let a = vault(&drive, "alex@team.org", "pc-a");
        a.vault.index.set_selection(&a.vault.drive_id, Some(true)).unwrap();
        a.vault.sync_down().await.unwrap();
        a.vault.check_out(&gear).await.unwrap();

        // Blair's client wrote a lock at the same moment and won the last write.
        let blair = drive.as_user("blair@team.org");
        let theirs = LockInfo {
            owner: "blair@team.org".into(),
            lock_id: "other".into(),
            locked_at: String::new(),
            machine: "pc-b".into(),
        };
        blair.update_metadata(&gear, a.vault.lock_patch(&theirs)).await.unwrap();
        a.vault.refresh().await.unwrap();
        assert!(!a.vault.i_hold_lock(&a.vault.index.file(&gear).unwrap().unwrap()));
    }

    #[tokio::test]
    async fn undo_with_discard_restores_latest() {
        let (drive, gear) = setup().await;
        let a = vault(&drive, "alex@team.org", "pc-a");
        a.vault.index.set_selection(&a.vault.drive_id, Some(true)).unwrap();
        a.vault.sync_down().await.unwrap();
        a.vault.check_out(&gear).await.unwrap();
        let p = a.vault.root.join("Gear.SLDPRT");
        std::fs::write(&p, b"oops").unwrap();
        a.vault.undo_check_out(&gear, true).await.unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"g1");
        assert!(localfs::is_read_only(&p));
        assert!(drive.get_file(&gear).await.unwrap().lock().is_none());
    }
}
