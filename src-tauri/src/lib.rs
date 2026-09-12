mod api;
mod art;
mod cli;
mod commands;
mod dat;
mod db;
mod models;
mod scanner;
mod state;

use state::AppState;
use tauri::Manager;

fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

/// Opens the library database and registers the shared state. Used by the
/// window and by dexter-cli when it runs without the app open.
fn init_state(app: &tauri::AppHandle) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_dir).map_err(|e| format!("failed to create app data dir: {}", e))?;
    let db_path = app_dir.join("library.db");

    let conn = rusqlite::Connection::open(db_path).map_err(|e| format!("failed to open database: {}", e))?;
    // The window and dexter-cli can both have the database open, so wait for
    // a lock briefly rather than failing immediately.
    conn.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    db::schema::migrate(&conn).map_err(|e| format!("failed to run database migrations: {}", e))?;

    app.manage(AppState {
        db: std::sync::Mutex::new(conn),
        cancel_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    });
    Ok(())
}

pub fn run_cli(args: Vec<String>) -> i32 {
    cli::run(args)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            init_state(app.handle())?;

            // The window is created here rather than from tauri.conf.json so
            // that dexter-cli can start the app without one.
            let window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .ok_or("no main window in tauri.conf.json")?
                .clone();
            tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)?.build()?;

            // The app still works without the API, so a failure to start it
            // (e.g. no free port) is reported rather than fatal.
            if let Err(e) = api::server::start(app.handle()) {
                eprintln!("{}", e);
            }
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
            commands::dat::pick_dat_folder,
            commands::dat::import_dat_folder,
            commands::dat::fetch_dat_file,
            commands::dat::has_known_dat_source,
            commands::dat::set_system_dat_url,
            commands::dat::list_dat_sources,
            commands::dat::remove_dat_source,
            commands::scan::scan_library,
            commands::hash::hash_pending_roms,
            commands::control::cancel_scan,
            commands::art::has_known_box_art_source,
            commands::art::get_box_art,
            commands::art::fetch_box_art,
            commands::art::fetch_all_box_art,
            commands::art::pick_and_set_box_art,
            commands::roms::list_roms,
            commands::roms::get_rom_details,
            commands::launch::launch_rom,
            commands::maintenance::list_duplicates,
            commands::maintenance::preview_renames,
            commands::maintenance::apply_renames,
            commands::maintenance::delete_roms,
        ])
        .build(context())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                api::server::stop(app);
            }
        });
}
