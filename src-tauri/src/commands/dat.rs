use crate::dat::{fetch, logiqx, match_system};
use crate::db::repo;
use crate::models::{DatFolderImportSummary, DatFolderImportedEntry, DatImportSummary, DatSourceDto};
use crate::state::AppState;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn pick_dat_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .add_filter("DAT files", &["dat", "xml", "zip"])
        .add_filter("All files", &["*"])
        .pick_file(move |file| {
            let _ = tx.send(file);
        });
    let result = rx.recv().map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}

/// Runs on a blocking thread: parsing a DAT and inserting its rows takes long
/// enough that doing it inline would freeze the window (sync commands run on
/// the main/UI thread).
#[tauri::command]
pub async fn import_dat_file(
    system_id: i64,
    file_path: String,
    app: tauri::AppHandle,
) -> Result<DatImportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || import_dat_file_blocking(system_id, file_path, &app))
        .await
        .map_err(|e| e.to_string())?
}

fn import_dat_file_blocking(
    system_id: i64,
    file_path: String,
    app: &tauri::AppHandle,
) -> Result<DatImportSummary, String> {
    let state = app.state::<AppState>();
    let bytes = std::fs::read(&file_path).map_err(|e| e.to_string())?;
    let picked_name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("import.dat");
    let (file_name, xml) = fetch::extract_dat_text(&bytes, picked_name).map_err(|e| e.to_string())?;
    let parsed = logiqx::parse_dat(&xml).map_err(|e| e.to_string())?;

    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let (games_imported, roms_imported) = repo::add_dat_source(
        &mut conn,
        system_id,
        &file_name,
        parsed.dat_name.as_deref(),
        parsed.dat_version.as_deref(),
        &parsed.games,
    )
    .map_err(|e| e.to_string())?;
    let (newly_matched, _) = repo::rematch_system(&mut conn, system_id).map_err(|e| e.to_string())?;

    Ok(DatImportSummary {
        games_imported,
        roms_imported,
        newly_matched,
        dat_name: parsed.dat_name,
        dat_version: parsed.dat_version,
    })
}

#[tauri::command]
pub async fn pick_dat_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let result = rx.recv().map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}

/// Imports every .dat/.xml/.zip file found directly inside a folder, matching
/// each one to an existing system by its DAT header name (best-effort — see
/// dat::match_system). Files that can't be confidently matched are reported
/// back so the user can import them individually and pick the system by hand.
#[tauri::command]
pub async fn import_dat_folder(
    folder_path: String,
    app: tauri::AppHandle,
) -> Result<DatFolderImportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || import_dat_folder_blocking(folder_path, &app))
        .await
        .map_err(|e| e.to_string())?
}

fn import_dat_folder_blocking(
    folder_path: String,
    app: &tauri::AppHandle,
) -> Result<DatFolderImportSummary, String> {
    let state = app.state::<AppState>();
    let dir = std::path::Path::new(&folder_path);
    if !dir.is_dir() {
        return Err(format!("Folder does not exist: {}", folder_path));
    }

    let mut candidate_files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_lowercase().as_str(), "dat" | "xml" | "zip"))
                .unwrap_or(false)
        })
        .collect();
    candidate_files.sort();

    let mut summary = DatFolderImportSummary::default();

    for path in candidate_files {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                summary.errors.push(format!("{}: {}", file_name, e));
                continue;
            }
        };
        let (inner_name, xml) = match fetch::extract_dat_text(&bytes, &file_name) {
            Ok(v) => v,
            Err(e) => {
                summary.errors.push(format!("{}: {}", file_name, e));
                continue;
            }
        };
        let parsed = match logiqx::parse_dat(&xml) {
            Ok(v) => v,
            Err(e) => {
                summary.errors.push(format!("{}: {}", file_name, e));
                continue;
            }
        };

        // Re-fetch systems each iteration so a match considers systems just
        // updated by this same folder import (not that it should matter, but
        // it's cheap and keeps this loop simple).
        let systems = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            repo::list_systems(&conn).map_err(|e| e.to_string())?
        };
        let match_name = parsed.dat_name.as_deref().unwrap_or(&file_name);
        let Some(system_id) = match_system::find_matching_system(&systems, match_name) else {
            summary.unmatched.push(file_name);
            continue;
        };
        let system_name = systems
            .iter()
            .find(|s| s.id == system_id)
            .map(|s| s.name.clone())
            .unwrap_or_default();

        let mut conn = state.db.lock().map_err(|e| e.to_string())?;
        match repo::add_dat_source(
            &mut conn,
            system_id,
            &inner_name,
            parsed.dat_name.as_deref(),
            parsed.dat_version.as_deref(),
            &parsed.games,
        ) {
            Ok((games_imported, roms_imported)) => {
                let newly_matched = repo::rematch_system(&mut conn, system_id)
                    .map(|(n, _)| n)
                    .unwrap_or(0);
                summary.imported.push(DatFolderImportedEntry {
                    file_name,
                    system_name,
                    games_imported,
                    roms_imported,
                    newly_matched,
                });
            }
            Err(e) => summary.errors.push(format!("{}: {}", file_name, e)),
        }
    }

    Ok(summary)
}

#[tauri::command]
pub async fn fetch_dat_file(system_id: i64, app: tauri::AppHandle) -> Result<DatImportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_dat_file_blocking(system_id, &app))
        .await
        .map_err(|e| e.to_string())?
}

fn fetch_dat_file_blocking(system_id: i64, app: &tauri::AppHandle) -> Result<DatImportSummary, String> {
    let state = app.state::<AppState>();
    let source = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::get_system_dat_source(&conn, system_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Unknown system".to_string())?
    };

    let (file_name, xml) = match source.dat_url.as_deref().filter(|u| !u.is_empty()) {
        Some(url) => fetch::fetch_dat_from_url(url).map_err(|e| e.to_string())?,
        None => fetch::fetch_dat_text(&source.folder_name).map_err(|e| e.to_string())?,
    };
    let parsed = logiqx::parse_dat(&xml).map_err(|e| e.to_string())?;

    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let (games_imported, roms_imported) = repo::add_dat_source(
        &mut conn,
        system_id,
        &file_name,
        parsed.dat_name.as_deref(),
        parsed.dat_version.as_deref(),
        &parsed.games,
    )
    .map_err(|e| e.to_string())?;
    let (newly_matched, _) = repo::rematch_system(&mut conn, system_id).map_err(|e| e.to_string())?;

    Ok(DatImportSummary {
        games_imported,
        roms_imported,
        newly_matched,
        dat_name: parsed.dat_name,
        dat_version: parsed.dat_version,
    })
}

#[tauri::command]
pub fn has_known_dat_source(folder_name: String) -> bool {
    fetch::has_known_source(&folder_name)
}

#[tauri::command]
pub fn set_system_dat_url(system_id: i64, dat_url: Option<String>, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::set_system_dat_url(&conn, system_id, dat_url.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_dat_sources(state: State<AppState>) -> Result<Vec<DatSourceDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    repo::list_dat_sources(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_dat_source(dat_source_id: i64, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::remove_dat_source(&mut conn, dat_source_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
