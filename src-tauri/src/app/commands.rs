//! Tauri commands: the UI's whole API. Inputs from the UI are validated here.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use super::config::AppConfig;
use super::{AppState, AppVault};
use crate::drive::auth::Auth;
use crate::drive::http::BrowseFolder;
use crate::drive::{props, LockInfo, RemoteFile};
use crate::error::{Error, Result};
use crate::index::PathMap;
use crate::names::{cad_kind, is_temp_file, CadKind};
use crate::refs::{CheckInPlan, Problem, SidecarRefs, StaticRefs};
use crate::sync::localfs;
use crate::sync::pull::PullReport;
use crate::sync::push::{PushOutcome, PushRequest};
use crate::sync::remove::DeleteOutcome;
use crate::team::{AreaInfo, Team};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// This build can sign in with Google (an OAuth client is built in or configured).
    sign_in_available: bool,
    /// Signed in to Google (may not have picked a drive yet).
    account: Option<String>,
    /// The signed-in account's Google name.
    account_name: Option<String>,
    /// A shared drive is open.
    signed_in: bool,
    /// Google rejected the saved sign-in; show "sign in again".
    sign_in_expired: bool,
    /// The saved sign-in is being restored right now.
    restoring: bool,
    /// A sync pass is running right now.
    syncing: bool,
    demo: bool,
    me: Option<String>,
    root: Option<PathBuf>,
    folder_id: Option<String>,
    folder_name: Option<String>,
    file_count: u64,
    pending_check_ins: usize,
    refs_available: bool,
    /// Reference checking is switched on in Settings (it may still be unavailable).
    refs_enabled: bool,
    /// The team has CADmins, so areas restrict who edits what.
    roles_on: bool,
    /// I'm a CADmin (always true while roles are off).
    is_admin: bool,
    /// Every project this PC has opened; the open one is marked `active`.
    projects: Vec<ProjectInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    id: String,
    name: String,
    root: PathBuf,
    active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    id: String,
    name: String,
    local_name: Option<String>,
    is_folder: bool,
    kind: CadKind,
    size: Option<u64>,
    modified_time: String,
    last_modifier: Option<String>,
    lock: Option<LockInfo>,
    mine: bool,
    /// I may check it out (its area allows me, or I'm a CADmin).
    can_edit: bool,
    /// The area that decides who edits it, if any.
    area: Option<AreaInfo>,
    /// synced | outdated | not_synced | modified | pending
    status: &'static str,
}

type ById<'a> = std::collections::HashMap<&'a str, &'a RemoteFile>;

fn entry(v: &AppVault, f: &RemoteFile, map: &PathMap, by_id: &ById, team: &Team) -> Result<Entry> {
    let status = if f.is_folder() {
        "synced"
    } else if f.prop(props::PENDING_CHANGESET).is_some_and(|c| !v.index.has_changeset(c).unwrap_or(false)) {
        "pending"
    } else {
        match v.index.local(&f.id)? {
            None => "not_synced",
            Some(l) => {
                let fp = l.synced_size.zip(l.synced_mtime);
                if localfs::is_modified(&v.abs(l.rel_path.as_ref()), l.synced_md5.as_deref(), fp)? {
                    "modified"
                } else if l.synced_rev != f.head_revision_id {
                    "outdated"
                } else {
                    "synced"
                }
            }
        }
    };
    Ok(Entry {
        id: f.id.clone(),
        name: f.name.clone(),
        local_name: map.path(&f.id).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()),
        is_folder: f.is_folder(),
        kind: cad_kind(&f.name),
        size: f.size,
        modified_time: f.modified_time.clone(),
        last_modifier: f.last_modifier.clone(),
        lock: f.lock(),
        mine: v.i_hold_lock(f),
        can_edit: team.can_edit(&v.me, &f.id, by_id),
        area: team.roles_on().then(|| team.area_for(&f.id, by_id)).flatten().map(|a| v.area_info(a, map)),
        status,
    })
}

/// Reject `..`, absolute paths and drive prefixes in paths from the UI.
fn safe_rel(p: &Path) -> Result<PathBuf> {
    if p.components().all(|c| matches!(c, Component::Normal(_))) {
        Ok(p.to_path_buf())
    } else {
        Err(Error::Conflict(format!("invalid path {}", p.display())))
    }
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<Status> {
    let cfg = state.config.read().await.clone();
    let vault = state.vault.read().await.clone();
    let refs_enabled = !cfg.refs_disabled;
    let team = match &vault {
        Some(v) => v.team()?,
        None => Team::default(),
    };
    let me = vault.as_ref().map(|v| v.me.clone()).unwrap_or_default();
    let refs_available = refs_enabled && sidecar_path(&cfg).is_some_and(|p| p.exists());
    Ok(Status {
        sign_in_available: state.oauth_client().await.is_some(),
        account: state.account.read().await.as_ref().map(|a| a.email.clone()),
        account_name: state.account.read().await.as_ref().map(|a| a.name.clone()),
        signed_in: vault.is_some(),
        sign_in_expired: state.sign_in_expired.load(std::sync::atomic::Ordering::Relaxed),
        restoring: state.restoring.load(std::sync::atomic::Ordering::Relaxed),
        syncing: state.syncing.load(std::sync::atomic::Ordering::Relaxed) > 0,
        demo: state.demo.load(std::sync::atomic::Ordering::Relaxed),
        me: vault.as_ref().map(|v| v.me.clone()),
        root: vault.as_ref().map(|v| v.root.clone()),
        folder_id: vault.as_ref().map(|v| v.root_id.clone()),
        folder_name: state.folder_name.read().await.clone(),
        file_count: match &vault {
            Some(v) => v.index.file_count()?,
            None => 0,
        },
        pending_check_ins: match &vault {
            Some(v) => v.index.journal()?.len(),
            None => 0,
        },
        refs_available,
        refs_enabled,
        roles_on: team.roles_on(),
        is_admin: team.is_admin(&me),
        projects: if state.demo.load(std::sync::atomic::Ordering::Relaxed) {
            vec![]
        } else {
            cfg.projects
                .iter()
                .map(|p| ProjectInfo {
                    id: p.folder_id.clone(),
                    name: p.name.clone(),
                    root: p.root.clone(),
                    active: vault.as_ref().is_some_and(|v| v.root_id == p.folder_id),
                })
                .collect()
        },
    })
}

/// The UI calls this once it has rendered; the start-up smoke test waits for it.
#[tauri::command]
pub fn frontend_ready(app: AppHandle) {
    super::smoke::mark_ui_ready(&app);
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: AppConfig) -> Result<()> {
    config.validate().map_err(Error::Config)?;
    config.save(&state.config_path)?;
    *state.config.write().await = config;
    Ok(())
}

/// Turn SolidWorks reference checking on or off for this computer.
#[tauri::command]
pub async fn set_refs_enabled(state: State<'_, AppState>, enabled: bool) -> Result<()> {
    let mut cfg = state.config.write().await;
    cfg.refs_disabled = !enabled;
    cfg.save(&state.config_path)
}

/// Sign in with Google in the browser. If a shared drive was picked before,
/// it opens straight away; otherwise the UI asks which drive to use.
#[tauri::command]
pub async fn sign_in(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let client = state
        .oauth_client()
        .await
        .ok_or_else(|| Error::Config("this build has no Google sign-in set up (see docs/SETUP.md)".into()))?;
    let opener = app.clone();
    let auth = Auth::sign_in(client, move |url| {
        if let Err(e) = opener.opener().open_url(url, None::<&str>) {
            tracing::error!(%e, "could not open browser");
        }
    })
    .await?;
    state.set_auth(std::sync::Arc::new(auth)).await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Where the folder picker starts: your shared drives, My Drive, and folders
/// shared with you.
#[derive(Serialize)]
pub struct Location {
    id: String,
    name: String,
    /// shared_drive | my_drive | shared_with_me
    kind: &'static str,
}

pub(crate) const SHARED_WITH_ME: &str = "__shared_with_me";

#[tauri::command]
pub async fn browse_locations(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Location>> {
    let r = state.http().await?.list_shared_drives().await;
    let drives = state.check_expired(&app, r).await?;
    let mut out: Vec<Location> = drives
        .into_iter()
        .map(|d| Location { id: d.id, name: d.name, kind: "shared_drive" })
        .collect();
    out.push(Location { id: "root".into(), name: "My Drive".into(), kind: "my_drive" });
    out.push(Location { id: SHARED_WITH_ME.into(), name: "Shared with me".into(), kind: "shared_with_me" });
    Ok(out)
}

/// The folders inside a location or folder shown in the picker.
#[tauri::command]
pub async fn browse_folders(app: AppHandle, state: State<'_, AppState>, parent_id: String) -> Result<Vec<BrowseFolder>> {
    let drive = state.http().await?;
    let r = if parent_id == SHARED_WITH_ME {
        drive.shared_with_me_folders().await
    } else if parent_id == "root" || super::config::is_valid_id(&parent_id) {
        drive.child_folders(&parent_id).await
    } else {
        return Err(Error::Config("invalid folder".into()));
    };
    state.check_expired(&app, r).await
}

/// Use this folder as the vault. `input` is a folder link or a folder id.
#[tauri::command]
pub async fn choose_folder(app: AppHandle, state: State<'_, AppState>, input: String) -> Result<()> {
    let folder_id = super::config::folder_id_from_input(&input)
        .ok_or_else(|| Error::Config("that doesn't look like a Drive folder link".into()))?;
    let r = state.open_folder(&folder_id).await;
    state.check_expired(&app, r).await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Switch to another project this PC has opened before (in-season, off-season, ...).
#[tauri::command]
pub async fn open_project(app: AppHandle, state: State<'_, AppState>, folder_id: String) -> Result<()> {
    if !state.config.read().await.projects.iter().any(|p| p.folder_id == folder_id) {
        return Err(Error::NotFound("that project".into()));
    }
    let r = state.open_folder(&folder_id).await;
    state.check_expired(&app, r).await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Forget a project on this PC. Its local files and the Drive folder are left alone.
#[tauri::command]
pub async fn remove_project(app: AppHandle, state: State<'_, AppState>, folder_id: String) -> Result<()> {
    let was_open = state.vault.read().await.as_ref().is_some_and(|v| v.root_id == folder_id);
    let next = {
        let mut cfg = state.config.write().await;
        cfg.projects.retain(|p| p.folder_id != folder_id);
        if cfg.folder_id.as_deref() == Some(folder_id.as_str()) {
            cfg.folder_id = None;
            cfg.root = None;
        }
        cfg.save(&state.config_path)?;
        cfg.projects.first().map(|p| p.folder_id.clone())
    };
    if was_open {
        *state.vault.write().await = None;
        *state.folder_name.write().await = None;
        if let Some(next) = next {
            let r = state.open_folder(&next).await;
            state.check_expired(&app, r).await?;
        }
    }
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Go to the folder picker to add another project (e.g. a new season's drive).
#[tauri::command]
pub async fn switch_drive(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    *state.vault.write().await = None;
    *state.folder_name.write().await = None;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.sign_out().await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn start_demo(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.open_demo().await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

/// Advanced: move the local vault folder (must match on every computer).
#[tauri::command]
pub async fn set_vault_folder(app: AppHandle, state: State<'_, AppState>, root: PathBuf) -> Result<()> {
    if !root.is_absolute() {
        return Err(Error::Config("use a full path like C:\\CAD\\Team Drive".into()));
    }
    let folder_id = state.vault().await?.root_id.clone();
    {
        let mut cfg = state.config.write().await;
        cfg.root = Some(root);
        cfg.save(&state.config_path)?;
    }
    state.open_folder(&folder_id).await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn list_children(state: State<'_, AppState>, parent_id: Option<String>) -> Result<Vec<Entry>> {
    let v = state.vault().await?;
    let all = v.index.all_files()?;
    let map = PathMap::build(&v.root_id, &all);
    let team = v.team()?;
    let by_id: ById = all.iter().map(|f| (f.id.as_str(), f)).collect();
    let parent = parent_id.unwrap_or_else(|| v.root_id.clone());
    v.index
        .children(&parent)?
        .iter()
        .filter(|f| map.contains(&f.id))
        .map(|f| entry(&v, f, &map, &by_id, &team))
        .collect()
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, state: State<'_, AppState>) -> Result<PullReport> {
    let r = state.sync_tracked(&app).await;
    state.check_expired(&app, r).await
}

#[tauri::command]
pub async fn check_out(app: AppHandle, state: State<'_, AppState>, file_id: String) -> Result<LockInfo> {
    let r = state.vault().await?.check_out(&file_id).await;
    if let Ok(l) = &r {
        tracing::info!(file = %file_id, owner = %l.owner, "checked out");
    }
    state.check_expired(&app, r).await
}

#[tauri::command]
pub async fn undo_check_out(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
    discard_changes: bool,
) -> Result<()> {
    let r = state.vault().await?.undo_check_out(&file_id, discard_changes).await;
    state.check_expired(&app, r).await
}

#[tauri::command]
pub async fn break_lock(app: AppHandle, state: State<'_, AppState>, file_id: String) -> Result<()> {
    let r = state.vault().await?.break_lock(&file_id).await.map(|_| ());
    state.check_expired(&app, r).await
}

/// Delete a file for the whole team (to the shared drive's trash). Without
/// `force`, refuses with "used by …" if an assembly is known to use it.
#[tauri::command]
pub async fn delete_from_drive(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
    force: bool,
) -> Result<DeleteOutcome> {
    let r = state.vault().await?.delete_from_drive(&file_id, force).await;
    let _ = app.emit("vault-changed", ());
    state.check_expired(&app, r).await
}

#[tauri::command]
pub async fn open_logs(app: AppHandle) -> Result<()> {
    let dir = super::log_dir(&app);
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

/// A crash report from a previous run that the user hasn't been told about.
#[tauri::command]
pub async fn take_new_crash_report(app: AppHandle) -> Result<Option<PathBuf>> {
    Ok(super::logging::take_new_crash_report(&super::log_dir(&app)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MyChanges {
    checked_out: Vec<Entry>,
    new_files: Vec<PathBuf>,
}

#[tauri::command]
pub async fn my_changes(state: State<'_, AppState>) -> Result<MyChanges> {
    let v = state.vault().await?;
    let all = v.index.all_files()?;
    let map = PathMap::build(&v.root_id, &all);
    let team = v.team()?;
    let by_id: ById = all.iter().map(|f| (f.id.as_str(), f)).collect();
    let checked_out = all
        .iter()
        .filter(|f| v.i_hold_lock(f) && map.contains(&f.id))
        .map(|f| entry(&v, f, &map, &by_id, &team))
        .collect::<Result<_>>()?;
    let mut new_files = Vec::new();
    collect_untracked(&v.root, Path::new(""), &map, &mut new_files)?;
    new_files.sort();
    Ok(MyChanges { checked_out, new_files })
}

/// Files on disk in the vault that aren't on Drive yet.
fn collect_untracked(root: &Path, rel: &Path, map: &PathMap, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(dir) = std::fs::read_dir(root.join(rel)) else { return Ok(()) };
    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || is_temp_file(&name) {
            continue;
        }
        let child = rel.join(&name);
        if entry.file_type()?.is_dir() {
            collect_untracked(root, &child, map, out)?;
        } else if map.id_for(&child).is_none() {
            out.push(child);
        }
    }
    Ok(())
}

pub(crate) fn sidecar_path(cfg: &AppConfig) -> Option<PathBuf> {
    cfg.sidecar_path.clone().or_else(|| {
        let exe = std::env::current_exe().ok()?;
        Some(bundled_sidecar(exe.parent()?))
    })
}

/// Where Tauri puts the `binaries/SwRefs` externalBin (tauri.windows.conf.json): next to the app
/// executable with the target triple dropped, both in the installed app and in target/<profile>
/// during `tauri dev`.
fn bundled_sidecar(exe_dir: &Path) -> PathBuf {
    exe_dir.join(if cfg!(windows) { "SwRefs.exe" } else { "SwRefs" })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInPreview {
    plan: CheckInPlan,
    /// False when the reference reader isn't set up, so dependencies were not checked.
    refs_checked: bool,
}

async fn plan_check_in(state: &AppState, files: &[String], new_files: &[PathBuf]) -> Result<CheckInPreview> {
    let v = state.vault().await?;
    let new_files = new_files.iter().map(|p| safe_rel(p)).collect::<Result<Vec<_>>>()?;
    let cfg = state.config.read().await.clone();
    match sidecar_path(&cfg).filter(|p| p.exists()).filter(|_| !cfg.refs_disabled) {
        Some(exe) => {
            let reader = SidecarRefs::new(exe);
            Ok(CheckInPreview { plan: v.prepare_check_in(&reader, files, &new_files).await?, refs_checked: true })
        }
        _ => {
            let plan = v.prepare_check_in(&StaticRefs::default(), files, &new_files).await?;
            Ok(CheckInPreview { plan, refs_checked: false })
        }
    }
}

#[tauri::command]
pub async fn preview_check_in(
    app: AppHandle,
    state: State<'_, AppState>,
    files: Vec<String>,
    new_files: Vec<PathBuf>,
) -> Result<CheckInPreview> {
    let r = plan_check_in(&state, &files, &new_files).await;
    state.check_expired(&app, r).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckInArgs {
    files: Vec<String>,
    new_files: Vec<PathBuf>,
    message: String,
    keep_locked: bool,
    /// Duplicate names are risky but common in COTS libraries; allow on request.
    allow_duplicate_names: bool,
}

#[tauri::command]
pub async fn check_in(app: AppHandle, state: State<'_, AppState>, args: CheckInArgs) -> Result<PushOutcome> {
    let message = args.message.trim().to_string();
    if message.is_empty() || message.len() > 500 {
        return Err(Error::Conflict("describe the change in 1–500 characters".into()));
    }
    let preview = plan_check_in(&state, &args.files, &args.new_files).await?;
    let blocking = preview
        .plan
        .problems
        .iter()
        .filter(|p| !(args.allow_duplicate_names && matches!(p, Problem::DuplicateName { .. })))
        .count();
    if blocking > 0 {
        return Err(Error::Validation(blocking));
    }
    let v = state.vault().await?;
    let _guard = state.sync_lock.lock().await;
    let r = v
        .push(PushRequest {
            files: preview.plan.files,
            new_files: preview.plan.new_files,
            message,
            keep_locked: args.keep_locked,
            moves: vec![],
        })
        .await;
    match &r {
        Ok(out) => tracing::info!(changeset = %out.changeset, files = out.files, "checked in"),
        Err(e) => tracing::warn!(%e, "check-in failed"),
    }
    drop(_guard);
    let out = state.check_expired(&app, r).await?;
    let _ = app.emit("vault-changed", ());
    Ok(out)
}

#[tauri::command]
pub async fn reveal_root(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let root = state.vault().await?.root.clone();
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

/// Open a downloaded file in whatever app handles it (SolidWorks, for CAD files).
#[tauri::command]
pub async fn open_file(app: AppHandle, state: State<'_, AppState>, file_id: String) -> Result<()> {
    let v = state.vault().await?;
    let local = v.index.local(&file_id)?.ok_or_else(|| Error::NotFound("that file isn't on this PC yet".into()))?;
    let path = v.abs(local.rel_path.as_ref());
    if !path.is_file() {
        return Err(Error::NotFound("that file isn't on this PC yet".into()));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_paths() {
        assert!(safe_rel(Path::new("Arm/Gear.SLDPRT")).is_ok());
        assert!(safe_rel(Path::new("../evil.sldprt")).is_err());
        assert!(safe_rel(Path::new("/etc/passwd")).is_err());
        assert!(safe_rel(Path::new("Arm/../../x")).is_err());
    }

    #[test]
    fn finds_sidecar_next_to_exe_or_in_config() {
        let dir = Path::new("app");
        let name = if cfg!(windows) { "SwRefs.exe" } else { "SwRefs" };
        assert_eq!(bundled_sidecar(dir), dir.join(name));

        let cfg = AppConfig { sidecar_path: Some(PathBuf::from("custom/SwRefs.exe")), ..Default::default() };
        assert_eq!(sidecar_path(&cfg), Some(PathBuf::from("custom/SwRefs.exe")));
    }
}
