//! Start-up smoke test, used by CI on a Windows runner.
//!
//! `CADSYNC_SMOKE=<result.json> cadsync.exe` runs the real app (window,
//! webview, IPC, SQLite, file system) against the demo vault, then exits:
//! 0 if everything worked, 1 otherwise. The result file says which checks
//! passed. Nothing touches Google Drive.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use super::AppState;
use crate::refs::SidecarRefs;
use crate::sync::localfs;

const ENV: &str = "CADSYNC_SMOKE";
const HOLD_ENV: &str = "CADSYNC_SMOKE_HOLD_SECS";
const UI_TIMEOUT: Duration = Duration::from_secs(90);
const SIDECAR_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Smoke {
    ui_ready: AtomicBool,
    notify: Notify,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SmokeResult {
    ok: bool,
    os: &'static str,
    demo_vault: bool,
    files_downloaded: usize,
    local_files_read_only: bool,
    sidecar: Option<String>,
    /// A Google OAuth client was built into this app (only the id's shape is
    /// reported, never the id or secret).
    sign_in_built_in: bool,
    ui_ready: bool,
    elapsed_ms: u128,
    error: Option<String>,
}

pub fn requested() -> Option<PathBuf> {
    std::env::var_os(ENV).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Called by the UI once it has rendered and talked to Rust.
pub fn mark_ui_ready(app: &AppHandle) {
    if let Some(s) = app.try_state::<Smoke>() {
        s.ui_ready.store(true, Ordering::SeqCst);
        s.notify.notify_one();
    }
}

pub fn start(app: AppHandle, out: PathBuf) {
    app.manage(Smoke::default());
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let mut result = SmokeResult { os: std::env::consts::OS, ..Default::default() };
        if let Err(e) = run(&app, &mut result).await {
            result.error = Some(e);
        }
        result.ok = result.error.is_none();
        result.elapsed_ms = started.elapsed().as_millis();

        let json = serde_json::to_string_pretty(&result).unwrap_or_default();
        println!("cadsync smoke test result:\n{json}");
        if let Err(e) = std::fs::write(&out, &json) {
            eprintln!("could not write {}: {e}", out.display());
        }
        // Optionally keep the window up so CI can take a screenshot.
        let hold = std::env::var(HOLD_ENV).ok().and_then(|s| s.parse().ok()).unwrap_or(0u64).min(120);
        tokio::time::sleep(Duration::from_secs(hold)).await;
        app.exit(if result.ok { 0 } else { 1 });
    });
}

async fn run(app: &AppHandle, r: &mut SmokeResult) -> Result<(), String> {
    let state = app.state::<AppState>();

    state.open_demo().await.map_err(|e| format!("open demo vault: {e}"))?;
    r.demo_vault = true;

    let report = state.sync_once().await.map_err(|e| format!("sync: {e}"))?;
    if !report.errors.is_empty() {
        return Err(format!("sync errors: {:?}", report.errors));
    }
    r.files_downloaded = report.downloaded;
    if report.downloaded == 0 {
        return Err("sync downloaded no files".into());
    }

    let vault = state.vault().await.map_err(|e| e.to_string())?;
    let locals = vault.index.all_local().map_err(|e| e.to_string())?;
    r.local_files_read_only = !locals.is_empty()
        && locals.iter().all(|l| localfs::is_read_only(&vault.abs(l.rel_path.as_ref())));
    if !r.local_files_read_only {
        return Err("synced files are not read-only".into());
    }

    let cfg = state.config.read().await.clone();
    match super::commands::sidecar_path(&cfg).filter(|p| p.exists()) {
        Some(exe) => {
            let refs = SidecarRefs::new(exe.clone(), String::new());
            let version = tokio::time::timeout(SIDECAR_TIMEOUT, refs.ping())
                .await
                .map_err(|_| format!("SwRefs at {} didn't answer", exe.display()))?
                .map_err(|e| format!("SwRefs ping: {e}"))?;
            r.sidecar = Some(version);
        }
        // The installer bundles SwRefs only on Windows.
        None if cfg!(windows) => return Err("SwRefs.exe is not next to cadsync.exe".into()),
        None => {}
    }

    let smoke = app.state::<Smoke>();
    if !smoke.ui_ready.load(Ordering::SeqCst) {
        tokio::time::timeout(UI_TIMEOUT, smoke.notify.notified())
            .await
            .map_err(|_| "the UI never reported it was ready (webview didn't load?)".to_string())?;
    }
    r.ui_ready = true;

    r.sign_in_built_in = crate::drive::auth::OAuthClient::built_in()
        .is_some_and(|c| c.client_id.ends_with(".apps.googleusercontent.com"));
    // CI sets this when the repo secrets are configured, so a build that
    // silently lost its Google client fails instead of shipping without sign-in.
    if std::env::var_os("CADSYNC_SMOKE_REQUIRE_SIGNIN").is_some() && !r.sign_in_built_in {
        return Err("this build has no Google sign-in built in (check the CADSYNC_GOOGLE_CLIENT_ID/SECRET repo secrets)".into());
    }
    Ok(())
}
