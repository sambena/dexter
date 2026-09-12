use crate::db::repo;
use crate::models::{Settings, SystemDto};
use crate::state::AppState;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::get_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings(settings: Settings, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::save_settings(&conn, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_rom_root_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let result = rx.recv().map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn pick_emulator_path(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_file(move |file| {
        let _ = tx.send(file);
    });
    let result = rx.recv().map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
pub fn list_systems(state: State<AppState>) -> Result<Vec<SystemDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::list_systems(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_system_emulator(
    system_id: i64,
    emulator_path: Option<String>,
    emulator_args: Option<String>,
    state: State<AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::set_system_emulator(&conn, system_id, emulator_path.as_deref(), emulator_args.as_deref())
        .map_err(|e| e.to_string())
}
