//! Google Drive access. `DriveApi` is the seam between sync logic and the
//! network: `HttpDrive` talks to Google, `FakeDrive` is an in-memory stand-in
//! used by tests and demo mode.

pub mod auth;
pub mod fake;
pub mod http;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

pub const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

/// appProperties keys. Values are visible to every client of this app.
///
/// The app used to be called cadsync, and drives written by those versions
/// carry the old `cadsync*` keys. New data is written under the `permafrost*`
/// keys; reads fall back to the old key (see `RemoteFile::prop`) and clearing
/// a key clears both (see `MetadataPatch::unset`).
pub mod props {
    pub const LOCK_OWNER: &str = "permafrostLockOwner";
    /// The owner's Google display name, for showing; `LOCK_OWNER` (the email)
    /// stays the key that ownership is compared on.
    pub const LOCK_OWNER_NAME: &str = "permafrostLockOwnerName";
    pub const LOCK_ID: &str = "permafrostLockId";
    pub const LOCKED_AT: &str = "permafrostLockedAt";
    pub const LOCK_MACHINE: &str = "permafrostLockMachine";
    pub const PENDING_CHANGESET: &str = "permafrostPending";
    /// Set on a check-in manifest file; the value is the changeset id.
    pub const MANIFEST: &str = "permafrostManifest";

    /// The pre-rename key for `key`, if it has one.
    pub fn legacy(key: &str) -> Option<&'static str> {
        Some(match key {
            LOCK_OWNER => "cadsyncLockOwner",
            LOCK_ID => "cadsyncLockId",
            LOCKED_AT => "cadsyncLockedAt",
            LOCK_MACHINE => "cadsyncLockMachine",
            PENDING_CHANGESET => "cadsyncPending",
            MANIFEST => "cadsyncManifest",
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub mime_type: String,
    pub md5: Option<String>,
    pub size: Option<u64>,
    pub head_revision_id: Option<String>,
    pub created_time: String,
    pub modified_time: String,
    pub trashed: bool,
    pub read_only: bool,
    pub app_properties: BTreeMap<String, String>,
    pub last_modifier: Option<String>,
}

impl RemoteFile {
    pub fn is_folder(&self) -> bool {
        self.mime_type == FOLDER_MIME
    }

    /// The value under `key`, or under its pre-rename key if `key` isn't set.
    pub fn prop(&self, key: &str) -> Option<&str> {
        let v = self.app_properties.get(key).or_else(|| props::legacy(key).and_then(|k| self.app_properties.get(k)));
        v.map(String::as_str)
    }

    pub fn lock(&self) -> Option<LockInfo> {
        Some(LockInfo {
            owner: self.prop(props::LOCK_OWNER)?.to_string(),
            owner_name: self.prop(props::LOCK_OWNER_NAME).unwrap_or_default().to_string(),
            lock_id: self.prop(props::LOCK_ID)?.to_string(),
            locked_at: self.prop(props::LOCKED_AT).unwrap_or_default().to_string(),
            machine: self.prop(props::LOCK_MACHINE).unwrap_or_default().to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockInfo {
    /// The owner's email: who holds the lock.
    pub owner: String,
    /// The owner's Google name, empty on locks written by older versions.
    #[serde(default)]
    pub owner_name: String,
    pub lock_id: String,
    pub locked_at: String,
    pub machine: String,
}

impl LockInfo {
    /// Who to call the owner when talking to a person.
    pub fn display_owner(&self) -> &str {
        if self.owner_name.is_empty() {
            &self.owner
        } else {
            &self.owner_name
        }
    }
}

/// The signed-in Google account. The email identifies them; the name is for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub email: String,
    /// Falls back to the email when Google doesn't give a name.
    pub name: String,
}

/// What the vault root is: any folder, in a shared drive, My Drive, or shared
/// with you. `drive_id` is set only when the folder is inside a shared drive
/// that the signed-in user is a **member** of. For a folder someone shared
/// with you from a drive you're not in, it's `None`: listing then goes through
/// the user's own view of Drive instead of the drive's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderInfo {
    /// The real folder id (an alias like "root" is resolved).
    pub id: String,
    pub name: String,
    pub is_folder: bool,
    pub drive_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FilePage {
    pub files: Vec<RemoteFile>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub file_id: String,
    pub removed: bool,
    pub file: Option<RemoteFile>,
}

#[derive(Debug, Clone)]
pub struct ChangePage {
    pub changes: Vec<Change>,
    pub next_page_token: Option<String>,
    pub new_start_page_token: Option<String>,
}

/// A partial metadata update. `None` values in `app_properties` delete the key.
#[derive(Debug, Clone, Default)]
pub struct MetadataPatch {
    pub app_properties: BTreeMap<String, Option<String>>,
    /// `Some((read_only, reason))` sets or clears the content restriction.
    pub read_only: Option<(bool, Option<String>)>,
}

impl MetadataPatch {
    pub fn set(mut self, key: &str, value: impl Into<String>) -> Self {
        self.app_properties.insert(key.to_string(), Some(value.into()));
        self
    }

    /// Delete `key` and its pre-rename key, so an old value can't resurface through the fallback in `prop`.
    pub fn unset(mut self, key: &str) -> Self {
        if let Some(old) = props::legacy(key) {
            self.app_properties.insert(old.to_string(), None);
        }
        self.app_properties.insert(key.to_string(), None);
        self
    }

    pub fn read_only(mut self, on: bool, reason: Option<String>) -> Self {
        self.read_only = Some((on, reason));
        self
    }
}

#[allow(async_fn_in_trait)]
pub trait DriveApi: Send + Sync {
    /// Who is signed in.
    async fn whoami(&self) -> Result<Account>;
    /// Resolve a folder id (or alias like "root") to its real id, name, and
    /// shared drive, if any.
    async fn folder_info(&self, id: &str) -> Result<FolderInfo>;
    /// One page of the non-trashed items directly inside any of `parents`.
    /// `scope` is the shared drive they're in, if known.
    async fn list_children(&self, scope: Option<&str>, parents: &[String], page_token: Option<&str>) -> Result<FilePage>;
    async fn start_page_token(&self, scope: Option<&str>) -> Result<String>;
    async fn list_changes(&self, scope: Option<&str>, page_token: &str) -> Result<ChangePage>;
    async fn get_file(&self, file_id: &str) -> Result<RemoteFile>;
    /// Download the current content of a file.
    async fn download_file(&self, file_id: &str, dest: &Path) -> Result<()>;
    /// Download one exact revision of a file.
    async fn download_revision(&self, file_id: &str, revision_id: &str, dest: &Path) -> Result<()>;
    /// Upload new content for an existing file. Returns the file with its new head revision.
    async fn upload_content(&self, file_id: &str, src: &Path) -> Result<RemoteFile>;
    async fn create_file(
        &self,
        parent_id: &str,
        name: &str,
        src: &Path,
        app_properties: BTreeMap<String, String>,
    ) -> Result<RemoteFile>;
    async fn create_folder(&self, parent_id: &str, name: &str) -> Result<RemoteFile>;
    async fn update_metadata(&self, file_id: &str, patch: MetadataPatch) -> Result<RemoteFile>;
    async fn keep_revision_forever(&self, file_id: &str, revision_id: &str) -> Result<()>;
    /// Move a file to the shared drive's trash (recoverable for 30 days).
    async fn trash_file(&self, file_id: &str) -> Result<()>;
}

/// The backend the app runs against: real Drive, or the in-memory demo.
pub enum Backend {
    Http(http::HttpDrive),
    Fake(fake::FakeDrive),
}

macro_rules! delegate {
    ($self:ident, $d:ident => $e:expr) => {
        match $self {
            Backend::Http($d) => $e,
            Backend::Fake($d) => $e,
        }
    };
}

impl DriveApi for Backend {
    async fn whoami(&self) -> Result<Account> {
        delegate!(self, d => d.whoami().await)
    }
    async fn folder_info(&self, id: &str) -> Result<FolderInfo> {
        delegate!(self, d => d.folder_info(id).await)
    }
    async fn list_children(&self, scope: Option<&str>, parents: &[String], page_token: Option<&str>) -> Result<FilePage> {
        delegate!(self, d => d.list_children(scope, parents, page_token).await)
    }
    async fn start_page_token(&self, scope: Option<&str>) -> Result<String> {
        delegate!(self, d => d.start_page_token(scope).await)
    }
    async fn list_changes(&self, scope: Option<&str>, page_token: &str) -> Result<ChangePage> {
        delegate!(self, d => d.list_changes(scope, page_token).await)
    }
    async fn get_file(&self, file_id: &str) -> Result<RemoteFile> {
        delegate!(self, d => d.get_file(file_id).await)
    }
    async fn download_file(&self, file_id: &str, dest: &Path) -> Result<()> {
        delegate!(self, d => d.download_file(file_id, dest).await)
    }
    async fn download_revision(&self, file_id: &str, revision_id: &str, dest: &Path) -> Result<()> {
        delegate!(self, d => d.download_revision(file_id, revision_id, dest).await)
    }
    async fn upload_content(&self, file_id: &str, src: &Path) -> Result<RemoteFile> {
        delegate!(self, d => d.upload_content(file_id, src).await)
    }
    async fn create_file(
        &self,
        parent_id: &str,
        name: &str,
        src: &Path,
        app_properties: BTreeMap<String, String>,
    ) -> Result<RemoteFile> {
        delegate!(self, d => d.create_file(parent_id, name, src, app_properties).await)
    }
    async fn create_folder(&self, parent_id: &str, name: &str) -> Result<RemoteFile> {
        delegate!(self, d => d.create_folder(parent_id, name).await)
    }
    async fn update_metadata(&self, file_id: &str, patch: MetadataPatch) -> Result<RemoteFile> {
        delegate!(self, d => d.update_metadata(file_id, patch).await)
    }
    async fn keep_revision_forever(&self, file_id: &str, revision_id: &str) -> Result<()> {
        delegate!(self, d => d.keep_revision_forever(file_id, revision_id).await)
    }
    async fn trash_file(&self, file_id: &str) -> Result<()> {
        delegate!(self, d => d.trash_file(file_id).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with(props: &[(&str, &str)]) -> RemoteFile {
        RemoteFile {
            id: "f".into(),
            name: "Gear.SLDPRT".into(),
            parent: None,
            mime_type: "x".into(),
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: String::new(),
            modified_time: String::new(),
            trashed: false,
            read_only: false,
            app_properties: props.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            last_modifier: None,
        }
    }

    #[test]
    fn reads_locks_written_under_the_old_names() {
        let f = file_with(&[
            ("cadsyncLockOwner", "alex@team.org"),
            ("cadsyncLockId", "L1"),
            ("cadsyncLockedAt", "2024-01-01T00:00:00Z"),
            ("cadsyncLockMachine", "pc-a"),
            ("cadsyncPending", "cs1"),
        ]);
        let l = f.lock().expect("legacy lock is seen");
        assert_eq!((l.owner.as_str(), l.lock_id.as_str(), l.machine.as_str()), ("alex@team.org", "L1", "pc-a"));
        assert_eq!(f.prop(props::PENDING_CHANGESET), Some("cs1"));
        assert_eq!(f.prop(props::MANIFEST), None);
    }

    #[test]
    fn new_names_win_over_old_ones() {
        let f = file_with(&[("cadsyncLockOwner", "old@team.org"), (props::LOCK_OWNER, "new@team.org"), (props::LOCK_ID, "L2")]);
        assert_eq!(f.lock().unwrap().owner, "new@team.org");
    }

    #[test]
    fn unset_clears_both_names() {
        let patch = MetadataPatch::default().unset(props::LOCK_OWNER).unset("unrelated");
        assert_eq!(patch.app_properties.get(props::LOCK_OWNER), Some(&None));
        assert_eq!(patch.app_properties.get("cadsyncLockOwner"), Some(&None));
        assert_eq!(patch.app_properties.get("unrelated"), Some(&None));
        assert_eq!(patch.app_properties.len(), 3);
    }

    #[test]
    fn set_writes_only_the_new_name() {
        let patch = MetadataPatch::default().set(props::LOCK_OWNER, "a");
        assert_eq!(patch.app_properties.keys().collect::<Vec<_>>(), [props::LOCK_OWNER]);
    }
}
