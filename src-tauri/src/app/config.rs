//! Per-machine settings, stored as JSON in the app config folder. The OAuth
//! client and Document Manager key belong to the team and are entered once
//! in Settings; they are never part of the source tree.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::drive::auth::OAuthClient;
use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub oauth: Option<OAuthClient>,
    /// Shared drive id (from its URL: drive.google.com/drive/folders/<id>).
    pub drive_id: Option<String>,
    /// Local vault root. Must be the same on every machine.
    pub root: Option<PathBuf>,
    /// Path to SwRefs.exe; defaults to the bundled sidecar.
    pub sidecar_path: Option<PathBuf>,
    /// SolidWorks Document Manager license key (from the SolidWorks customer portal).
    pub swdm_key: Option<String>,
    /// Seconds between background syncs.
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
}

fn default_poll() -> u64 {
    15
}

pub fn default_root(drive_name: &str) -> PathBuf {
    let base = if cfg!(windows) { PathBuf::from("C:\\CAD") } else { dirs_home().join("CAD") };
    base.join(crate::names::sanitize(drive_name))
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(AppConfig { poll_seconds: default_poll(), ..Default::default() })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    /// Validate values coming from the settings form.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if let Some(id) = &self.drive_id {
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err("Shared drive id should look like 0AbCdEf123… (copy it from the drive's URL)".into());
            }
        }
        if let Some(root) = &self.root {
            if !root.is_absolute() {
                return Err("Vault folder must be a full path like C:\\CAD\\Team Drive".into());
            }
        }
        if let Some(o) = &self.oauth {
            if !o.client_id.ends_with(".apps.googleusercontent.com") {
                return Err("OAuth client id should end with .apps.googleusercontent.com".into());
            }
        }
        if !(5..=3600).contains(&self.poll_seconds) {
            return Err("Sync interval must be between 5 and 3600 seconds".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_settings() {
        let mut c = AppConfig { poll_seconds: 15, ..Default::default() };
        assert!(c.validate().is_ok());
        c.drive_id = Some("0AbC-d_9".into());
        assert!(c.validate().is_ok());
        c.drive_id = Some("https://drive.google.com/x".into());
        assert!(c.validate().is_err());
        c.drive_id = None;
        c.root = Some("relative/path".into());
        assert!(c.validate().is_err());
        c.root = None;
        c.poll_seconds = 1;
        assert!(c.validate().is_err());
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        let c = AppConfig { drive_id: Some("0Abc".into()), poll_seconds: 30, ..Default::default() };
        c.save(&p).unwrap();
        let back = AppConfig::load(&p);
        assert_eq!(back.drive_id.as_deref(), Some("0Abc"));
        assert_eq!(back.poll_seconds, 30);
        assert_eq!(AppConfig::load(&dir.path().join("missing.json")).poll_seconds, 15);
    }
}
