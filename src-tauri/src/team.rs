//! Who may edit what.
//!
//! Every PC keeps the whole CAD folder, but editing is assigned. The team settings live in
//! `.permafrost/team.json` on Drive, so every copy of Permafrost sees the same rules:
//!
//! - **CADmins** can edit everything (including the top-level robot assembly) and change who
//!   owns what. While there are no CADmins, roles are off and anyone can edit anything.
//! - **Areas**: a folder or a single file (a subsystem's assembly, say) that only its listed
//!   editors may edit. The nearest area above a file decides; an area with no editors is
//!   CADmins only. Files in no area can be edited by anyone.
//!
//! Like check-out locks, this is enforced by Permafrost itself: someone who edits files on the
//! Drive website directly isn't stopped by it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::drive::{DriveApi, RemoteFile};
use crate::error::{Error, Result};
use crate::sync::{shown, Vault};

pub const TEAM_FILE: &str = "team.json";
const TEAM_KEY: &str = "team";
const TEAM_REV_KEY: &str = "team_rev";
const MAX_PEOPLE: usize = 200;
const MAX_AREAS: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    /// CADmin emails. Empty means roles are off.
    #[serde(default)]
    pub admins: Vec<String>,
    #[serde(default)]
    pub areas: Vec<Area>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    /// Drive id of the folder or file.
    pub id: String,
    /// Who may edit it besides CADmins. Empty = CADmins only.
    #[serde(default)]
    pub editors: Vec<String>,
}

fn has(list: &[String], who: &str) -> bool {
    list.iter().any(|e| e.eq_ignore_ascii_case(who))
}

fn looks_like_email(s: &str) -> bool {
    s.len() <= 254 && !s.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',') && {
        let mut parts = s.splitn(2, '@');
        matches!((parts.next(), parts.next()), (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() && !b.contains('@'))
    }
}

fn clean_people(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in list {
        let p = p.trim().to_lowercase();
        if !p.is_empty() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

impl Team {
    pub fn roles_on(&self) -> bool {
        !self.admins.is_empty()
    }

    pub fn is_admin(&self, who: &str) -> bool {
        !self.roles_on() || has(&self.admins, who)
    }

    /// The area that decides who edits `id`: its own, or its nearest folder's.
    pub fn area_for(&self, id: &str, by_id: &HashMap<&str, &RemoteFile>) -> Option<&Area> {
        let mut cur = Some(id);
        // Bounded so a malformed parent chain can't loop forever.
        for _ in 0..256 {
            let c = cur?;
            if let Some(a) = self.areas.iter().find(|a| a.id == c) {
                return Some(a);
            }
            cur = by_id.get(c).and_then(|f| f.parent.as_deref());
        }
        None
    }

    pub fn can_edit(&self, who: &str, id: &str, by_id: &HashMap<&str, &RemoteFile>) -> bool {
        if !self.roles_on() || self.is_admin(who) {
            return true;
        }
        self.area_for(id, by_id).is_none_or(|a| has(&a.editors, who))
    }

    /// Lower-case, de-duplicate and check what the UI sent. `me` must stay a CADmin, so nobody
    /// locks themselves (and possibly the whole team) out by accident.
    pub fn normalized(self, me: &str) -> std::result::Result<Team, String> {
        let admins = clean_people(self.admins);
        let mut areas: Vec<Area> = Vec::new();
        for a in self.areas {
            if !crate::app::config::is_valid_id(&a.id) {
                return Err("an area points at an invalid file".into());
            }
            let area = Area { id: a.id, editors: clean_people(a.editors) };
            match areas.iter_mut().find(|x| x.id == area.id) {
                Some(x) => *x = area,
                None => areas.push(area),
            }
        }
        if let Some(bad) = admins.iter().chain(areas.iter().flat_map(|a| &a.editors)).find(|p| !looks_like_email(p)) {
            return Err(format!("\"{bad}\" isn't an email address"));
        }
        if admins.len() > MAX_PEOPLE || areas.len() > MAX_AREAS || areas.iter().any(|a| a.editors.len() > MAX_PEOPLE) {
            return Err("too many people or areas".into());
        }
        if !admins.is_empty() && !has(&admins, me) {
            return Err("keep yourself in the CADmin list (another CADmin can remove you later)".into());
        }
        if admins.is_empty() && !areas.is_empty() {
            return Err("add at least one CADmin before giving areas owners".into());
        }
        Ok(Team { admins, areas })
    }
}

/// The area that applies to a file, for showing in the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaInfo {
    pub id: String,
    /// Path of the area's folder or file in the CAD folder ("" for the CAD folder itself).
    pub path: String,
    pub editors: Vec<String>,
}

impl<D: DriveApi> Vault<D> {
    /// The team settings as last read from Drive.
    pub fn team(&self) -> Result<Team> {
        let raw = self.index.meta(TEAM_KEY)?.unwrap_or_default();
        if raw.is_empty() {
            return Ok(Team::default());
        }
        Ok(serde_json::from_str(&raw).unwrap_or_else(|e| {
            tracing::warn!(%e, "ignoring unreadable team settings");
            Team::default()
        }))
    }

    pub fn is_admin(&self) -> Result<bool> {
        Ok(self.team()?.is_admin(&self.me))
    }

    /// Where an area's folder or file is, as shown to people.
    pub fn area_info(&self, a: &Area, map: &crate::index::PathMap) -> AreaInfo {
        let path = if a.id == self.root_id { String::new() } else { map.path(&a.id).map(shown).unwrap_or_else(|| "(moved or deleted)".into()) };
        AreaInfo { id: a.id.clone(), path, editors: a.editors.clone() }
    }

    /// Error unless I may edit `f` under the team's areas.
    pub(crate) fn ensure_can_edit(&self, f: &RemoteFile) -> Result<()> {
        let team = self.team()?;
        if !team.roles_on() {
            return Ok(());
        }
        // Just the file and its folders, not the whole index: this runs once per file in "Check out all".
        let mut chain = vec![f.clone()];
        while let Some(parent) = chain.last().and_then(|x| x.parent.clone()) {
            match self.index.file(&parent)? {
                Some(p) if chain.len() < 256 => chain.push(p),
                _ => break,
            }
        }
        let by_id: HashMap<&str, &RemoteFile> = chain.iter().map(|f| (f.id.as_str(), f)).collect();
        if team.can_edit(&self.me, &f.id, &by_id) {
            return Ok(());
        }
        let area = team.area_for(&f.id, &by_id).expect("can't edit implies an area");
        let place = if area.id == self.root_id {
            "the CAD folder".to_string()
        } else {
            self.index.file(&area.id)?.map_or_else(|| "its area".to_string(), |a| a.name)
        };
        let who = if area.editors.is_empty() { "CADmins".to_string() } else { format!("{} and CADmins", area.editors.join(", ")) };
        Err(Error::NotAllowed { name: f.name.clone(), area: place, who })
    }

    /// Error unless I may add files at `rel` (a path in the CAD folder that may not exist on
    /// Drive yet): the nearest folder that does exist decides.
    pub(crate) fn ensure_can_add(&self, rel: &std::path::Path) -> Result<()> {
        let map = self.paths()?;
        let mut cur = rel.parent();
        let folder = loop {
            match cur {
                Some(p) if !p.as_os_str().is_empty() => match map.id_for(p) {
                    Some(id) => break id.to_string(),
                    None => cur = p.parent(),
                },
                _ => break self.root_id.clone(),
            }
        };
        let f = self.index.file(&folder)?.unwrap_or_else(|| RemoteFile {
            id: folder.clone(),
            name: "the CAD folder".into(),
            parent: None,
            mime_type: crate::drive::FOLDER_MIME.into(),
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: String::new(),
            modified_time: String::new(),
            trashed: false,
            read_only: false,
            app_properties: Default::default(),
            last_modifier: None,
        });
        self.ensure_can_edit(&f).map_err(|e| match e {
            Error::NotAllowed { area, who, .. } => Error::NotAllowed { name: shown(rel), area, who },
            e => e,
        })
    }

    fn team_file(&self) -> Result<Option<RemoteFile>> {
        let Some(meta) = self.paths()?.meta_folder_id else { return Ok(None) };
        Ok(self.index.children(&meta)?.into_iter().find(|f| !f.is_folder() && !f.trashed && f.name == TEAM_FILE))
    }

    /// Re-read `team.json` if it changed on Drive. Called on every sync.
    pub async fn load_team(&self) -> Result<()> {
        let Some(f) = self.team_file()? else {
            self.index.set_meta(TEAM_KEY, "")?;
            self.index.set_meta(TEAM_REV_KEY, "")?;
            return Ok(());
        };
        let rev = f.head_revision_id.clone().unwrap_or_default();
        if self.index.meta(TEAM_REV_KEY)?.as_deref() == Some(rev.as_str()) {
            return Ok(());
        }
        std::fs::create_dir_all(self.scratch_dir())?;
        let tmp = self.scratch_dir().join("team-in.json");
        self.drive.download_file(&f.id, &tmp).await?;
        let bytes = std::fs::read(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        match serde_json::from_slice::<Team>(&bytes) {
            Ok(t) => {
                self.index.set_meta(TEAM_KEY, &serde_json::to_string(&t)?)?;
                self.index.set_meta(TEAM_REV_KEY, &rev)?;
            }
            Err(e) => tracing::warn!(%e, "team.json on Drive is unreadable; keeping the last good copy"),
        }
        Ok(())
    }

    /// Save new team settings for everyone. Only CADmins (or anyone, while roles are off).
    pub async fn save_team(&self, team: Team) -> Result<Team> {
        if !self.team()?.is_admin(&self.me) {
            return Err(Error::Conflict("only CADmins can change who edits what".into()));
        }
        let team = team.normalized(&self.me).map_err(Error::Config)?;
        let existing = self.team_file()?;
        if let Some(f) = &existing {
            // Two CADmins editing at once: don't silently drop the other one's change.
            let now = self.drive.get_file(&f.id).await?;
            let known = self.index.meta(TEAM_REV_KEY)?.unwrap_or_default();
            if now.head_revision_id.as_deref().unwrap_or_default() != known {
                self.index.apply_remote(std::slice::from_ref(&now), &[])?;
                self.load_team().await.ok();
                return Err(Error::Conflict("someone else just changed the team settings; check them and try again".into()));
            }
        }
        std::fs::create_dir_all(self.scratch_dir())?;
        let tmp = self.scratch_dir().join("team-out.json");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&team)?)?;
        let written = match existing {
            Some(f) => self.drive.upload_content(&f.id, &tmp).await,
            None => {
                let meta = self.ensure_meta_folder().await?;
                self.drive.create_file(&meta, TEAM_FILE, &tmp, Default::default()).await
            }
        };
        let _ = std::fs::remove_file(&tmp);
        let written = written?;
        self.index.apply_remote(std::slice::from_ref(&written), &[])?;
        self.index.set_meta(TEAM_KEY, &serde_json::to_string(&team)?)?;
        self.index.set_meta(TEAM_REV_KEY, written.head_revision_id.as_deref().unwrap_or_default())?;
        tracing::info!(admins = team.admins.len(), areas = team.areas.len(), "team settings saved");
        Ok(team)
    }

    /// Give a folder or file its own editors (`None` removes the area, so the one above applies).
    pub async fn set_area(&self, id: &str, editors: Option<Vec<String>>) -> Result<Team> {
        let mut team = self.team()?;
        team.areas.retain(|a| a.id != id);
        if let Some(editors) = editors {
            team.areas.push(Area { id: id.to_string(), editors });
        }
        self.save_team(team).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::fake::FakeDrive;
    use crate::sync::testutil::vault;

    fn team(admins: &[&str], areas: &[(&str, &[&str])]) -> Team {
        Team {
            admins: admins.iter().map(|s| s.to_string()).collect(),
            areas: areas
                .iter()
                .map(|(id, eds)| Area { id: id.to_string(), editors: eds.iter().map(|s| s.to_string()).collect() })
                .collect(),
        }
    }

    #[test]
    fn nearest_area_decides_and_admins_edit_everything() {
        let mk = |id: &str, parent: &str| RemoteFile {
            id: id.into(),
            name: id.into(),
            parent: Some(parent.into()),
            mime_type: String::new(),
            md5: None,
            size: None,
            head_revision_id: None,
            created_time: String::new(),
            modified_time: String::new(),
            trashed: false,
            read_only: false,
            app_properties: Default::default(),
            last_modifier: None,
        };
        let files = [mk("robot", "root"), mk("arm", "robot"), mk("gear", "arm"), mk("top", "robot"), mk("lib", "root")];
        let by_id: HashMap<&str, &RemoteFile> = files.iter().map(|f| (f.id.as_str(), f)).collect();
        let t = team(&["cadmin@t.org"], &[("robot", &[]), ("arm", &["alex@t.org"])]);

        assert!(t.can_edit("alex@t.org", "gear", &by_id), "arm editor edits arm parts");
        assert!(!t.can_edit("blair@t.org", "gear", &by_id));
        assert!(!t.can_edit("alex@t.org", "top", &by_id), "robot area is CADmins only");
        assert!(t.can_edit("CADMIN@t.org", "top", &by_id), "CADmins edit the top level");
        assert!(t.can_edit("blair@t.org", "lib", &by_id), "no area: anyone");

        let off = team(&[], &[("arm", &["alex@t.org"])]);
        assert!(off.can_edit("blair@t.org", "gear", &by_id), "no CADmins: roles are off");
    }

    #[test]
    fn normalizing_keeps_me_admin_and_rejects_junk() {
        let t = team(&[" Me@T.org ", "me@t.org"], &[("a1", &["X@t.org"]), ("a1", &["y@t.org"])]).normalized("me@t.org").unwrap();
        assert_eq!(t.admins, ["me@t.org"]);
        assert_eq!(t.areas.len(), 1);
        assert_eq!(t.areas[0].editors, ["y@t.org"]);

        assert!(team(&["other@t.org"], &[]).normalized("me@t.org").is_err(), "can't remove yourself");
        assert!(team(&["me@t.org", "not an email"], &[]).normalized("me@t.org").is_err());
        assert!(team(&[], &[("a1", &["x@t.org"])]).normalized("me@t.org").is_err(), "areas need a CADmin");
        assert!(team(&["me@t.org"], &[("../x", &[])]).normalized("me@t.org").is_err());
        assert!(team(&[], &[]).normalized("me@t.org").is_ok(), "turning roles off is fine");
    }

    #[tokio::test]
    async fn areas_block_check_out_and_new_files_for_everyone_else() {
        let drive = FakeDrive::new("seed@team.org");
        let root = drive.drive_id.clone();
        let arm = drive.seed_folder(&root, "Arm");
        let gear = drive.seed_file(&arm, "Gear.SLDPRT", b"g1");
        let top = drive.seed_file(&root, "Robot.SLDASM", b"r1");

        let boss = vault(&drive, "boss@team.org", "pc-boss");
        let alex = vault(&drive, "alex@team.org", "pc-a");
        let blair = vault(&drive, "blair@team.org", "pc-b");
        boss.vault.sync_down().await.unwrap();
        boss.vault
            .save_team(team(&["boss@team.org"], &[(&root, &[]), (&arm, &["alex@team.org"])]))
            .await
            .unwrap();

        for v in [&alex.vault, &blair.vault] {
            v.sync_down().await.unwrap();
            assert!(v.team().unwrap().roles_on(), "team.json reaches every PC");
            assert!(!v.root.join(".permafrost").exists(), "settings aren't copied into the CAD folder");
        }

        alex.vault.check_out(&gear).await.unwrap();
        alex.vault.undo_check_out(&gear, false).await.unwrap();
        assert!(matches!(blair.vault.check_out(&gear).await, Err(Error::NotAllowed { .. })));
        assert!(matches!(alex.vault.check_out(&top).await, Err(Error::NotAllowed { .. })), "top level is CADmins only");
        assert!(matches!(blair.vault.break_lock(&gear).await, Err(Error::NotAllowed { .. })));

        assert!(alex.vault.ensure_can_add(std::path::Path::new("Arm/New/Bracket.SLDPRT")).is_ok());
        assert!(matches!(blair.vault.ensure_can_add(std::path::Path::new("Arm/Bracket.SLDPRT")), Err(Error::NotAllowed { .. })));
        assert!(matches!(alex.vault.ensure_can_add(std::path::Path::new("Bracket.SLDPRT")), Err(Error::NotAllowed { .. })));

        // The CADmin can check out just the top level, leaving the subsystem free.
        boss.vault.check_out(&top).await.unwrap();
        alex.vault.check_out(&gear).await.unwrap();

        // Only CADmins change the rules.
        assert!(alex.vault.set_area(&arm, Some(vec!["alex@team.org".into(), "blair@team.org".into()])).await.is_err());
        boss.vault.set_area(&arm, Some(vec!["alex@team.org".into(), "blair@team.org".into()])).await.unwrap();
        blair.vault.sync_down().await.unwrap();
        alex.vault.undo_check_out(&gear, false).await.unwrap();
        blair.vault.check_out(&gear).await.unwrap();
    }

    #[tokio::test]
    async fn a_stale_copy_of_the_settings_is_not_saved_over_a_newer_one() {
        let drive = FakeDrive::new("seed@team.org");
        let a = vault(&drive, "a@team.org", "pc-a");
        let b = vault(&drive, "b@team.org", "pc-b");
        a.vault.sync_down().await.unwrap();
        a.vault.save_team(team(&["a@team.org", "b@team.org"], &[])).await.unwrap();
        b.vault.sync_down().await.unwrap();
        a.vault.save_team(team(&["a@team.org", "b@team.org", "c@team.org"], &[])).await.unwrap();
        // b hasn't synced since: its save is refused, and it now has a's version.
        assert!(b.vault.save_team(team(&["b@team.org"], &[])).await.is_err());
        assert_eq!(b.vault.team().unwrap().admins.len(), 3);
    }
}
