use crate::db::repo;
use crate::models::{RomDetailsDto, RomFilter, RomListItemDto};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn list_roms(filter: RomFilter, state: State<AppState>) -> Result<Vec<RomListItemDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::list_roms(&conn, &filter).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_rom_details(rom_id: i64, state: State<AppState>) -> Result<Option<RomDetailsDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::get_rom_details(&conn, rom_id).map_err(|e| e.to_string())
}
