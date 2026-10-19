//! Deleting a file for the whole team. Deleting it locally isn't enough: the
//! next sync puts it back, so nobody loses a part by accident. This moves the
//! Drive file to the shared drive's trash (recoverable for 30 days).

use std::path::PathBuf;

use serde::Serialize;

use super::localfs;
use super::Vault;
use crate::drive::{props, DriveApi};
use crate::error::{Error, Result};
use crate::refs::relative_to;

#[derive(Debug, Clone, Serialize)]
pub struct DeleteOutcome {
    pub name: String,
    /// Where the local copy went (the app's trash folder), if there was one.
    pub local_copy: Option<PathBuf>,
}

impl<D: DriveApi> Vault<D> {
    /// Assemblies and drawings known (from earlier check-ins) to use this file.
    pub fn used_by(&self, file_id: &str) -> Result<Vec<String>> {
        let map = self.paths()?;
        let Some(target) = map.path(file_id).map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase()) else {
            return Ok(vec![]);
        };
        let mut users = Vec::new();
        for (from, to) in self.index.all_refs()? {
            let hit = relative_to(&self.root, &to)
                .is_some_and(|rel| rel.to_string_lossy().replace('\\', "/").to_lowercase() == target);
            if hit && from != file_id {
                if let Some(f) = self.index.file(&from)? {
                    users.push(f.name);
                }
            }
        }
        users.sort();
        users.dedup();
        Ok(users)
    }

    pub async fn delete_from_drive(&self, file_id: &str, force: bool) -> Result<DeleteOutcome> {
        let f = self.drive.get_file(file_id).await?;
        if f.is_folder() {
            return Err(Error::Conflict("folders can't be deleted from cadsync yet; delete the files inside".into()));
        }
        if let Some(l) = f.lock() {
            if !self.i_hold_lock(&f) {
                return Err(Error::Locked { name: f.name, by: l.owner });
            }
        }
        if f.prop(props::PENDING_CHANGESET).is_some() {
            return Err(Error::Conflict(format!("{} is being checked in right now", f.name)));
        }
        let rel = self.paths()?.path(file_id).map(PathBuf::from);
        if let Some(rel) = &rel {
            if localfs::is_open_in_cad(&self.abs(rel)) {
                return Err(Error::Conflict(format!("close {} in SolidWorks first", f.name)));
            }
        }
        let users = self.used_by(file_id)?;
        if !users.is_empty() && !force {
            return Err(Error::InUse { name: f.name, by: users });
        }

        self.drive.trash_file(file_id).await?;
        tracing::info!(file = %f.name, by = %self.me, "moved to the shared drive's trash");
        self.index.apply_remote(&[], &[file_id.to_string()])?;
        self.index.set_refs(file_id, &[])?;
        let mut local_copy = None;
        if let Some(rel) = rel {
            let p = self.abs(&rel);
            if p.exists() {
                local_copy = Some(localfs::move_to_trash(&p, &self.trash_dir())?);
            }
        }
        self.index.remove_local(file_id)?;
        Ok(DeleteOutcome { name: f.name, local_copy })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::sync::testutil::vault;

    #[tokio::test]
    async fn deletes_for_everyone_but_guards_locks_and_users() {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let gear = drive.seed_file(&root, "Gear.SLDPRT", b"g");
        let asm = drive.seed_file(&root, "Arm.SLDASM", b"a");
        let old = drive.seed_file(&root, "Old Bracket.SLDPRT", b"o");
        let a = vault(&drive, "alex@team.org", "pc-a");
        let b = vault(&drive, "blair@team.org", "pc-b");
        for v in [&a.vault, &b.vault] {
            v.index.set_selection(&v.drive_id, Some(true)).unwrap();
            v.sync_down().await.unwrap();
        }
        let (av, bv) = (&a.vault, &b.vault);

        // Someone else has it checked out: refused.
        bv.check_out(&old).await.unwrap();
        assert!(matches!(av.delete_from_drive(&old, false).await, Err(Error::Locked { .. })));
        bv.undo_check_out(&old, false).await.unwrap();
        av.refresh().await.unwrap();

        // Unused file: trashed on Drive, local copy to the app trash, gone for Blair too.
        let out = av.delete_from_drive(&old, false).await.unwrap();
        assert!(drive.get_file(&old).await.unwrap().trashed);
        assert!(out.local_copy.unwrap().exists());
        assert!(!av.root.join("Old Bracket.SLDPRT").exists());
        let r = bv.sync_down().await.unwrap();
        assert_eq!(r.removed, 1);
        assert!(!bv.root.join("Old Bracket.SLDPRT").exists());
        assert_eq!(av.sync_down().await.unwrap().downloaded, 0, "not restored");

        // A part an assembly uses needs confirmation.
        let gear_path = av.root.join("Gear.SLDPRT").to_string_lossy().to_string();
        av.index.set_refs(&asm, &[gear_path]).unwrap();
        match av.delete_from_drive(&gear, false).await {
            Err(Error::InUse { by, .. }) => assert_eq!(by, ["Arm.SLDASM"]),
            other => panic!("expected InUse, got {other:?}"),
        }
        av.delete_from_drive(&gear, true).await.unwrap();
        assert!(drive.get_file(&gear).await.unwrap().trashed);
    }
}
