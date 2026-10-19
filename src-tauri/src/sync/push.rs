//! Atomic check-in (the fix for broken mates).
//!
//! Drive has no transactions, so a check-in is a small protocol:
//!  1. Verify every file is checked out by me and still at the revision I
//!     started from. Nothing is uploaded unless all files pass.
//!  2. Journal the plan locally, then upload parts, then assemblies, then
//!     drawings. Each upload is tagged `cadsyncPending=<changeset>`, and other
//!     clients ignore tagged revisions.
//!  3. Write the manifest `.cadsync/changesets/<id>.json`. That write is the
//!     commit; from then on, other clients pull all files together.
//!  4. Mark revisions keep-forever, clear tags, and release or keep the locks.
//!
//! A crash at any step resumes from the journal, and the locks held the whole
//! time mean nobody else could touch the files meanwhile.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::crawl::MANIFEST_PROP;
use super::localfs;
use super::Vault;
use crate::drive::{props, DriveApi, MetadataPatch, RemoteFile};
use crate::error::{Error, Result};
use crate::index::{LocalState, Manifest, META_FOLDER};
use crate::names::{cad_kind, CadKind};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushRequest {
    /// Existing files, already checked out by me.
    pub files: Vec<String>,
    /// New local files (relative to the vault root) to add to Drive.
    pub new_files: Vec<PathBuf>,
    pub message: String,
    /// Keep the files checked out after the check-in.
    pub keep_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalItem {
    file_id: Option<String>,
    rel_path: PathBuf,
    uploaded_rev: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    id: String,
    message: String,
    keep_locked: bool,
    created_time: String,
    items: Vec<JournalItem>,
    committed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushOutcome {
    pub changeset: String,
    pub files: usize,
}

fn upload_order(rel: &std::path::Path) -> u8 {
    match cad_kind(&rel.to_string_lossy()) {
        CadKind::Other => 0,
        CadKind::Part => 1,
        CadKind::Assembly => 2,
        CadKind::Drawing => 3,
    }
}

impl<D: DriveApi> Vault<D> {
    pub async fn push(&self, req: PushRequest) -> Result<PushOutcome> {
        if req.files.is_empty() && req.new_files.is_empty() {
            return Err(Error::Conflict("nothing to check in".into()));
        }
        let map = self.paths()?;
        let mut items = Vec::new();

        // 1. Verify everything before uploading anything.
        for id in &req.files {
            let f = self.drive.get_file(id).await?;
            self.index.apply_remote(std::slice::from_ref(&f), &[])?;
            if !self.i_hold_lock(&f) {
                return Err(match f.lock() {
                    Some(l) => Error::Locked { name: f.name, by: l.owner },
                    None => Error::NotCheckedOut(f.name),
                });
            }
            let local = self.index.local(id)?.ok_or_else(|| Error::NotFound(f.name.clone()))?;
            if local.synced_rev != f.head_revision_id {
                return Err(Error::Conflict(format!(
                    "{} changed on Drive since you checked it out (was the lock broken?). \
                     Your copy is untouched; save it under another name and get the latest.",
                    f.name
                )));
            }
            let rel = map.path(id).ok_or_else(|| Error::NotFound(f.name.clone()))?.to_path_buf();
            if !self.abs(&rel).exists() {
                return Err(Error::NotFound(rel.display().to_string()));
            }
            items.push(JournalItem { file_id: Some(id.clone()), rel_path: rel, uploaded_rev: None });
        }
        for rel in &req.new_files {
            if map.id_for(rel).is_some() {
                return Err(Error::Conflict(format!("{} is already on Drive", rel.display())));
            }
            if !self.abs(rel).is_file() {
                return Err(Error::NotFound(rel.display().to_string()));
            }
            items.push(JournalItem { file_id: None, rel_path: rel.clone(), uploaded_rev: None });
        }
        items.sort_by_key(|i| upload_order(&i.rel_path));

        let mut j = Journal {
            id: ulid::Ulid::new().to_string(),
            message: req.message,
            keep_locked: req.keep_locked,
            created_time: chrono::Utc::now().to_rfc3339(),
            items,
            committed: false,
        };
        self.save_journal(&j)?;
        self.continue_push(&mut j).await
    }

    /// Finish any check-ins interrupted by a crash or lost connection.
    pub async fn resume_pushes(&self) -> Result<Vec<PushOutcome>> {
        let mut done = Vec::new();
        for (_, payload) in self.index.journal()? {
            let mut j: Journal = serde_json::from_str(&payload)?;
            done.push(self.continue_push(&mut j).await?);
        }
        Ok(done)
    }

    fn save_journal(&self, j: &Journal) -> Result<()> {
        self.index.put_journal(&j.id, &serde_json::to_string(j)?)
    }

    async fn continue_push(&self, j: &mut Journal) -> Result<PushOutcome> {
        // 2. Upload whatever isn't uploaded yet.
        for n in 0..j.items.len() {
            if j.items[n].uploaded_rev.is_some() {
                continue;
            }
            let uploaded = self.upload_item(&j.id, &j.items[n]).await?;
            j.items[n].file_id = Some(uploaded.id.clone());
            j.items[n].uploaded_rev = uploaded.head_revision_id.clone();
            self.index.apply_remote(std::slice::from_ref(&uploaded), &[])?;
            self.save_journal(j)?;
        }

        // 3. Commit.
        let manifest = Manifest {
            id: j.id.clone(),
            author: self.me.clone(),
            message: j.message.clone(),
            created_time: j.created_time.clone(),
            files: j
                .items
                .iter()
                .filter_map(|i| Some((i.file_id.clone()?, i.uploaded_rev.clone()?)))
                .collect::<BTreeMap<_, _>>(),
        };
        if !j.committed {
            self.write_manifest(&manifest).await?;
            j.committed = true;
            self.save_journal(j)?;
        }
        self.index.insert_changeset(&manifest)?;

        // 4. Finalize each file.
        for item in &j.items {
            let (Some(id), Some(rev)) = (&item.file_id, &item.uploaded_rev) else { continue };
            self.drive.keep_revision_forever(id, rev).await?;
            let patch = if j.keep_locked {
                MetadataPatch::default()
                    .unset(props::PENDING_CHANGESET)
                    .read_only(true, Some(format!("Checked out by {} in cadsync", self.me)))
            } else {
                Self::unlock_patch().unset(props::PENDING_CHANGESET)
            };
            let f = self.drive.update_metadata(id, patch).await?;
            self.index.apply_remote(std::slice::from_ref(&f), &[])?;

            let abs = self.abs(&item.rel_path);
            let fp = localfs::fingerprint(&abs);
            self.index.set_local(&LocalState {
                file_id: id.clone(),
                rel_path: item.rel_path.to_string_lossy().to_string(),
                synced_rev: Some(rev.clone()),
                synced_md5: f.md5.clone(),
                synced_size: fp.map(|x| x.0),
                synced_mtime: fp.map(|x| x.1),
            })?;
            if abs.exists() {
                localfs::set_read_only(&abs, !j.keep_locked)?;
            }
        }
        self.index.delete_journal(&j.id)?;
        Ok(PushOutcome { changeset: j.id.clone(), files: j.items.len() })
    }

    async fn upload_item(&self, changeset: &str, item: &JournalItem) -> Result<RemoteFile> {
        let src = self.abs(&item.rel_path);
        match &item.file_id {
            Some(id) => {
                // Lift the content restriction just for the upload; the lock
                // properties stay, so other clients still treat it as checked out.
                let patch = MetadataPatch::default()
                    .set(props::PENDING_CHANGESET, changeset)
                    .read_only(false, None);
                self.drive.update_metadata(id, patch).await?;
                self.drive.upload_content(id, &src).await
            }
            None => {
                let parent = self.ensure_parent_folder(&item.rel_path).await?;
                let name = item.rel_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let props = BTreeMap::from([
                    (props::PENDING_CHANGESET.to_string(), changeset.to_string()),
                    (props::LOCK_OWNER.to_string(), self.me.clone()),
                    (props::LOCK_ID.to_string(), ulid::Ulid::new().to_string()),
                    (props::LOCKED_AT.to_string(), chrono::Utc::now().to_rfc3339()),
                    (props::LOCK_MACHINE.to_string(), self.machine.clone()),
                ]);
                self.drive.create_file(&parent, &name, &src, props).await
            }
        }
    }

    /// Create any Drive folders missing on the way to a new file.
    async fn ensure_parent_folder(&self, rel: &std::path::Path) -> Result<String> {
        let mut parent_id = self.drive_id.clone();
        let mut so_far = PathBuf::new();
        let Some(dir) = rel.parent() else { return Ok(parent_id) };
        for comp in dir.components() {
            so_far.push(comp);
            let map = self.paths()?;
            parent_id = match map.id_for(&so_far) {
                Some(id) => id.to_string(),
                None => {
                    let name = comp.as_os_str().to_string_lossy().to_string();
                    let f = self.drive.create_folder(&parent_id, &name).await?;
                    self.index.apply_remote(std::slice::from_ref(&f), &[])?;
                    f.id
                }
            };
        }
        Ok(parent_id)
    }

    /// The hidden `.cadsync` folder at the drive root, created if missing.
    pub(crate) async fn ensure_meta_folder(&self) -> Result<String> {
        if let Some(id) = self.paths()?.meta_folder_id {
            return Ok(id);
        }
        let f = self.drive.create_folder(&self.drive_id, META_FOLDER).await?;
        self.index.apply_remote(std::slice::from_ref(&f), &[])?;
        Ok(f.id)
    }

    async fn write_manifest(&self, m: &Manifest) -> Result<()> {
        let meta = self.ensure_meta_folder().await?;
        let changesets = match self.index.children(&meta)?.into_iter().find(|f| f.is_folder() && f.name == "changesets") {
            Some(f) => f.id,
            None => {
                let f = self.drive.create_folder(&meta, "changesets").await?;
                self.index.apply_remote(std::slice::from_ref(&f), &[])?;
                f.id
            }
        };
        std::fs::create_dir_all(self.scratch_dir())?;
        let tmp = self.scratch_dir().join(format!("{}.json", m.id));
        std::fs::write(&tmp, serde_json::to_vec_pretty(m)?)?;
        let props = BTreeMap::from([(MANIFEST_PROP.to_string(), m.id.clone())]);
        let f = self.drive.create_file(&changesets, &format!("{}.json", m.id), &tmp, props).await?;
        let _ = std::fs::remove_file(&tmp);
        self.index.apply_remote(std::slice::from_ref(&f), &[])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::sync::pull::Deferred;
    use crate::sync::testutil::vault;

    /// Robot.SLDASM + Gear.SLDPRT on Drive; Alex and Blair both synced.
    async fn team() -> (FakeDrive, String, String, crate::sync::testutil::Fixture, crate::sync::testutil::Fixture) {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let gear = drive.seed_file(&root, "Gear.SLDPRT", b"gear v1");
        let robot = drive.seed_file(&root, "Robot.SLDASM", b"robot v1");
        let a = vault(&drive, "alex@team.org", "pc-a");
        let b = vault(&drive, "blair@team.org", "pc-b");
        for v in [&a.vault, &b.vault] {
            v.index.set_selection(&v.drive_id, Some(true)).unwrap();
            v.sync_down().await.unwrap();
        }
        (drive, gear, robot, a, b)
    }

    fn edit(v: &Vault<FakeDrive>, rel: &str, bytes: &[u8]) {
        std::fs::write(v.root.join(rel), bytes).unwrap();
    }

    #[tokio::test]
    async fn assembly_and_parts_arrive_together() {
        let (drive, gear, robot, a, b) = team().await;
        let av = &a.vault;
        av.check_out(&gear).await.unwrap();
        av.check_out(&robot).await.unwrap();
        edit(av, "Gear.SLDPRT", b"gear v2");
        edit(av, "Robot.SLDASM", b"robot v2");
        std::fs::write(av.root.join("Spacer.SLDPRT"), b"spacer v1").unwrap();

        let out = av
            .push(PushRequest {
                files: vec![robot.clone(), gear.clone()],
                new_files: vec!["Spacer.SLDPRT".into()],
                message: "new gear ratio".into(),
                keep_locked: false,
            })
            .await
            .unwrap();
        assert_eq!(out.files, 3);
        assert!(av.index.changeset(&out.changeset).unwrap().is_some());
        let gf = drive.get_file(&gear).await.unwrap();
        assert!(gf.lock().is_none() && !gf.read_only && gf.prop(props::PENDING_CHANGESET).is_none());
        assert!(drive.is_kept_forever(&gear, gf.head_revision_id.as_deref().unwrap()));
        assert!(localfs::is_read_only(&av.root.join("Gear.SLDPRT")), "released → read-only again");

        let r = b.vault.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 3, "{r:?}");
        assert_eq!(std::fs::read(b.vault.root.join("Robot.SLDASM")).unwrap(), b"robot v2");
        assert_eq!(std::fs::read(b.vault.root.join("Spacer.SLDPRT")).unwrap(), b"spacer v1");
    }

    #[tokio::test]
    async fn half_finished_push_is_invisible_then_resumes() {
        let (drive, gear, robot, a, b) = team().await;
        let av = &a.vault;
        av.check_out(&gear).await.unwrap();
        av.check_out(&robot).await.unwrap();
        edit(av, "Gear.SLDPRT", b"gear v2");
        edit(av, "Robot.SLDASM", b"robot v2");

        drive.crash_after_uploads(1); // gear uploads, then the "app crashes"
        let req = PushRequest { files: vec![gear.clone(), robot.clone()], message: "m".into(), ..Default::default() };
        assert!(av.push(req).await.is_err());
        assert_eq!(drive.content(&gear).unwrap(), b"gear v2", "gear really is on Drive");

        // Blair sees neither file yet: the gear is tagged with an uncommitted changeset.
        let r = b.vault.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 0);
        assert!(r.deferred.iter().any(|d| matches!(d, Deferred::Pending { file_id, .. } if *file_id == gear)));
        assert_eq!(std::fs::read(b.vault.root.join("Gear.SLDPRT")).unwrap(), b"gear v1");

        drive.clear_faults();
        let done = av.resume_pushes().await.unwrap();
        assert_eq!(done.len(), 1);
        assert!(av.index.journal().unwrap().is_empty());

        let r = b.vault.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 2);
        assert_eq!(std::fs::read(b.vault.root.join("Gear.SLDPRT")).unwrap(), b"gear v2");
        assert_eq!(std::fs::read(b.vault.root.join("Robot.SLDASM")).unwrap(), b"robot v2");
    }

    #[tokio::test]
    async fn refuses_files_not_checked_out_or_changed_underneath() {
        let (drive, gear, robot, a, b) = team().await;
        let av = &a.vault;
        let req = |files: Vec<String>| PushRequest { files, message: "m".into(), ..Default::default() };
        assert!(matches!(av.push(req(vec![gear.clone()])).await, Err(Error::NotCheckedOut(_))));

        b.vault.check_out(&robot).await.unwrap();
        assert!(matches!(av.push(req(vec![robot.clone()])).await, Err(Error::Locked { .. })));

        // Lock broken and file replaced behind Alex's back: conflict, nothing uploaded.
        av.check_out(&gear).await.unwrap();
        edit(av, "Gear.SLDPRT", b"alex's gear");
        b.vault.break_lock(&gear).await.unwrap();
        drive.external_edit(&gear, b"someone else's gear");
        let relock = crate::drive::LockInfo {
            owner: av.me.clone(),
            lock_id: "x".into(),
            locked_at: String::new(),
            machine: av.machine.clone(),
        };
        av.drive.update_metadata(&gear, av.lock_patch(&relock)).await.unwrap();
        assert!(matches!(av.push(req(vec![gear.clone()])).await, Err(Error::Conflict(_))));
        assert_eq!(std::fs::read(av.root.join("Gear.SLDPRT")).unwrap(), b"alex's gear", "local work kept");
    }

    #[tokio::test]
    async fn transient_upload_errors_leave_a_resumable_journal() {
        let (drive, gear, _robot, a, _b) = team().await;
        let av = &a.vault;
        av.check_out(&gear).await.unwrap();
        edit(av, "Gear.SLDPRT", b"gear v2");
        drive.fail_next_uploads(1);
        let req = PushRequest { files: vec![gear.clone()], message: "m".into(), keep_locked: true, ..Default::default() };
        assert!(av.push(req).await.is_err());
        assert_eq!(av.index.journal().unwrap().len(), 1);
        av.resume_pushes().await.unwrap();
        let f = drive.get_file(&gear).await.unwrap();
        assert!(av.i_hold_lock(&f) && f.read_only, "kept checked out");
        assert!(!localfs::is_read_only(&av.root.join("Gear.SLDPRT")));
    }
}
