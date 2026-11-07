//! Feature checks for the smoke test: everyone mirrors the whole folder, roles/areas are
//! enforced for a second (non-CADmin) teammate, and extra projects get their own folders.
//! Runs against the in-memory demo drive, so nothing touches Google.

use std::time::Duration;

use super::config::Project;
use super::AppState;
use crate::drive::Backend;
use crate::error::Error;
use crate::index::Index;
use crate::sync::Vault;

type Check = Result<(), String>;

fn ensure(ok: bool, what: &str) -> Check {
    if ok { Ok(()) } else { Err(format!("feature check failed: {what}")) }
}

/// Run every feature check; returns the names of the ones that passed.
pub async fn run(state: &AppState) -> Result<Vec<String>, String> {
    let mut passed = Vec::new();
    mirror_is_complete(state).await?;
    passed.push("whole folder is mirrored on this PC".to_string());
    roles_and_areas(state).await?;
    passed.push("CADmin/areas: teammate mirrors everything but can only edit their areas".to_string());
    projects_get_separate_folders(state).await?;
    passed.push("projects: same-named Drive folders get separate local folders".to_string());
    Ok(passed)
}

async fn mirror_is_complete(state: &AppState) -> Check {
    let v = state.vault().await.map_err(|e| e.to_string())?;
    let map = v.paths().map_err(|e| e.to_string())?;
    let mut parts = 0;
    for f in v.index.all_files().map_err(|e| e.to_string())? {
        if f.is_folder() || f.trashed || f.name == crate::team::TEAM_FILE {
            continue;
        }
        let Some(rel) = map.path(&f.id) else { continue };
        ensure(v.abs(rel.as_ref()).exists(), &format!("{} is not on this PC", f.name))?;
        parts += 1;
    }
    // The demo has 11 files, including the one a teammate has checked out.
    ensure(parts >= 11, "the demo files were not all downloaded")
}

fn id_of(v: &crate::app::AppVault, name: &str) -> Result<String, String> {
    v.index
        .all_files()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|f| f.name == name && !f.trashed)
        .map(|f| f.id)
        .ok_or_else(|| format!("demo has no \"{name}\""))
}

async fn roles_and_areas(state: &AppState) -> Check {
    let v = state.vault().await.map_err(|e| e.to_string())?;
    let fake = match &v.drive {
        Backend::Fake(d) => d.clone(),
        Backend::Http(_) => return Err("roles check needs the demo drive".into()),
    };
    let (arm, robot, frame, tube) =
        (id_of(&v, "Arm")?, id_of(&v, "Robot.SLDASM")?, id_of(&v, "Frame Rail.SLDPRT")?, id_of(&v, "Arm Tube.SLDPRT")?);
    let morgan = "morgan@demo.team";

    // Casey (this PC) becomes the CADmin; Morgan may edit only the Arm (Drivetrain is Jordan's); Robot.SLDASM is CADmin-only.
    v.save_team(crate::team::Team { admins: vec![v.me.clone()], areas: vec![] }).await.map_err(|e| e.to_string())?;
    v.set_area(&arm, Some(vec![morgan.into()])).await.map_err(|e| e.to_string())?;
    v.set_area(&id_of(&v, "Drivetrain")?, Some(vec!["jordan@demo.team".into()])).await.map_err(|e| e.to_string())?;
    v.set_area(&robot, Some(vec![])).await.map_err(|e| e.to_string())?;
    ensure(v.is_admin().map_err(|e| e.to_string())?, "casey should be a CADmin")?;

    // Morgan's PC: a separate index and folder, same drive.
    let dir = std::env::temp_dir().join("permafrost-smoke-morgan");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("vault")).map_err(|e| e.to_string())?;
    let m = Vault {
        drive: Backend::Fake(fake.as_user(morgan)),
        index: Index::open(&dir.join("index.sqlite")).map_err(|e| e.to_string())?,
        root_id: v.root_id.clone(),
        scope: None,
        root: dir.join("vault"),
        data_dir: dir.clone(),
        me: morgan.into(),
        my_name: "Morgan".into(),
        machine: "SMOKE-PC-2".into(),
        lock_verify_delay: Duration::from_millis(300),
    };
    let report = m.sync_down().await.map_err(|e| format!("morgan sync: {e}"))?;
    ensure(report.errors.is_empty(), "morgan's sync had errors")?;
    let mine = v.index.all_local().map_err(|e| e.to_string())?.len();
    let theirs = m.index.all_local().map_err(|e| e.to_string())?.len();
    ensure(theirs > 0 && theirs == mine, "morgan didn't mirror the same files as casey")?;
    ensure(!m.is_admin().map_err(|e| e.to_string())?, "morgan should not be a CADmin")?;

    // Morgan can edit the Arm, but not the Drivetrain or the top-level assembly.
    m.check_out(&tube).await.map_err(|e| format!("morgan couldn't check out in their area: {e}"))?;
    m.undo_check_out(&tube, true).await.map_err(|e| e.to_string())?;
    for (id, what) in [(&frame, "a part outside their area"), (&robot, "the CADmin-only top-level assembly")] {
        match m.check_out(id).await {
            Err(Error::NotAllowed { .. }) => {}
            other => return Err(format!("morgan should not be able to check out {what}, got {other:?}")),
        }
    }
    // The CADmin can edit the top level.
    v.check_out(&robot).await.map_err(|e| format!("casey couldn't check out the top-level assembly: {e}"))?;
    v.undo_check_out(&robot, true).await.map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

async fn projects_get_separate_folders(state: &AppState) -> Check {
    let mut cfg = state.config.read().await.clone();
    let proj = |id: &str, root| Project { folder_id: id.into(), name: "CAD".into(), root };
    let first = cfg.root_for("smokeFolderAAAAAA", "CAD");
    cfg.upsert_project(proj("smokeFolderAAAAAA", first.clone()));
    let second = cfg.root_for("smokeFolderBBBBBB", "CAD");
    ensure(first != second, "two projects named CAD share a local folder")?;
    // A known project keeps its folder.
    ensure(cfg.root_for("smokeFolderAAAAAA", "CAD") == first, "a project's folder changed")
}
