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
pub mod props {
    pub const LOCK_OWNER: &str = "cadsyncLockOwner";
    pub const LOCK_ID: &str = "cadsyncLockId";
    pub const LOCKED_AT: &str = "cadsyncLockedAt";
    pub const LOCK_MACHINE: &str = "cadsyncLockMachine";
    pub const PENDING_CHANGESET: &str = "cadsyncPending";
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

    pub fn prop(&self, key: &str) -> Option<&str> {
        self.app_properties.get(key).map(String::as_str)
    }

    pub fn lock(&self) -> Option<LockInfo> {
        Some(LockInfo {
            owner: self.prop(props::LOCK_OWNER)?.to_string(),
            lock_id: self.prop(props::LOCK_ID)?.to_string(),
            locked_at: self.prop(props::LOCKED_AT).unwrap_or_default().to_string(),
            machine: self.prop(props::LOCK_MACHINE).unwrap_or_default().to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockInfo {
    pub owner: String,
    pub lock_id: String,
    pub locked_at: String,
    pub machine: String,
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

    pub fn unset(mut self, key: &str) -> Self {
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
    /// Who is signed in (email address).
    async fn whoami(&self) -> Result<String>;
    /// One page of every non-trashed item in the shared drive.
    async fn list_drive_files(&self, drive_id: &str, page_token: Option<&str>) -> Result<FilePage>;
    async fn start_page_token(&self, drive_id: &str) -> Result<String>;
    async fn list_changes(&self, drive_id: &str, page_token: &str) -> Result<ChangePage>;
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
    async fn whoami(&self) -> Result<String> {
        delegate!(self, d => d.whoami().await)
    }
    async fn list_drive_files(&self, drive_id: &str, page_token: Option<&str>) -> Result<FilePage> {
        delegate!(self, d => d.list_drive_files(drive_id, page_token).await)
    }
    async fn start_page_token(&self, drive_id: &str) -> Result<String> {
        delegate!(self, d => d.start_page_token(drive_id).await)
    }
    async fn list_changes(&self, drive_id: &str, page_token: &str) -> Result<ChangePage> {
        delegate!(self, d => d.list_changes(drive_id, page_token).await)
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
