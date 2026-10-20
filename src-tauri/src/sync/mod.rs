//! Sync engine: keeping the index current (`crawl`), getting files (`pull`),
//! check-out locks (`locks`) and atomic check-ins (`push`).

pub mod crawl;
pub mod localfs;
pub mod locks;
pub mod pull;
pub mod push;
pub mod remove;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::drive::{DriveApi, RemoteFile};
use crate::error::Result;
use crate::index::{Index, PathMap};

/// Everything the sync engine needs for one vault (one shared drive).
pub struct Vault<D> {
    pub drive: D,
    pub index: Index,
    /// The vault folder: the root of everything cadsync syncs. Any folder in
    /// Drive (a shared drive's root, or a folder deep inside one, or in My Drive).
    pub root_id: String,
    /// The shared drive the folder is in, if any (speeds up listing).
    pub scope: Option<String>,
    /// Local vault root, e.g. `C:\CAD\Team Drive`. Must be the same path on
    /// every machine so SolidWorks' absolute references resolve.
    pub root: PathBuf,
    /// App data folder for staging, trash and scratch files (outside the vault
    /// so SolidWorks' reference search never finds half-downloaded files).
    pub data_dir: PathBuf,
    pub me: String,
    pub machine: String,
    /// How long to wait before re-reading a lock to detect a race.
    pub lock_verify_delay: Duration,
}

impl<D: DriveApi> Vault<D> {
    pub fn paths(&self) -> Result<PathMap> {
        Ok(PathMap::build(&self.root_id, &self.index.all_files()?))
    }

    pub fn abs(&self, rel: &Path) -> PathBuf {
        self.root.join(rel)
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.data_dir.join("staging")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.data_dir.join("trash")
    }

    pub fn scratch_dir(&self) -> PathBuf {
        self.data_dir.join("scratch")
    }

    /// Checked out by me, on this machine.
    pub fn i_hold_lock(&self, f: &RemoteFile) -> bool {
        f.lock().is_some_and(|l| l.owner == self.me && l.machine == self.machine)
    }

    pub async fn refresh(&self) -> Result<crawl::CrawlStats> {
        tracing::debug!(scope = ?self.scope, "refresh: checking Drive for changes");
        crawl::poll_changes(&self.drive, &self.index, self.scope.as_deref(), &self.root_id, &self.scratch_dir()).await
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::drive::fake::FakeDrive;

    pub struct Fixture {
        pub vault: Vault<FakeDrive>,
        _dirs: (tempfile::TempDir, tempfile::TempDir),
    }

    /// A vault on its own temp root and data dir, sharing `drive`'s state.
    pub fn vault(drive: &FakeDrive, user: &str, machine: &str) -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let vault = Vault {
            drive: drive.as_user(user),
            index: Index::in_memory().unwrap(),
            root_id: drive.drive_id.clone(),
            scope: None,
            root: root.path().to_path_buf(),
            data_dir: data.path().to_path_buf(),
            me: user.to_string(),
            machine: machine.to_string(),
            lock_verify_delay: Duration::from_millis(0),
        };
        Fixture { vault, _dirs: (root, data) }
    }
}
