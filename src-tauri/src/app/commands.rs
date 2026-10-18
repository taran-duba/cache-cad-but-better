//! Tauri commands: the UI's whole API. Inputs from the UI are validated here.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use super::config::AppConfig;
use super::{AppState, AppVault};
use crate::drive::auth::Auth;
use crate::drive::{props, LockInfo, RemoteFile};
use crate::error::{Error, Result};
use crate::index::{Manifest, PathMap};
use crate::names::{cad_kind, is_temp_file, CadKind};
use crate::refs::{CheckInPlan, Problem, SidecarRefs, StaticRefs};
use crate::sync::localfs;
use crate::sync::pull::{is_selected, PullReport};
use crate::sync::push::{PushOutcome, PushRequest};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    signed_in: bool,
    demo: bool,
    me: Option<String>,
    root: Option<PathBuf>,
    drive_id: Option<String>,
    file_count: u64,
    pending_check_ins: usize,
    refs_available: bool,
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
    /// Explicit selective-sync choice on this item (None = inherited).
    selection: Option<bool>,
    selected: bool,
    /// synced | outdated | not_synced | modified | pending
    status: &'static str,
}

fn entry(v: &AppVault, f: &RemoteFile, map: &PathMap, all: &[RemoteFile], sel: &std::collections::BTreeMap<String, bool>) -> Result<Entry> {
    let by_id = all.iter().map(|f| (f.id.as_str(), f)).collect();
    let selected = is_selected(&f.id, &by_id, sel);
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
        selection: sel.get(&f.id).copied(),
        selected,
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
    let refs_available = sidecar_path(&cfg).is_some_and(|p| p.exists()) && cfg.swdm_key.is_some();
    Ok(Status {
        configured: cfg.oauth.is_some() && cfg.drive_id.is_some(),
        signed_in: vault.is_some(),
        demo: state.demo.load(std::sync::atomic::Ordering::Relaxed),
        me: vault.as_ref().map(|v| v.me.clone()),
        root: vault.as_ref().map(|v| v.root.clone()),
        drive_id: vault.as_ref().map(|v| v.drive_id.clone()),
        file_count: match &vault {
            Some(v) => v.index.file_count()?,
            None => 0,
        },
        pending_check_ins: match &vault {
            Some(v) => v.index.journal()?.len(),
            None => 0,
        },
        refs_available,
    })
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

#[tauri::command]
pub async fn sign_in(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let oauth = state.config.read().await.oauth.clone().ok_or_else(|| Error::Config("OAuth client".into()))?;
    let opener = app.clone();
    let auth = Auth::sign_in(oauth, move |url| {
        if let Err(e) = opener.opener().open_url(url, None::<&str>) {
            tracing::error!(%e, "could not open browser");
        }
    })
    .await?;
    state.open_http(auth).await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    Auth::sign_out()?;
    *state.vault.write().await = None;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn start_demo(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.open_demo().await?;
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn list_children(state: State<'_, AppState>, parent_id: Option<String>) -> Result<Vec<Entry>> {
    let v = state.vault().await?;
    let all = v.index.all_files()?;
    let map = PathMap::build(&v.drive_id, &all);
    let sel = v.index.selections()?;
    let parent = parent_id.unwrap_or_else(|| v.drive_id.clone());
    v.index
        .children(&parent)?
        .iter()
        .filter(|f| map.contains(&f.id))
        .map(|f| entry(&v, f, &map, &all, &sel))
        .collect()
}

#[tauri::command]
pub async fn set_selection(state: State<'_, AppState>, file_id: String, get: Option<bool>) -> Result<()> {
    state.vault().await?.index.set_selection(&file_id, get)
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> Result<PullReport> {
    state.sync_once().await
}

#[tauri::command]
pub async fn check_out(state: State<'_, AppState>, file_id: String) -> Result<LockInfo> {
    state.vault().await?.check_out(&file_id).await
}

#[tauri::command]
pub async fn undo_check_out(state: State<'_, AppState>, file_id: String, discard_changes: bool) -> Result<()> {
    state.vault().await?.undo_check_out(&file_id, discard_changes).await
}

#[tauri::command]
pub async fn break_lock(state: State<'_, AppState>, file_id: String) -> Result<()> {
    state.vault().await?.break_lock(&file_id).await.map(|_| ())
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
    let map = PathMap::build(&v.drive_id, &all);
    let sel = v.index.selections()?;
    let checked_out = all
        .iter()
        .filter(|f| v.i_hold_lock(f) && map.contains(&f.id))
        .map(|f| entry(&v, f, &map, &all, &sel))
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

fn sidecar_path(cfg: &AppConfig) -> Option<PathBuf> {
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
    match (sidecar_path(&cfg).filter(|p| p.exists()), cfg.swdm_key) {
        (Some(exe), Some(key)) => {
            let reader = SidecarRefs::new(exe, key);
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
    state: State<'_, AppState>,
    files: Vec<String>,
    new_files: Vec<PathBuf>,
) -> Result<CheckInPreview> {
    plan_check_in(&state, &files, &new_files).await
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
    let out = v
        .push(PushRequest {
            files: preview.plan.files,
            new_files: preview.plan.new_files,
            message,
            keep_locked: args.keep_locked,
        })
        .await?;
    let _ = app.emit("vault-changed", ());
    Ok(out)
}

#[tauri::command]
pub async fn history(state: State<'_, AppState>, limit: Option<u32>) -> Result<Vec<Manifest>> {
    state.vault().await?.index.recent_changesets(limit.unwrap_or(50).min(500))
}

#[tauri::command]
pub async fn reveal_root(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let root = state.vault().await?.root.clone();
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
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
