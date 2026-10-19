pub mod app;
pub mod drive;
pub mod error;
pub mod index;
pub mod names;
pub mod refs;
pub mod sync;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "cadsync=info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(app::AppState::new(app.handle()));
            match app::smoke::requested() {
                Some(out) => app::smoke::start(app.handle().clone(), out),
                None => {
                    app::restore_session(app.handle().clone());
                    app::spawn_poller(app.handle().clone());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::commands::get_status,
            app::commands::get_config,
            app::commands::save_config,
            app::commands::sign_in,
            app::commands::sign_out,
            app::commands::start_demo,
            app::commands::list_children,
            app::commands::set_selection,
            app::commands::sync_now,
            app::commands::check_out,
            app::commands::undo_check_out,
            app::commands::break_lock,
            app::commands::my_changes,
            app::commands::preview_check_in,
            app::commands::check_in,
            app::commands::history,
            app::commands::reveal_root,
            app::commands::frontend_ready,
            app::commands::list_shared_drives,
            app::commands::choose_drive,
            app::commands::switch_drive,
            app::commands::get_team_settings,
            app::commands::save_team_settings,
            app::commands::set_vault_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
