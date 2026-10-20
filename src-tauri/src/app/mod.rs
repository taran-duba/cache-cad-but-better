//! App state shared by the Tauri commands, plus the background sync loop.
//!
//! Sign-in is two steps: a Google account (`auth`), then a shared drive
//! (`vault`). In between, the UI shows the list of shared drives to pick from.

pub mod commands;
pub mod config;
mod demo;
pub mod logging;
pub mod smoke;
pub mod team;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

use crate::drive::auth::{Auth, OAuthClient};
use crate::drive::http::HttpDrive;
use crate::drive::{Backend, DriveApi};
use crate::error::{Error, Result};
use crate::index::Index;
use crate::sync::Vault;
use config::AppConfig;

pub type AppVault = Vault<Backend>;

const POLL: Duration = Duration::from_secs(15);
/// A whole sync pass (refresh + downloads) must finish within this, so one
/// stuck request can't hold the sync lock and block Sync now and check-ins.
const SYNC_PASS_LIMIT: Duration = Duration::from_secs(300);

pub struct AppState {
    pub auth: RwLock<Option<Arc<Auth>>>,
    pub account: RwLock<Option<String>>,
    pub vault: RwLock<Option<Arc<AppVault>>>,
    pub folder_name: RwLock<Option<String>>,
    pub team: team::TeamCache,
    pub config: RwLock<AppConfig>,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub demo: std::sync::atomic::AtomicBool,
    /// True while the saved sign-in is being restored at startup (the system
    /// keychain may be asking for permission), so the window says so instead of
    /// showing a Sign in button that would race it.
    pub restoring: std::sync::atomic::AtomicBool,
    /// Set when Google rejected the saved sign-in; the welcome screen says so.
    pub sign_in_expired: std::sync::atomic::AtomicBool,
    /// Serializes sync passes so the poller and a manual "Sync now" never overlap.
    pub sync_lock: tokio::sync::Mutex<()>,
}

/// Where the log file and crash reports go.
pub fn log_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_log_dir()
        .or_else(|_| app.path().app_data_dir().map(|d| d.join("logs")))
        .unwrap_or_else(|_| PathBuf::from("cadsync-logs"))
}

fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "this-computer".into())
}

impl AppState {
    pub fn new(app: &tauri::AppHandle) -> Self {
        let data_dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from(".cadsync-data"));
        let config_path = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| data_dir.clone())
            .join("config.json");
        Self::with_paths(data_dir, config_path)
    }

    pub fn with_paths(data_dir: PathBuf, config_path: PathBuf) -> Self {
        Self {
            auth: RwLock::new(None),
            account: RwLock::new(None),
            vault: RwLock::new(None),
            folder_name: RwLock::new(None),
            team: Default::default(),
            config: RwLock::new(AppConfig::load(&config_path)),
            config_path,
            data_dir,
            demo: Default::default(),
            restoring: Default::default(),
            sign_in_expired: Default::default(),
            sync_lock: Default::default(),
        }
    }

    /// The Google OAuth client: built into the app, or an override from config.
    pub async fn oauth_client(&self) -> Option<OAuthClient> {
        self.config.read().await.oauth.clone().or_else(OAuthClient::built_in)
    }

    pub async fn vault(&self) -> Result<Arc<AppVault>> {
        self.vault
            .read()
            .await
            .clone()
            .ok_or_else(|| Error::Auth("sign in and choose a shared drive first".into()))
    }

    pub async fn http(&self) -> Result<HttpDrive> {
        let auth = self.auth.read().await.clone().ok_or_else(|| Error::Auth("sign in with Google first".into()))?;
        Ok(HttpDrive::new(auth))
    }

    /// Remember the Google sign-in, and open the saved drive if there is one.
    pub async fn set_auth(&self, auth: Arc<Auth>) -> Result<()> {
        let email = HttpDrive::new(auth.clone()).whoami().await?;
        tracing::info!("signed in to Google");
        self.sign_in_expired.store(false, std::sync::atomic::Ordering::Relaxed);
        *self.auth.write().await = Some(auth);
        *self.account.write().await = Some(email);
        let auth = self.auth.read().await.clone().ok_or_else(|| Error::Auth("sign in with Google first".into()))?;
        self.open_saved_folder_on(Backend::Http(HttpDrive::new(auth))).await;
        Ok(())
    }

    /// Open the folder chosen last time, if any, on `drive`. A folder that can't
    /// be opened (say a different account without access) just leaves the user
    /// at the picker.
    pub(crate) async fn open_saved_folder_on(&self, drive: Backend) {
        let saved = self.config.read().await.folder_id.clone();
        // `saved` is a copy: no config lock may be held while the folder opens,
        // because opening it writes the config.
        if let Some(folder_id) = saved {
            if let Err(e) = self.open_folder_on(drive, &folder_id).await {
                tracing::warn!(%e, "saved folder could not be opened");
            }
        }
    }

    /// Open a Drive folder (a shared drive's root, or any folder inside one,
    /// in My Drive, or shared with you) as the vault, and remember it.
    pub async fn open_folder(&self, folder_id: &str) -> Result<()> {
        let auth = self.auth.read().await.clone().ok_or_else(|| Error::Auth("sign in with Google first".into()))?;
        self.open_folder_on(Backend::Http(HttpDrive::new(auth)), folder_id).await
    }

    pub(crate) async fn open_folder_on(&self, drive: Backend, folder_id: &str) -> Result<()> {
        let me = self.account.read().await.clone().unwrap_or_default();
        let info = drive.folder_info(folder_id).await?;
        if !info.is_folder {
            return Err(Error::Config("that link isn't a folder".into()));
        }
        // From here on use the real id ("root" and other aliases resolve to it).
        let (folder_id, name) = (info.id, info.name);

        let mut cfg = self.config.read().await.clone();
        let root = match (&cfg.folder_id, &cfg.root) {
            (Some(saved), Some(root)) if *saved == folder_id => root.clone(),
            _ => config::default_root(&name),
        };
        let vault_data = self.data_dir.join(&folder_id);
        std::fs::create_dir_all(&vault_data)?;
        std::fs::create_dir_all(&root)?;
        let vault = Vault {
            drive,
            index: Index::open(&vault_data.join("index.sqlite"))?,
            root_id: folder_id.clone(),
            scope: info.drive_id,
            root: root.clone(),
            data_dir: vault_data,
            me,
            machine: machine_name(),
            lock_verify_delay: Duration::from_secs(2),
        };

        tracing::info!(folder = %name, shared_drive = ?vault.scope, "CAD folder opened");
        cfg.folder_id = Some(folder_id);
        cfg.root = Some(root);
        cfg.save(&self.config_path)?;
        *self.config.write().await = cfg;
        *self.vault.write().await = Some(Arc::new(vault));
        *self.folder_name.write().await = Some(name);
        self.team.clear().await;
        self.demo.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Google rejected the saved sign-in (revoked, expired, password changed):
    /// forget it and send the user back to the welcome screen. Local files and
    /// any interrupted check-in journal are kept for when they sign in again.
    pub async fn expire_sign_in(&self, app: &tauri::AppHandle) {
        tracing::warn!("Google sign-in expired or was revoked");
        let _ = self.sign_out().await;
        self.sign_in_expired.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = app.emit("vault-changed", ());
    }

    /// Pass a result through, signing out first if it says the sign-in expired.
    pub async fn check_expired<T>(&self, app: &tauri::AppHandle, r: Result<T>) -> Result<T> {
        if matches!(r, Err(Error::SignInExpired)) {
            self.expire_sign_in(app).await;
        }
        r
    }

    pub async fn sign_out(&self) -> Result<()> {
        Auth::sign_out()?;
        *self.auth.write().await = None;
        *self.account.write().await = None;
        *self.vault.write().await = None;
        *self.folder_name.write().await = None;
        self.team.clear().await;
        self.demo.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub async fn open_demo(&self) -> Result<()> {
        let dir = self.data_dir.join("demo");
        let vault = demo::build(&dir).await?;
        *self.account.write().await = Some(vault.me.clone());
        *self.vault.write().await = Some(Arc::new(vault));
        *self.folder_name.write().await = Some("Demo Team Drive".into());
        self.team.clear().await;
        self.demo.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// One sync pass: finish interrupted check-ins, refresh, get files.
    pub async fn sync_once(&self) -> Result<crate::sync::pull::PullReport> {
        tracing::debug!("sync: waiting for the sync lock");
        let _guard = self.sync_lock.lock().await;
        tracing::debug!("sync: got the sync lock");
        let started = std::time::Instant::now();
        let r = match tokio::time::timeout(SYNC_PASS_LIMIT, self.sync_pass()).await {
            Ok(r) => r,
            Err(_) => Err(Error::Network(format!(
                "sync took longer than {} minutes and was stopped; it will try again",
                SYNC_PASS_LIMIT.as_secs() / 60
            ))),
        };
        match &r {
            Ok(rep) if started.elapsed() > Duration::from_secs(10) || rep.downloaded > 0 => {
                tracing::info!(elapsed = ?started.elapsed(), downloaded = rep.downloaded, "sync pass finished")
            }
            Ok(_) => tracing::debug!(elapsed = ?started.elapsed(), "sync pass finished, nothing new"),
            Err(e) => tracing::warn!(%e, elapsed = ?started.elapsed(), "sync pass failed"),
        }
        r
    }

    async fn sync_pass(&self) -> Result<crate::sync::pull::PullReport> {
        let vault = self.vault().await?;
        match Self::sync_vault(&vault).await {
            // Drive says "not found/forbidden" for a drive-scoped listing: the user
            // may have been removed from the shared drive (or added to it). Work
            // out the right scope again, and retry once if it changed.
            Err(Error::Drive { status: 403 | 404, .. }) if vault.scope.is_some() => {
                let old = vault.scope.clone();
                self.open_folder(&vault.root_id).await?;
                let vault = self.vault().await?;
                if vault.scope == old {
                    return Err(Error::Drive { status: 404, message: "the CAD folder can no longer be read".into() });
                }
                tracing::info!(scope = ?vault.scope, "shared drive access changed; retrying");
                Self::sync_vault(&vault).await
            }
            r => r,
        }
    }

    async fn sync_vault(vault: &AppVault) -> Result<crate::sync::pull::PullReport> {
        if let Err(e) = vault.resume_pushes().await {
            tracing::warn!(%e, "could not resume an interrupted check-in yet");
        }
        vault.sync_down().await
    }
}

/// Background loop: sync every 15 s, tell the UI what happened.
pub fn spawn_poller(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            tracing::debug!("poller: tick");
            if state.vault.read().await.is_some() {
                tracing::debug!("poller: starting a sync pass");
                match state.sync_once().await {
                    Ok(report) => {
                        let _ = app.emit("sync-report", &report);
                    }
                    Err(Error::SignInExpired) => state.expire_sign_in(&app).await,
                    Err(e) => {
                        tracing::warn!(%e, "background sync failed");
                        let _ = app.emit("sync-error", e.to_string());
                    }
                }
            }
            tokio::time::sleep(POLL).await;
        }
    });
}

/// Try the saved sign-in at startup so the app opens straight to the vault.
pub fn restore_session(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if std::env::var("CADSYNC_DEMO").is_ok() {
            if let Err(e) = state.open_demo().await {
                tracing::error!(%e, "demo mode failed");
            }
        } else if let Some(client) = state.oauth_client().await {
            // The system keychain can block (it may show a permission prompt),
            // so ask it off the async workers.
            tracing::info!("restoring the saved Google sign-in");
            state.restoring.store(true, std::sync::atomic::Ordering::Relaxed);
            let saved = tauri::async_runtime::spawn_blocking(move || Auth::from_saved(client)).await.ok().flatten();
            if saved.is_none() {
                tracing::info!("no saved Google sign-in");
            }
            if let Some(auth) = saved {
                match state.set_auth(Arc::new(auth)).await {
                    Ok(()) => {}
                    Err(Error::SignInExpired) => state.expire_sign_in(&app).await,
                    Err(e) => tracing::warn!(%e, "saved sign-in could not be restored"),
                }
            }
        }
        state.restoring.store(false, std::sync::atomic::Ordering::Relaxed);
        let _ = app.emit("vault-changed", ());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;

    /// Restarting with a saved folder must open it. Regression test: the
    /// restore step once held a config read lock while opening the folder,
    /// which needs the config write lock, so the app waited on itself forever:
    /// no vault, no sync, and Sign in froze behind the stuck write.
    #[tokio::test]
    async fn restarting_with_a_saved_folder_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::with_paths(dir.path().join("data"), dir.path().join("config.json"));
        let drive = FakeDrive::new("alex@team.org");
        state.config.write().await.folder_id = Some(drive.drive_id.clone());
        *state.account.write().await = Some("alex@team.org".into());

        let opened = tokio::time::timeout(
            Duration::from_secs(5),
            state.open_saved_folder_on(Backend::Fake(drive)),
        )
        .await;
        assert!(opened.is_ok(), "opening the saved folder deadlocked");
        assert!(state.vault.read().await.is_some(), "the vault should be open");

        // And the config is still usable afterwards (Sign in reads it first).
        let client = tokio::time::timeout(Duration::from_secs(5), state.oauth_client()).await;
        assert!(client.is_ok(), "reading the config hung after opening the folder");
    }
}
