//! Commands that work on many files at once: check out a folder or an assembly with its
//! parts, rename and move, and History (including restoring old versions).

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::commands::sidecar_path;
use super::AppState;
use crate::checkout::CheckOutReport;
use crate::error::Result;
use crate::index::Manifest;
use crate::refs::{SidecarRefs, StaticRefs};
use crate::rename::RenameReport;
use crate::sync::restore::RestoreReport;

/// The SolidWorks helper, if it's installed and reference checking is on.
pub(crate) async fn sidecar(state: &AppState) -> Option<SidecarRefs> {
    let cfg = state.config.read().await.clone();
    sidecar_path(&cfg).filter(|p| p.exists()).filter(|_| !cfg.refs_disabled).map(SidecarRefs::new)
}

/// "Check out all": select everything under a folder, then check out every file in it that
/// nobody holds, plus anything its assemblies use from other folders.
#[tauri::command]
pub async fn check_out_all(app: AppHandle, state: State<'_, AppState>, folder_id: Option<String>) -> Result<CheckOutReport> {
    let v = state.vault().await?;
    let id = folder_id.unwrap_or_else(|| v.root_id.clone());
    v.index.select_subtree(&id)?;
    let map = v.paths()?;
    let ids: Vec<String> = v
        .index
        .descendants(&id)?
        .into_iter()
        .filter(|f| !f.is_folder() && !f.trashed && map.contains(&f.id))
        .map(|f| f.id)
        .collect();
    let r = match sidecar(&state).await {
        Some(s) => v.check_out_many(&s, &ids).await,
        None => v.check_out_many(&StaticRefs::default(), &ids).await,
    };
    if let Ok(rep) = &r {
        tracing::info!(folder = %id, checked_out = rep.checked_out, deps = rep.deps_checked_out, "checked out folder");
    }
    let _ = app.emit("vault-changed", ());
    state.check_expired(&app, r).await
}

/// Check out a file plus everything an assembly references.
#[tauri::command]
pub async fn check_out_with_refs(app: AppHandle, state: State<'_, AppState>, file_id: String) -> Result<CheckOutReport> {
    let v = state.vault().await?;
    let r = match sidecar(&state).await {
        Some(s) => v.check_out_with_refs(&s, &file_id).await,
        None => v.check_out_with_refs(&StaticRefs::default(), &file_id).await,
    };
    if let Ok(rep) = &r {
        tracing::info!(file = %file_id, deps = rep.deps_checked_out, "checked out with references");
    }
    state.check_expired(&app, r).await
}

/// Rename or move a file or folder to `new_path` (relative to the CAD folder), updating every
/// assembly and drawing that uses it.
#[tauri::command]
pub async fn rename_item(app: AppHandle, state: State<'_, AppState>, file_id: String, new_path: String) -> Result<RenameReport> {
    let v = state.vault().await?;
    let new_rel = PathBuf::from(new_path.trim().replace('\\', "/"));
    let sidecar = sidecar(&state).await;
    // Keep the background sync out while files move on this PC.
    let guard = state.sync_lock.lock().await;
    let r = match &sidecar {
        Some(s) => v.rename(s, Some(s), &file_id, &new_rel).await,
        None => v.rename(&StaticRefs::default(), None::<&SidecarRefs>, &file_id, &new_rel).await,
    };
    drop(guard);
    match &r {
        Ok(rep) => tracing::info!(file = %file_id, to = %rep.new_path.display(), updated = rep.updated.len(), "renamed"),
        Err(e) => tracing::warn!(%e, file = %file_id, "rename failed"),
    }
    let r = state.check_expired(&app, r).await;
    let _ = app.emit("vault-changed", ());
    r
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFile {
    id: String,
    /// Where the file is now (None if it was deleted or moved out of the CAD folder).
    path: Option<String>,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    #[serde(flatten)]
    manifest: Manifest,
    file_list: Vec<HistoryFile>,
}

#[tauri::command]
pub async fn history(state: State<'_, AppState>, limit: Option<u32>) -> Result<Vec<HistoryEntry>> {
    let v = state.vault().await?;
    let map = v.paths()?;
    let mut out = Vec::new();
    for m in v.index.recent_changesets(limit.unwrap_or(50).min(500))? {
        let mut file_list = Vec::new();
        for id in m.files.keys() {
            let f = v.index.file(id)?;
            if f.as_ref().is_some_and(|f| f.is_folder()) {
                continue;
            }
            file_list.push(HistoryFile {
                id: id.clone(),
                path: map.path(id).map(|p| p.to_string_lossy().to_string()),
                name: f.map_or_else(|| "(deleted file)".to_string(), |f| f.name),
            });
        }
        file_list.sort_by(|a, b| a.path.cmp(&b.path));
        out.push(HistoryEntry { manifest: m, file_list });
    }
    Ok(out)
}

/// Put the files of a past push (or just `files` of them) back to how that push left them.
#[tauri::command]
pub async fn restore_push(
    app: AppHandle,
    state: State<'_, AppState>,
    changeset_id: String,
    files: Option<Vec<String>>,
) -> Result<RestoreReport> {
    let v = state.vault().await?;
    let guard = state.sync_lock.lock().await;
    let r = v.restore(&changeset_id, files.as_deref()).await;
    drop(guard);
    match &r {
        Ok(rep) => tracing::info!(from = %changeset_id, files = rep.restored.len(), "restored"),
        Err(e) => tracing::warn!(%e, from = %changeset_id, "restore failed"),
    }
    let r = state.check_expired(&app, r).await;
    let _ = app.emit("vault-changed", ());
    r
}
