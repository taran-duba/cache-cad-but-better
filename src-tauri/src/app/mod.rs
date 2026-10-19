//! App state shared by the Tauri commands, plus the background sync loop.
//!
//! Sign-in is two steps: a Google account (`auth`), then a shared drive
//! (`vault`). In between, the UI shows the list of shared drives to pick from.

pub mod commands;
pub mod config;
mod demo;
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

pub struct AppState {
    pub auth: RwLock<Option<Arc<Auth>>>,
    pub account: RwLock<Option<String>>,
    pub vault: RwLock<Option<Arc<AppVault>>>,
    pub drive_name: RwLock<Option<String>>,
    pub team: team::TeamCache,
    pub config: RwLock<AppConfig>,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub demo: std::sync::atomic::AtomicBool,
    /// Serializes sync passes so the poller and a manual "Sync now" never overlap.
    pub sync_lock: tokio::sync::Mutex<()>,
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
        Self {
            auth: RwLock::new(None),
            account: RwLock::new(None),
            vault: RwLock::new(None),
            drive_name: RwLock::new(None),
            team: Default::default(),
            config: RwLock::new(AppConfig::load(&config_path)),
            config_path,
            data_dir,
            demo: Default::default(),
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
        *self.auth.write().await = Some(auth);
        *self.account.write().await = Some(email);
        if let Some(drive_id) = self.config.read().await.drive_id.clone() {
            if let Err(e) = self.open_drive(&drive_id).await {
                // E.g. a different account without access: let them pick again.
                tracing::warn!(%e, "saved shared drive could not be opened");
            }
        }
        Ok(())
    }

    /// Open a shared drive as the vault, and remember it for next time.
    pub async fn open_drive(&self, drive_id: &str) -> Result<()> {
        let auth = self.auth.read().await.clone().ok_or_else(|| Error::Auth("sign in with Google first".into()))?;
        let me = self.account.read().await.clone().unwrap_or_default();
        let drive = Backend::Http(HttpDrive::new(auth));
        let name = drive.get_file(drive_id).await?.name;

        let mut cfg = self.config.read().await.clone();
        let root = match (&cfg.drive_id, &cfg.root) {
            (Some(saved), Some(root)) if saved == drive_id => root.clone(),
            _ => config::default_root(&name),
        };
        let vault_data = self.data_dir.join(drive_id);
        std::fs::create_dir_all(&vault_data)?;
        std::fs::create_dir_all(&root)?;
        let vault = Vault {
            drive,
            index: Index::open(&vault_data.join("index.sqlite"))?,
            drive_id: drive_id.to_string(),
            root: root.clone(),
            data_dir: vault_data,
            me,
            machine: machine_name(),
            lock_verify_delay: Duration::from_secs(2),
        };

        cfg.drive_id = Some(drive_id.to_string());
        cfg.root = Some(root);
        cfg.save(&self.config_path)?;
        *self.config.write().await = cfg;
        *self.vault.write().await = Some(Arc::new(vault));
        *self.drive_name.write().await = Some(name);
        self.team.clear().await;
        self.demo.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub async fn sign_out(&self) -> Result<()> {
        Auth::sign_out()?;
        *self.auth.write().await = None;
        *self.account.write().await = None;
        *self.vault.write().await = None;
        *self.drive_name.write().await = None;
        self.team.clear().await;
        self.demo.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub async fn open_demo(&self) -> Result<()> {
        let dir = self.data_dir.join("demo");
        let vault = demo::build(&dir).await?;
        *self.account.write().await = Some(vault.me.clone());
        *self.vault.write().await = Some(Arc::new(vault));
        *self.drive_name.write().await = Some("Demo Team Drive".into());
        self.team.clear().await;
        self.demo.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// One sync pass: finish interrupted check-ins, refresh, get files.
    pub async fn sync_once(&self) -> Result<crate::sync::pull::PullReport> {
        let _guard = self.sync_lock.lock().await;
        let vault = self.vault().await?;
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
            if state.vault.read().await.is_some() {
                match state.sync_once().await {
                    Ok(report) => {
                        let _ = app.emit("sync-report", &report);
                    }
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
            if let Some(auth) = Auth::from_saved(client) {
                if let Err(e) = state.set_auth(Arc::new(auth)).await {
                    tracing::warn!(%e, "saved sign-in could not be restored");
                }
            }
        }
        let _ = app.emit("vault-changed", ());
    });
}
