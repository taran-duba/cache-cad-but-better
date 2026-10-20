//! In-memory Drive used by tests and demo mode. Several `FakeDrive` handles
//! can share one state with different signed-in users (`as_user`), which is
//! how tests simulate two teammates racing for the same file.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use md5::{Digest, Md5};

use super::*;
use crate::error::{Error, Result};

#[derive(Default)]
struct State {
    files: BTreeMap<String, RemoteFile>,
    revisions: HashMap<String, Vec<(String, Vec<u8>)>>,
    kept_forever: Vec<(String, String)>,
    /// (sequence number, file id) — the change feed.
    changes: Vec<(u64, String)>,
    seq: u64,
    next_id: u64,
    /// Fail this many upcoming uploads with a 503 (fault injection).
    fail_uploads: usize,
    /// Fail the upload after this many more successful ones (simulated crash mid-push).
    crash_after_uploads: Option<usize>,
}

#[derive(Clone)]
pub struct FakeDrive {
    state: Arc<Mutex<State>>,
    user: String,
    pub drive_id: String,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn md5_hex(bytes: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

impl FakeDrive {
    pub fn new(user: &str) -> Self {
        let drive = Self {
            state: Arc::default(),
            user: user.to_string(),
            drive_id: "drive-root".to_string(),
        };
        let root = RemoteFile {
            id: drive.drive_id.clone(),
            name: "Team Drive".into(),
            parent: None,
            mime_type: FOLDER_MIME.into(),
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: now(),
            modified_time: now(),
            trashed: false,
            read_only: false,
            app_properties: BTreeMap::new(),
            last_modifier: None,
        };
        drive.state.lock().unwrap().files.insert(root.id.clone(), root);
        drive
    }

    /// Another handle on the same drive, signed in as someone else.
    pub fn as_user(&self, user: &str) -> Self {
        Self { state: self.state.clone(), user: user.to_string(), drive_id: self.drive_id.clone() }
    }

    pub fn fail_next_uploads(&self, n: usize) {
        self.state.lock().unwrap().fail_uploads = n;
    }

    pub fn crash_after_uploads(&self, n: usize) {
        self.state.lock().unwrap().crash_after_uploads = Some(n);
    }

    pub fn clear_faults(&self) {
        let mut s = self.state.lock().unwrap();
        s.fail_uploads = 0;
        s.crash_after_uploads = None;
    }

    pub fn is_kept_forever(&self, file_id: &str, rev: &str) -> bool {
        self.state.lock().unwrap().kept_forever.contains(&(file_id.into(), rev.into()))
    }

    pub fn content(&self, file_id: &str) -> Option<Vec<u8>> {
        let s = self.state.lock().unwrap();
        s.revisions.get(file_id)?.last().map(|(_, b)| b.clone())
    }

    /// Seed helpers for tests and demo data.
    pub fn seed_folder(&self, parent: &str, name: &str) -> String {
        let mut s = self.state.lock().unwrap();
        let f = Self::new_entry(&mut s, parent, name, FOLDER_MIME, &self.user);
        let id = f.id.clone();
        Self::commit_change(&mut s, f);
        id
    }

    pub fn seed_file(&self, parent: &str, name: &str, bytes: &[u8]) -> String {
        let mut s = self.state.lock().unwrap();
        let mut f = Self::new_entry(&mut s, parent, name, "application/octet-stream", &self.user);
        let id = f.id.clone();
        Self::add_revision(&mut s, &mut f, bytes.to_vec());
        Self::commit_change(&mut s, f);
        id
    }

    /// Move a file or folder (like dragging it in the Drive website).
    pub fn move_item(&self, id: &str, new_parent: &str) {
        let mut s = self.state.lock().unwrap();
        let mut f = s.files.get(id).cloned().expect("item exists");
        f.parent = Some(new_parent.into());
        Self::commit_change(&mut s, f);
    }

    /// Simulate someone editing a file outside this app (e.g. Drive web upload).
    pub fn external_edit(&self, file_id: &str, bytes: &[u8]) {
        let mut s = self.state.lock().unwrap();
        let mut f = s.files.get(file_id).cloned().expect("file exists");
        Self::add_revision(&mut s, &mut f, bytes.to_vec());
        Self::commit_change(&mut s, f);
    }

    fn new_entry(s: &mut State, parent: &str, name: &str, mime: &str, user: &str) -> RemoteFile {
        s.next_id += 1;
        RemoteFile {
            id: format!("f{}", s.next_id),
            name: name.into(),
            parent: Some(parent.into()),
            mime_type: mime.into(),
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: format!("2026-01-01T00:00:{:02}.{:06}Z", s.next_id % 60, s.next_id),
            modified_time: now(),
            trashed: false,
            read_only: false,
            app_properties: BTreeMap::new(),
            last_modifier: Some(user.into()),
        }
    }

    fn add_revision(s: &mut State, f: &mut RemoteFile, bytes: Vec<u8>) {
        let revs = s.revisions.entry(f.id.clone()).or_default();
        let rev = format!("{}-r{}", f.id, revs.len() + 1);
        f.md5 = Some(md5_hex(&bytes));
        f.size = Some(bytes.len() as u64);
        f.head_revision_id = Some(rev.clone());
        f.modified_time = now();
        revs.push((rev, bytes));
    }

    fn commit_change(s: &mut State, f: RemoteFile) {
        s.seq += 1;
        let seq = s.seq;
        s.changes.push((seq, f.id.clone()));
        s.files.insert(f.id.clone(), f);
    }

    fn get(s: &State, id: &str) -> Result<RemoteFile> {
        s.files.get(id).cloned().ok_or_else(|| Error::NotFound(id.into()))
    }

    fn check_upload_faults(s: &mut State) -> Result<()> {
        if s.fail_uploads > 0 {
            s.fail_uploads -= 1;
            return Err(Error::Drive { status: 503, message: "backendError".into() });
        }
        if let Some(n) = s.crash_after_uploads.as_mut() {
            if *n == 0 {
                return Err(Error::Network("simulated crash".into()));
            }
            *n -= 1;
        }
        Ok(())
    }
}

impl DriveApi for FakeDrive {
    async fn whoami(&self) -> Result<String> {
        Ok(self.user.clone())
    }

    async fn folder_info(&self, id: &str) -> Result<FolderInfo> {
        let s = self.state.lock().unwrap();
        let f = Self::get(&s, id)?;
        Ok(FolderInfo { is_folder: f.is_folder(), id: f.id, name: f.name, drive_id: None })
    }

    async fn list_children(&self, _scope: Option<&str>, parents: &[String], page_token: Option<&str>) -> Result<FilePage> {
        // Page size 2 so tests exercise pagination.
        let s = self.state.lock().unwrap();
        let all: Vec<_> = s
            .files
            .values()
            .filter(|f| !f.trashed && f.parent.as_ref().is_some_and(|p| parents.contains(p)))
            .cloned()
            .collect();
        let start: usize = page_token.map(|t| t.parse().unwrap_or(0)).unwrap_or(0);
        let end = (start + 2).min(all.len());
        Ok(FilePage {
            files: all[start..end].to_vec(),
            next_page_token: (end < all.len()).then(|| end.to_string()),
        })
    }

    async fn start_page_token(&self, _scope: Option<&str>) -> Result<String> {
        Ok(self.state.lock().unwrap().seq.to_string())
    }

    async fn list_changes(&self, _scope: Option<&str>, page_token: &str) -> Result<ChangePage> {
        let s = self.state.lock().unwrap();
        let after: u64 = page_token.parse().unwrap_or(0);
        let mut seen = std::collections::HashSet::new();
        let mut changes = Vec::new();
        for (seq, id) in s.changes.iter().rev() {
            if *seq <= after {
                break;
            }
            if seen.insert(id.clone()) {
                let file = s.files.get(id).cloned();
                let removed = file.as_ref().is_none_or(|f| f.trashed);
                changes.push(Change { file_id: id.clone(), removed, file });
            }
        }
        changes.reverse();
        Ok(ChangePage { changes, next_page_token: None, new_start_page_token: Some(s.seq.to_string()) })
    }

    async fn get_file(&self, file_id: &str) -> Result<RemoteFile> {
        Self::get(&self.state.lock().unwrap(), file_id)
    }

    async fn download_file(&self, file_id: &str, dest: &Path) -> Result<()> {
        let bytes = self.content(file_id).ok_or_else(|| Error::NotFound(file_id.into()))?;
        std::fs::write(dest, bytes)?;
        Ok(())
    }

    async fn download_revision(&self, file_id: &str, revision_id: &str, dest: &Path) -> Result<()> {
        let bytes = {
            let s = self.state.lock().unwrap();
            s.revisions
                .get(file_id)
                .and_then(|r| r.iter().find(|(id, _)| id == revision_id))
                .map(|(_, b)| b.clone())
                .ok_or_else(|| Error::NotFound(format!("{file_id}@{revision_id}")))?
        };
        std::fs::write(dest, bytes)?;
        Ok(())
    }

    async fn upload_content(&self, file_id: &str, src: &Path) -> Result<RemoteFile> {
        let bytes = std::fs::read(src)?;
        let mut s = self.state.lock().unwrap();
        Self::check_upload_faults(&mut s)?;
        let mut f = Self::get(&s, file_id)?;
        if f.read_only {
            // What phase 0 must confirm real Drive does for binary files.
            return Err(Error::Drive { status: 403, message: "contentRestricted".into() });
        }
        Self::add_revision(&mut s, &mut f, bytes);
        f.last_modifier = Some(self.user.clone());
        Self::commit_change(&mut s, f.clone());
        Ok(f)
    }

    async fn create_file(
        &self,
        parent_id: &str,
        name: &str,
        src: &Path,
        app_properties: BTreeMap<String, String>,
    ) -> Result<RemoteFile> {
        let bytes = std::fs::read(src)?;
        let mut s = self.state.lock().unwrap();
        Self::check_upload_faults(&mut s)?;
        Self::get(&s, parent_id)?;
        let mut f = Self::new_entry(&mut s, parent_id, name, "application/octet-stream", &self.user);
        f.app_properties = app_properties;
        Self::add_revision(&mut s, &mut f, bytes);
        Self::commit_change(&mut s, f.clone());
        Ok(f)
    }

    async fn create_folder(&self, parent_id: &str, name: &str) -> Result<RemoteFile> {
        let mut s = self.state.lock().unwrap();
        Self::get(&s, parent_id)?;
        let f = Self::new_entry(&mut s, parent_id, name, FOLDER_MIME, &self.user);
        Self::commit_change(&mut s, f.clone());
        Ok(f)
    }

    async fn update_metadata(&self, file_id: &str, patch: MetadataPatch) -> Result<RemoteFile> {
        let mut s = self.state.lock().unwrap();
        let mut f = Self::get(&s, file_id)?;
        for (k, v) in patch.app_properties {
            match v {
                Some(v) => f.app_properties.insert(k, v),
                None => f.app_properties.remove(&k),
            };
        }
        if let Some((on, _reason)) = patch.read_only {
            f.read_only = on;
        }
        Self::commit_change(&mut s, f.clone());
        Ok(f)
    }

    async fn keep_revision_forever(&self, file_id: &str, revision_id: &str) -> Result<()> {
        self.state.lock().unwrap().kept_forever.push((file_id.into(), revision_id.into()));
        Ok(())
    }

    async fn trash_file(&self, file_id: &str) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        let mut f = Self::get(&s, file_id)?;
        f.trashed = true;
        Self::commit_change(&mut s, f);
        Ok(())
    }
}
