//! Settings shared by the whole team, stored in the shared drive itself at
//! `.cadsync/team.json`. Only drive members can read it, and whoever sets it
//! up does so once for everybody — no per-computer configuration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::AppVault;
use crate::drive::DriveApi;
use crate::error::{Error, Result};

const FILE: &str = "team.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSettings {
    /// SolidWorks Document Manager license key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swdm_key: Option<String>,
}

impl TeamSettings {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if let Some(k) = &self.swdm_key {
            if k.len() > 4096 || k.chars().any(|c| c.is_control()) {
                return Err("That doesn't look like a Document Manager key".into());
            }
        }
        Ok(())
    }
}

/// Last loaded settings, keyed by the file's md5 so we only re-download on change.
#[derive(Default)]
pub struct TeamCache(RwLock<Option<(Option<String>, TeamSettings)>>);

impl TeamCache {
    pub async fn clear(&self) {
        *self.0.write().await = None;
    }

    /// Current team settings (empty if none have been saved yet).
    pub async fn load(&self, v: &AppVault) -> Result<TeamSettings> {
        let Some(meta) = v.paths()?.meta_folder_id else { return Ok(TeamSettings::default()) };
        let Some(f) = v.index.children(&meta)?.into_iter().find(|f| f.name == FILE && !f.is_folder()) else {
            return Ok(TeamSettings::default());
        };
        if let Some((md5, s)) = self.0.read().await.as_ref() {
            if *md5 == f.md5 {
                return Ok(s.clone());
            }
        }
        std::fs::create_dir_all(v.scratch_dir())?;
        let tmp = v.scratch_dir().join(FILE);
        v.drive.download_file(&f.id, &tmp).await?;
        let bytes = std::fs::read(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        let settings: TeamSettings = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!(%e, "ignoring unreadable team.json");
            TeamSettings::default()
        });
        *self.0.write().await = Some((f.md5.clone(), settings.clone()));
        Ok(settings)
    }

    pub async fn save(&self, v: &AppVault, settings: &TeamSettings) -> Result<()> {
        settings.validate().map_err(Error::Config)?;
        let meta = v.ensure_meta_folder().await?;
        std::fs::create_dir_all(v.scratch_dir())?;
        let tmp = v.scratch_dir().join(FILE);
        std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
        let existing = v.index.children(&meta)?.into_iter().find(|f| f.name == FILE && !f.is_folder());
        let f = match existing {
            Some(f) => v.drive.upload_content(&f.id, &tmp).await?,
            None => v.drive.create_file(&meta, FILE, &tmp, BTreeMap::new()).await?,
        };
        let _ = std::fs::remove_file(&tmp);
        v.index.apply_remote(std::slice::from_ref(&f), &[])?;
        *self.0.write().await = Some((f.md5.clone(), settings.clone()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn saves_to_the_drive_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let v = super::super::demo::build(dir.path()).await.unwrap();
        v.refresh().await.unwrap();
        let cache = TeamCache::default();
        assert_eq!(cache.load(&v).await.unwrap(), TeamSettings::default());

        let s = TeamSettings { swdm_key: Some("ABC-123".into()) };
        cache.save(&v, &s).await.unwrap();
        cache.clear().await;
        assert_eq!(cache.load(&v).await.unwrap(), s, "read back from Drive, not the cache");

        // Updating replaces the same file.
        let s2 = TeamSettings { swdm_key: Some("DEF-456".into()) };
        cache.save(&v, &s2).await.unwrap();
        v.refresh().await.unwrap();
        cache.clear().await;
        assert_eq!(cache.load(&v).await.unwrap(), s2);
        let meta = v.paths().unwrap().meta_folder_id.unwrap();
        let count = v.index.children(&meta).unwrap().iter().filter(|f| f.name == FILE).count();
        assert_eq!(count, 1);

        assert!(TeamSettings { swdm_key: Some("bad\nkey".into()) }.validate().is_err());
    }
}
