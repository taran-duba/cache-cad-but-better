pub mod app;
pub mod checkout;
pub mod drive;
pub mod error;
pub mod index;
pub mod names;
pub mod refs;
pub mod rename;
pub mod sync;
pub mod team;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch focuses this window and exits, so two
        // copies never sync into the same folder and index at once.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let log_dir = app::log_dir(app.handle());
            app.manage(app::logging::init(&log_dir));
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
            app::commands::set_refs_enabled,
            app::commands::get_config,
            app::commands::save_config,
            app::commands::sign_in,
            app::commands::sign_out,
            app::commands::start_demo,
            app::commands::list_children,
            app::file_ops::check_out_all,
            app::file_ops::check_out_with_refs,
            app::commands::sync_now,
            app::commands::check_out,
            app::commands::undo_check_out,
            app::commands::break_lock,
            app::commands::my_changes,
            app::commands::preview_check_in,
            app::commands::check_in,
            app::file_ops::history,
            app::file_ops::rename_item,
            app::file_ops::restore_push,
            app::commands::reveal_root,
            app::commands::open_file,
            app::commands::frontend_ready,
            app::commands::browse_locations,
            app::commands::browse_folders,
            app::commands::choose_folder,
            app::commands::switch_drive,
            app::commands::open_project,
            app::commands::remove_project,
            app::commands::set_vault_folder,
            app::commands::open_logs,
            app::commands::take_new_crash_report,
            app::commands::delete_from_drive,
            app::team_ops::get_team,
            app::team_ops::set_admins,
            app::team_ops::set_area,
            app::team_ops::open_in_drive,
            app::team_ops::create_drive_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
