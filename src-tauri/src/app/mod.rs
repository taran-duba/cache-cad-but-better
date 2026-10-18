//! App state shared by the Tauri commands, plus the background sync loop.

pub mod commands;
pub mod config;
mod demo;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

use crate::drive::auth::Auth;
use crate::drive::http::HttpDrive;
use crate::drive::{Backend, DriveApi};
use crate::error::{Error, Result};
use crate::index::Index;
use crate::sync::Vault;
use config::AppConfig;

pub type AppVault = Vault<Backend>;

pub struct AppState {
    pub vault: RwLock<Option<Arc<AppVault>>>,
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
            vault: RwLock::new(None),
            config: RwLock::new(AppConfig::load(&config_path)),
            config_path,
            data_dir,
            demo: Default::default(),
            sync_lock: Default::default(),
        }
    }

    pub async fn vault(&self) -> Result<Arc<AppVault>> {
        self.vault
            .read()
            .await
            .clone()
            .ok_or_else(|| Error::Auth("sign in and choose a shared drive first".into()))
    }

    /// Open the vault against real Drive using a saved or fresh sign-in.
    pub async fn open_http(&self, auth: Auth) -> Result<()> {
        let cfg = self.config.read().await.clone();
        let drive_id = cfg.drive_id.ok_or_else(|| Error::Config("shared drive id".into()))?;
        let drive = Backend::Http(HttpDrive::new(Arc::new(auth)));
        let me = drive.whoami().await?;
        let root_name = drive.get_file(&drive_id).await.map(|f| f.name).unwrap_or_else(|_| "Team Drive".into());
        let root = cfg.root.unwrap_or_else(|| config::default_root(&root_name));
        let vault_data = self.data_dir.join(&drive_id);
        std::fs::create_dir_all(&vault_data)?;
        std::fs::create_dir_all(&root)?;
        let vault = Vault {
            drive,
            index: Index::open(&vault_data.join("index.sqlite"))?,
            drive_id,
            root,
            data_dir: vault_data,
            me,
            machine: machine_name(),
            lock_verify_delay: Duration::from_secs(2),
        };
        *self.vault.write().await = Some(Arc::new(vault));
        self.demo.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub async fn open_demo(&self) -> Result<()> {
        let dir = self.data_dir.join("demo");
        let vault = demo::build(&dir).await?;
        *self.vault.write().await = Some(Arc::new(vault));
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

/// Background loop: sync every `poll_seconds`, tell the UI what happened.
pub fn spawn_poller(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let secs = state.config.read().await.poll_seconds.max(5);
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
            tokio::time::sleep(Duration::from_secs(secs)).await;
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
        } else {
            let cfg = state.config.read().await.clone();
            if let (Some(oauth), Some(_)) = (cfg.oauth, cfg.drive_id) {
                if let Some(auth) = Auth::from_saved(oauth) {
                    if let Err(e) = state.open_http(auth).await {
                        tracing::warn!(%e, "saved session could not be restored");
                    }
                }
            }
        }
        let _ = app.emit("vault-changed", ());
    });
}
