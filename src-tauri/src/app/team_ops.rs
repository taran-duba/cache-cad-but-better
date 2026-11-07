//! Commands for team roles (CADmins and who edits which area) and for Drive itself:
//! opening the CAD folder on the Drive website and making a new CAD folder.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use super::commands::SHARED_WITH_ME;
use super::config::is_valid_id;
use super::AppState;
use crate::drive::http::BrowseFolder;
use crate::drive::DriveApi;
use crate::error::{Error, Result};
use crate::team::{AreaInfo, Team};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamView {
    admins: Vec<String>,
    areas: Vec<AreaInfo>,
}

async fn team_view(state: &AppState, team: &Team) -> Result<TeamView> {
    let v = state.vault().await?;
    let map = v.paths()?;
    let mut areas: Vec<AreaInfo> = team.areas.iter().map(|a| v.area_info(a, &map)).collect();
    areas.sort_by_key(|a| a.path.to_lowercase());
    Ok(TeamView { admins: team.admins.clone(), areas })
}

#[tauri::command]
pub async fn get_team(state: State<'_, AppState>) -> Result<TeamView> {
    let team = state.vault().await?.team()?;
    team_view(&state, &team).await
}

/// Replace the CADmin list (areas are kept). An empty list turns roles off.
#[tauri::command]
pub async fn set_admins(app: AppHandle, state: State<'_, AppState>, admins: Vec<String>) -> Result<TeamView> {
    let v = state.vault().await?;
    let mut team = v.team()?;
    team.admins = admins;
    if team.admins.iter().all(|a| a.trim().is_empty()) {
        // Roles off: areas would do nothing, and normalizing refuses areas without CADmins.
        team.areas.clear();
    }
    let r = v.save_team(team).await;
    let team = state.check_expired(&app, r).await?;
    let _ = app.emit("vault-changed", ());
    team_view(&state, &team).await
}

/// Give a folder or file its own editors; `None` removes the area.
#[tauri::command]
pub async fn set_area(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: String,
    editors: Option<Vec<String>>,
) -> Result<TeamView> {
    if !is_valid_id(&file_id) {
        return Err(Error::Config("invalid file".into()));
    }
    let r = state.vault().await?.set_area(&file_id, editors).await;
    let team = state.check_expired(&app, r).await?;
    let _ = app.emit("vault-changed", ());
    team_view(&state, &team).await
}

/// Open the CAD folder, or one file or folder in it, on the Google Drive website.
#[tauri::command]
pub async fn open_in_drive(app: AppHandle, state: State<'_, AppState>, file_id: Option<String>) -> Result<()> {
    if state.demo.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(Error::Config("demo mode has no Google Drive to open".into()));
    }
    let v = state.vault().await?;
    let id = file_id.unwrap_or_else(|| v.root_id.clone());
    if !is_valid_id(&id) {
        return Err(Error::Config("invalid file".into()));
    }
    let is_folder = id == v.root_id || v.index.file(&id)?.is_some_and(|f| f.is_folder());
    let url = if is_folder {
        format!("https://drive.google.com/drive/folders/{id}")
    } else {
        format!("https://drive.google.com/file/d/{id}/view")
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

/// Make a new folder on Drive (to start a new CAD folder from the picker).
#[tauri::command]
pub async fn create_drive_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    parent_id: String,
    name: String,
) -> Result<BrowseFolder> {
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 200 || name.chars().any(char::is_control) {
        return Err(Error::Config("give the folder a name (up to 200 characters)".into()));
    }
    if parent_id == SHARED_WITH_ME || !(parent_id == "root" || is_valid_id(&parent_id)) {
        return Err(Error::Config("pick a shared drive, My Drive, or a folder to put it in".into()));
    }
    let r = state.http().await?.create_folder(&parent_id, &name).await;
    let f = state.check_expired(&app, r).await?;
    Ok(BrowseFolder { id: f.id, name: f.name })
}
