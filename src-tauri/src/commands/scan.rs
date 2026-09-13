use crate::db::repo;
use crate::models::{ScanProgress, ScanSummary};
use crate::scanner::walker::ScanTarget;
use crate::scanner::{archive, disc, formats, walker, wiiu};
use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::{Emitter, State};

fn is_zip(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

/// Quick file-discovery scan: walks the ROM root and records every file (and, for
/// .zip archives, every inner entry) without hashing anything, so the library shows
/// up immediately. Hashing/DAT-matching is a separate, explicit `hash_pending_roms` pass.
#[tauri::command]
pub async fn scan_library(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let root_path = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::get_settings(&conn)
            .map_err(|e| e.to_string())?
            .rom_root_path
            .ok_or_else(|| "No ROM root folder configured. Set one in Settings first.".to_string())?
    };
    let root = std::path::PathBuf::from(&root_path);
    if !root.is_dir() {
        return Err(format!("ROM root folder does not exist: {}", root_path));
    }

    state.cancel_flag.store(false, Ordering::SeqCst);

    let system_folders = walker::list_system_folders(&root).map_err(|e| e.to_string())?;

    let mut jobs: Vec<(i64, ScanTarget)> = Vec::new();
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for folder in &system_folders {
            let system_id = repo::get_or_create_system_by_folder(&conn, &folder.folder_name)
                .map_err(|e| e.to_string())?;
            repo::mark_system_unseen(&conn, system_id).map_err(|e| e.to_string())?;
            for target in walker::list_scan_targets(&folder.path) {
                jobs.push((system_id, target));
            }
        }
    }

    let total = jobs.len();
    let mut summary = ScanSummary::default();

    let mut cancelled = false;
    for (i, (system_id, target)) in jobs.iter().enumerate() {
        if state.cancel_flag.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        if i % 25 == 0 || i + 1 == total {
            let display_path = match target {
                ScanTarget::File(p) => p.display().to_string(),
                ScanTarget::FolderRom(p) => p.display().to_string(),
            };
            let _ = app.emit(
                "scan://progress",
                ScanProgress { current: i + 1, total, current_file: display_path },
            );
        }

        // Read before taking the lock: it's a couple of small files over the network.
        let title_info = match target {
            ScanTarget::FolderRom(path) => wiiu::read_title_info(path),
            ScanTarget::File(path) if path.file_name().and_then(|n| n.to_str()).is_some_and(disc::is_disc_image) => {
                disc::read_disc_title(path)
            }
            ScanTarget::File(_) => None,
        };
        let conn = state.db.lock().map_err(|e| e.to_string())?;

        match target {
            ScanTarget::FolderRom(path) => {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                let file_path = path.display().to_string();
                repo::upsert_unverifiable_rom(&conn, *system_id, &file_path, &file_name, None, None)
                    .map_err(|e| e.to_string())?;
                repo::set_title_info(&conn, &file_path, title_info.as_ref()).map_err(|e| e.to_string())?;
                summary.unverifiable += 1;
                summary.scanned_files += 1;
            }
            ScanTarget::File(path) if is_zip(path) => match archive::list_zip_entries(path) {
                Ok(entries) => {
                    for entry in entries {
                        let file_path = format!("{}::{}", path.display(), entry.inner_name);
                        let upsert = if formats::is_unverifiable(&entry.inner_name) {
                            summary.unverifiable += 1;
                            repo::upsert_unverifiable_rom
                        } else {
                            summary.pending += 1;
                            repo::upsert_pending_rom
                        };
                        upsert(
                            &conn,
                            *system_id,
                            &file_path,
                            &entry.inner_name,
                            Some(entry.size as i64),
                            Some(&entry.inner_name),
                        )
                        .map_err(|e| e.to_string())?;
                        summary.scanned_files += 1;
                    }
                }
                Err(e) => summary.errors.push(format!("{}: {}", path.display(), e)),
            },
            ScanTarget::File(path) => {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                let size = std::fs::metadata(path).ok().map(|m| m.len() as i64);
                let upsert = if formats::is_unverifiable(&file_name) {
                    summary.unverifiable += 1;
                    repo::upsert_unverifiable_rom
                } else {
                    summary.pending += 1;
                    repo::upsert_pending_rom
                };
                let file_path = path.display().to_string();
                upsert(&conn, *system_id, &file_path, &file_name, size, None).map_err(|e| e.to_string())?;
                if disc::is_disc_image(&file_name) {
                    repo::set_title_info(&conn, &file_path, title_info.as_ref()).map_err(|e| e.to_string())?;
                }
                summary.scanned_files += 1;
            }
        }
    }

    if !cancelled {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for folder in &system_folders {
            if let Ok(system_id) = repo::get_or_create_system_by_folder(&conn, &folder.folder_name) {
                summary.removed += repo::prune_system_unseen(&conn, system_id).map_err(|e| e.to_string())?;
                repo::identify_titles(&conn, Some(system_id)).map_err(|e| e.to_string())?;
            }
        }
    }

    let _ = app.emit("scan://done", summary.clone());
    Ok(summary)
}
