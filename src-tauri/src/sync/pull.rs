//! Getting files from Drive onto this machine.
//!
//! The planner compares the index with local state and decides what to do;
//! the executor does it. Files that arrived in the same changeset are applied
//! together or not at all, so you never open an assembly whose parts are from
//! a different push.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde::Serialize;

use super::localfs;
use super::Vault;
use crate::drive::{props, DriveApi, RemoteFile};
use crate::error::Result;
use crate::index::{LocalState, PathMap};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PullItem {
    pub file_id: String,
    pub revision: String,
    pub rel_path: PathBuf,
    pub md5: Option<String>,
    /// Local copy was edited without a check-out; keep it as a conflict copy.
    pub keep_local_as_conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PullGroup {
    /// The changeset these revisions came from, if they came from one.
    pub changeset: Option<String>,
    pub items: Vec<PullItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum Deferred {
    /// Part of a check-in that hasn't been committed yet.
    Pending { file_id: String, changeset: String },
    /// Open in SolidWorks (or part of a group with a file that is).
    Open { file_id: String },
    /// Edited locally but no longer on Drive / no longer selected; left alone.
    ModifiedOrphan { file_id: String },
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct PullPlan {
    pub groups: Vec<PullGroup>,
    /// (file id, old relative path, new relative path)
    pub moves: Vec<(String, PathBuf, PathBuf)>,
    pub removals: Vec<(String, PathBuf)>,
    pub deferred: Vec<Deferred>,
}

impl PullPlan {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.moves.is_empty() && self.removals.is_empty()
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct PullReport {
    pub downloaded: usize,
    pub moved: usize,
    pub removed: usize,
    pub conflicts: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub deferred: Vec<Deferred>,
}

/// Selective sync: the nearest explicit choice on the file or an ancestor wins.
pub fn is_selected(id: &str, files: &HashMap<&str, &RemoteFile>, sel: &BTreeMap<String, bool>) -> bool {
    let mut cur = Some(id);
    while let Some(c) = cur {
        if let Some(&g) = sel.get(c) {
            return g;
        }
        cur = files.get(c).and_then(|f| f.parent.as_deref());
    }
    false
}

impl<D: DriveApi> Vault<D> {
    fn is_locally_modified(&self, l: &LocalState) -> Result<bool> {
        let fp = l.synced_size.zip(l.synced_mtime);
        localfs::is_modified(&self.abs(l.rel_path.as_ref()), l.synced_md5.as_deref(), fp)
    }

    pub fn plan_pull(&self) -> Result<PullPlan> {
        let all = self.index.all_files()?;
        let map = PathMap::build(&self.drive_id, &all);
        let by_id: HashMap<&str, &RemoteFile> = all.iter().map(|f| (f.id.as_str(), f)).collect();
        let sel = self.index.selections()?;
        let locals: HashMap<String, LocalState> =
            self.index.all_local()?.into_iter().map(|l| (l.file_id.clone(), l)).collect();
        let mut plan = PullPlan::default();

        // Files we have that moved, were removed, or were deselected.
        for l in locals.values() {
            let wanted = map.contains(&l.file_id) && is_selected(&l.file_id, &by_id, &sel);
            let old = PathBuf::from(&l.rel_path);
            if !wanted {
                if self.is_locally_modified(l)? {
                    plan.deferred.push(Deferred::ModifiedOrphan { file_id: l.file_id.clone() });
                } else if localfs::is_open_in_cad(&self.abs(&old)) {
                    plan.deferred.push(Deferred::Open { file_id: l.file_id.clone() });
                } else {
                    plan.removals.push((l.file_id.clone(), old));
                }
                continue;
            }
            let new = map.path(&l.file_id).expect("contains checked").to_path_buf();
            if new != old {
                if localfs::is_open_in_cad(&self.abs(&old)) {
                    plan.deferred.push(Deferred::Open { file_id: l.file_id.clone() });
                } else {
                    plan.moves.push((l.file_id.clone(), old, new));
                }
            }
        }

        // Files that need downloading, grouped by the changeset that made them.
        let mut groups: BTreeMap<String, Vec<PullItem>> = BTreeMap::new();
        let mut loose = Vec::new();
        for f in &all {
            if f.is_folder() || !map.contains(&f.id) || !is_selected(&f.id, &by_id, &sel) {
                continue;
            }
            let Some(rev) = f.head_revision_id.clone() else { continue };
            let local = locals.get(&f.id);
            if local.and_then(|l| l.synced_rev.as_deref()) == Some(rev.as_str()) {
                continue;
            }
            if let Some(cs) = f.prop(props::PENDING_CHANGESET) {
                if !self.index.has_changeset(cs)? {
                    plan.deferred.push(Deferred::Pending { file_id: f.id.clone(), changeset: cs.to_string() });
                    continue;
                }
            }
            let mut conflict = false;
            if let Some(l) = local {
                if self.is_locally_modified(l)? {
                    if self.i_hold_lock(f) {
                        continue; // my working copy; check-in will catch any conflict
                    }
                    conflict = true;
                }
            }
            let item = PullItem {
                file_id: f.id.clone(),
                revision: rev.clone(),
                rel_path: map.path(&f.id).expect("contains checked").to_path_buf(),
                md5: f.md5.clone(),
                keep_local_as_conflict: conflict,
            };
            match self.index.changeset_for_revision(&f.id, &rev)? {
                Some(cs) => groups.entry(cs).or_default().push(item),
                None => loose.push(item),
            }
        }
        for (cs, items) in groups {
            plan.groups.push(PullGroup { changeset: Some(cs), items });
        }
        for item in loose {
            plan.groups.push(PullGroup { changeset: None, items: vec![item] });
        }

        // A group with any file open in SolidWorks waits as a whole.
        let mut ready = Vec::new();
        for g in plan.groups.drain(..) {
            let open: Vec<_> = g
                .items
                .iter()
                .filter(|i| localfs::is_open_in_cad(&self.abs(&i.rel_path)))
                .collect();
            if open.is_empty() {
                ready.push(g);
            } else {
                plan.deferred.extend(g.items.iter().map(|i| Deferred::Open { file_id: i.file_id.clone() }));
            }
        }
        plan.groups = ready;
        Ok(plan)
    }

    pub async fn apply_pull(&self, plan: PullPlan) -> Result<PullReport> {
        let mut report = PullReport { deferred: plan.deferred.clone(), ..Default::default() };

        for (id, old, new) in &plan.moves {
            let (from, to) = (self.abs(old), self.abs(new));
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)?;
            }
            match std::fs::rename(&from, &to) {
                Ok(()) => {
                    if let Some(mut l) = self.index.local(id)? {
                        l.rel_path = new.to_string_lossy().to_string();
                        self.index.set_local(&l)?;
                    }
                    report.moved += 1;
                }
                Err(e) => report.errors.push(format!("move {}: {e}", old.display())),
            }
        }

        for g in &plan.groups {
            match self.apply_group(g, &mut report).await {
                Ok(n) => report.downloaded += n,
                Err(e) => report.errors.push(format!(
                    "{} not applied: {e}",
                    g.changeset.as_deref().map_or("file".to_string(), |c| format!("changeset {c}"))
                )),
            }
        }

        for (id, rel) in &plan.removals {
            let p = self.abs(rel);
            if p.exists() {
                localfs::move_to_trash(&p, &self.trash_dir())?;
            }
            self.index.remove_local(id)?;
            report.removed += 1;
        }
        Ok(report)
    }

    /// Download every file in the group to staging, verify, then swap them all in.
    async fn apply_group(&self, g: &PullGroup, report: &mut PullReport) -> Result<usize> {
        let stage = self.staging_dir().join(ulid::Ulid::new().to_string());
        std::fs::create_dir_all(&stage)?;
        let result = self.stage_and_swap(g, &stage, report).await;
        let _ = std::fs::remove_dir_all(&stage);
        result
    }

    async fn stage_and_swap(&self, g: &PullGroup, stage: &std::path::Path, report: &mut PullReport) -> Result<usize> {
        let mut staged = Vec::new();
        for (n, item) in g.items.iter().enumerate() {
            let tmp = stage.join(n.to_string());
            self.drive.download_revision(&item.file_id, &item.revision, &tmp).await?;
            if let Some(expected) = &item.md5 {
                let got = localfs::md5_file(&tmp)?;
                if &got != expected {
                    return Err(crate::error::Error::Conflict(format!(
                        "{} changed while downloading",
                        item.rel_path.display()
                    )));
                }
            }
            staged.push((item, tmp));
        }
        for (item, tmp) in &staged {
            let dest = self.abs(&item.rel_path);
            if item.keep_local_as_conflict && dest.exists() {
                let copy = localfs::conflict_name(&dest, &self.me);
                localfs::set_read_only(&dest, false)?;
                std::fs::rename(&dest, &copy)?;
                report.conflicts.push(copy);
            }
            localfs::replace(tmp, &dest)?;
            let f = self.index.file(&item.file_id)?;
            let mine = f.as_ref().is_some_and(|f| self.i_hold_lock(f));
            localfs::set_read_only(&dest, !mine)?;
            let fp = localfs::fingerprint(&dest);
            self.index.set_local(&LocalState {
                file_id: item.file_id.clone(),
                rel_path: item.rel_path.to_string_lossy().to_string(),
                synced_rev: Some(item.revision.clone()),
                synced_md5: item.md5.clone(),
                synced_size: fp.map(|f| f.0),
                synced_mtime: fp.map(|f| f.1),
            })?;
        }
        Ok(staged.len())
    }

    /// Refresh the index and get everything selected.
    pub async fn sync_down(&self) -> Result<PullReport> {
        self.refresh().await?;
        let plan = self.plan_pull()?;
        self.apply_pull(plan).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::sync::testutil::vault;

    #[tokio::test]
    async fn pulls_selected_files_read_only() {
        let drive = FakeDrive::new("a@team.org");
        let root = drive.drive_id.clone();
        let dt = drive.seed_folder(&root, "Drivetrain");
        let gear = drive.seed_file(&dt, "Gear.SLDPRT", b"g1");
        let other = drive.seed_folder(&root, "Climber");
        drive.seed_file(&other, "Hook.SLDPRT", b"h1");

        let fx = vault(&drive, "b@team.org", "pc-b");
        let v = &fx.vault;
        v.refresh().await.unwrap();
        v.index.set_selection(&dt, Some(true)).unwrap();
        let r = v.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 1, "only the selected folder");
        let p = v.root.join("Drivetrain/Gear.SLDPRT");
        assert_eq!(std::fs::read(&p).unwrap(), b"g1");
        assert!(localfs::is_read_only(&p), "not checked out, so read-only");

        drive.external_edit(&gear, b"g2");
        let r = v.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 1);
        assert_eq!(std::fs::read(&p).unwrap(), b"g2");
        assert!(v.sync_down().await.unwrap().downloaded == 0, "up to date");

        // Deselect: the local copy moves to the app's trash folder.
        v.index.set_selection(&dt, Some(false)).unwrap();
        let r = v.sync_down().await.unwrap();
        assert_eq!(r.removed, 1);
        assert!(!p.exists());
        assert_eq!(std::fs::read_dir(v.trash_dir()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn defers_files_open_in_solidworks_and_keeps_conflicts() {
        let drive = FakeDrive::new("a@team.org");
        let root = drive.drive_id.clone();
        let gear = drive.seed_file(&root, "Gear.SLDPRT", b"g1");
        let fx = vault(&drive, "b@team.org", "pc-b");
        let v = &fx.vault;
        v.index.set_selection(&root, Some(true)).unwrap();
        v.sync_down().await.unwrap();
        let p = v.root.join("Gear.SLDPRT");

        drive.external_edit(&gear, b"g2");
        let marker = v.root.join("~$Gear.SLDPRT");
        std::fs::write(&marker, b"").unwrap();
        let r = v.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 0);
        assert!(matches!(r.deferred[0], Deferred::Open { .. }));
        std::fs::remove_file(&marker).unwrap();

        // Someone cleared read-only and edited without checking out.
        localfs::set_read_only(&p, false).unwrap();
        std::fs::write(&p, b"my unsaved idea").unwrap();
        let r = v.sync_down().await.unwrap();
        assert_eq!(r.downloaded, 1);
        assert_eq!(std::fs::read(&p).unwrap(), b"g2");
        assert_eq!(std::fs::read(&r.conflicts[0]).unwrap(), b"my unsaved idea");
    }

    #[test]
    fn selection_inherits_from_nearest_ancestor() {
        let mk = |id: &str, parent: Option<&str>| RemoteFile {
            id: id.into(),
            name: id.into(),
            parent: parent.map(Into::into),
            mime_type: String::new(),
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: String::new(),
            modified_time: String::new(),
            trashed: false,
            read_only: false,
            app_properties: Default::default(),
            last_modifier: None,
        };
        let files = [mk("a", Some("root")), mk("b", Some("a")), mk("c", Some("b"))];
        let by_id: HashMap<&str, &RemoteFile> = files.iter().map(|f| (f.id.as_str(), f)).collect();
        let mut sel = BTreeMap::new();
        assert!(!is_selected("c", &by_id, &sel));
        sel.insert("a".to_string(), true);
        assert!(is_selected("c", &by_id, &sel));
        sel.insert("b".to_string(), false);
        assert!(!is_selected("c", &by_id, &sel));
        assert!(is_selected("a", &by_id, &sel));
    }
}
