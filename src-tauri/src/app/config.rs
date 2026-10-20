//! Per-computer settings, stored as JSON in the app config folder. Users never
//! edit these by hand: the drive is picked after sign-in, the vault folder has
//! a default, the Google client is built in, and the Document Manager key is a
//! team setting on the shared drive. `oauth` and `swdm_key` are overrides only.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::drive::auth::OAuthClient;
use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub oauth: Option<OAuthClient>,
    /// The vault folder's Drive id: a shared drive's root, or any folder.
    pub folder_id: Option<String>,
    /// Local vault root. Must be the same on every machine.
    pub root: Option<PathBuf>,
    /// Path to SwRefs.exe; defaults to the bundled sidecar.
    pub sidecar_path: Option<PathBuf>,
    /// Local override for the Document Manager key. Normally the key comes
    /// from the team settings on the shared drive (see app/team.rs).
    pub swdm_key: Option<String>,
}

/// Drive ids are letters, digits, `-` and `_`.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 200 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Accepts a folder link (`.../folders/<id>`, `...?id=<id>`) or a bare id.
pub fn folder_id_from_input(input: &str) -> Option<String> {
    let input = input.trim();
    let id = if let Some(i) = input.find("/folders/") {
        input[i + "/folders/".len()..].split(['?', '/', '#', '&']).next().unwrap_or("")
    } else if let Some(i) = input.find("id=") {
        input[i + 3..].split(['&', '#']).next().unwrap_or("")
    } else {
        input
    };
    is_valid_id(id).then(|| id.to_string())
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
            .unwrap_or_default()
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
        if let Some(id) = &self.folder_id {
            if !is_valid_id(id) {
                return Err("The folder id looks wrong".into());
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_settings() {
        let mut c = AppConfig::default();
        assert!(c.validate().is_ok());
        c.folder_id = Some("0AbC-d_9".into());
        assert!(c.validate().is_ok());
        c.folder_id = Some("https://drive.google.com/x".into());
        assert!(c.validate().is_err());
        c.folder_id = None;
        c.root = Some("relative/path".into());
        assert!(c.validate().is_err());
        c.root = None;
        assert!(c.validate().is_ok());
    }

    #[test]
    fn reads_folder_links() {
        let id = Some("1AbC-dEf_9".to_string());
        assert_eq!(folder_id_from_input("1AbC-dEf_9"), id);
        assert_eq!(folder_id_from_input("https://drive.google.com/drive/folders/1AbC-dEf_9?usp=sharing"), id);
        assert_eq!(folder_id_from_input("https://drive.google.com/drive/u/1/folders/1AbC-dEf_9"), id);
        assert_eq!(folder_id_from_input("https://drive.google.com/open?id=1AbC-dEf_9&usp=x"), id);
        assert_eq!(folder_id_from_input("  1AbC-dEf_9 \n"), id);
        assert_eq!(folder_id_from_input("https://drive.google.com/file/d/xyz/view"), None);
        assert_eq!(folder_id_from_input(""), None);
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        let c = AppConfig { folder_id: Some("0Abc".into()), ..Default::default() };
        c.save(&p).unwrap();
        let back = AppConfig::load(&p);
        assert_eq!(back.folder_id.as_deref(), Some("0Abc"));
        assert!(AppConfig::load(&dir.path().join("missing.json")).folder_id.is_none());
    }
}
