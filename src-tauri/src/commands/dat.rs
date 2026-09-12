use crate::dat::{fetch, logiqx};
use crate::db::repo;
use crate::models::DatImportSummary;
use crate::state::AppState;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn pick_dat_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_file(move |file| {
        let _ = tx.send(file);
    });
    let result = rx.recv().map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
pub fn import_dat_file(
    system_id: i64,
    file_path: String,
    state: State<AppState>,
) -> Result<DatImportSummary, String> {
    let xml = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let parsed = logiqx::parse_dat(&xml).map_err(|e| e.to_string())?;

    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("import.dat")
        .to_string();

    let (games_imported, roms_imported) = repo::replace_dat_source(
        &mut conn,
        system_id,
        &file_name,
        parsed.dat_name.as_deref(),
        parsed.dat_version.as_deref(),
        &parsed.games,
    )
    .map_err(|e| e.to_string())?;

    Ok(DatImportSummary {
        games_imported,
        roms_imported,
        dat_name: parsed.dat_name,
        dat_version: parsed.dat_version,
    })
}

#[tauri::command]
pub fn fetch_dat_file(system_id: i64, state: State<AppState>) -> Result<DatImportSummary, String> {
    let folder_name = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::get_system_folder_name(&conn, system_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Unknown system".to_string())?
    };

    let (file_name, xml) = fetch::fetch_dat_text(&folder_name).map_err(|e| e.to_string())?;
    let parsed = logiqx::parse_dat(&xml).map_err(|e| e.to_string())?;

    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let (games_imported, roms_imported) = repo::replace_dat_source(
        &mut conn,
        system_id,
        &file_name,
        parsed.dat_name.as_deref(),
        parsed.dat_version.as_deref(),
        &parsed.games,
    )
    .map_err(|e| e.to_string())?;

    Ok(DatImportSummary {
        games_imported,
        roms_imported,
        dat_name: parsed.dat_name,
        dat_version: parsed.dat_version,
    })
}

#[tauri::command]
pub fn has_known_dat_source(folder_name: String) -> bool {
    fetch::has_known_source(&folder_name)
}
