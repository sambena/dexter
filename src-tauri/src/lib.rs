mod commands;
mod dat;
mod db;
mod models;
mod scanner;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");
            let db_path = app_dir.join("library.db");

            let conn = rusqlite::Connection::open(db_path).expect("failed to open database");
            db::schema::migrate(&conn).expect("failed to run database migrations");

            app.manage(AppState {
                db: std::sync::Mutex::new(conn),
                cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::pick_rom_root_folder,
            commands::settings::pick_emulator_path,
            commands::settings::list_systems,
            commands::settings::set_system_emulator,
            commands::dat::pick_dat_file,
            commands::dat::import_dat_file,
            commands::dat::fetch_dat_file,
            commands::dat::has_known_dat_source,
            commands::scan::scan_library,
            commands::hash::hash_pending_roms,
            commands::control::cancel_scan,
            commands::roms::list_roms,
            commands::roms::get_rom_details,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
