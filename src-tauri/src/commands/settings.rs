use crate::db::repo;
use crate::models::{MaintenanceSummary, ScanProgress, Settings, StorageLocationsDto, SystemDto};
use crate::state::AppState;
use crate::storage;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
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
pub fn get_storage_locations(app: AppHandle) -> Result<StorageLocationsDto, String> {
    let app_folder = storage::app_folder(&app)?;
    let locations = storage::load(&app_folder)?;
    Ok(StorageLocationsDto {
        library_folder: locations.library_folder(&app_folder).to_string_lossy().into_owned(),
        library_is_default: locations.library_folder.is_none(),
        art_folder: locations.art_folder(&app_folder).to_string_lossy().into_owned(),
        art_is_default: locations.art_folder.is_none(),
        app_folder: app_folder.to_string_lossy().into_owned(),
    })
}

/// Moves library.db into `folder` (None: back to the app data folder) and
/// carries on with the copy there.
#[tauri::command]
pub async fn move_library_database(folder: Option<String>, app: AppHandle) -> Result<StorageLocationsDto, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || move_library_database_blocking(folder.map(PathBuf::from), &handle))
        .await
        .map_err(|e| e.to_string())??;
    get_storage_locations(app)
}

fn move_library_database_blocking(folder: Option<PathBuf>, app: &AppHandle) -> Result<(), String> {
    let app_folder = storage::app_folder(app)?;
    let mut locations = storage::load(&app_folder)?;
    let current = locations.database_path(&app_folder);
    let target_folder = folder.unwrap_or_else(|| app_folder.clone());
    if storage::same_path(&locations.library_folder(&app_folder), &target_folder) {
        return Err(format!("The library is already in {}.", target_folder.display()));
    }

    let state = app.state::<AppState>();
    // Held throughout, so nothing writes to the old copy after it's taken.
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let (copy, _) = storage::copy_database(&conn, &target_folder)?;
    locations.library_folder = storage::chosen(Some(&target_folder), &app_folder);
    storage::save(&app_folder, &locations)?;
    drop(std::mem::replace(&mut *conn, copy));
    drop(conn);
    // The move has happened; a leftover file is only untidy.
    if let Err(e) = storage::remove_database(&current) {
        eprintln!("{}", e);
    }
    Ok(())
}

/// Moves the box art images into `folder` (None: back to the default) and
/// saves new art there.
#[tauri::command]
pub async fn move_box_art(folder: Option<String>, app: AppHandle) -> Result<MaintenanceSummary, String> {
    tauri::async_runtime::spawn_blocking(move || move_box_art_blocking(folder.map(PathBuf::from), &app))
        .await
        .map_err(|e| e.to_string())?
}

fn move_box_art_blocking(folder: Option<PathBuf>, app: &AppHandle) -> Result<MaintenanceSummary, String> {
    let app_folder = storage::app_folder(app)?;
    let mut locations = storage::load(&app_folder)?;
    let previous = locations.art_folder(&app_folder);
    let default = storage::default_art_folder(&app_folder);
    let target = folder.unwrap_or_else(|| default.clone());
    if storage::same_path(&previous, &target) {
        return Err(format!("Box art is already saved in {}.", target.display()));
    }

    // Saved first, so art downloaded during the move already goes to the new
    // folder. Each image's row is updated as it moves, so a failure part way
    // leaves the rest working where they are.
    std::fs::create_dir_all(&target).map_err(|e| format!("failed to create {}: {}", target.display(), e))?;
    locations.art_folder = storage::chosen(Some(&target), &default);
    storage::save(&app_folder, &locations)?;

    let state = app.state::<AppState>();
    let moved = storage::move_art(&state.db, &previous, &target, |current, total, name| {
        let _ = app.emit(
            "storage://progress",
            ScanProgress { current, total, current_file: name.to_string() },
        );
    })?;
    Ok(MaintenanceSummary {
        succeeded: moved.moved,
        skipped: moved.missing,
        errors: moved.errors,
        moved_to: vec![target.to_string_lossy().into_owned()],
    })
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
