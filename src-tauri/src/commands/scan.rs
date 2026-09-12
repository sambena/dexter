use crate::db::repo;
use crate::models::{ScanProgress, ScanSummary};
use crate::scanner::{archive, hashing, walker};
use crate::scanner::walker::ScanTarget;
use crate::state::AppState;
use tauri::{Emitter, State};

fn is_zip(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

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

    let system_folders = walker::list_system_folders(&root).map_err(|e| e.to_string())?;

    // Resolve/create a system row for every folder up front, and collect the scan targets.
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

    for (i, (system_id, target)) in jobs.iter().enumerate() {
        let display_path = match target {
            ScanTarget::File(p) => p.display().to_string(),
            ScanTarget::FolderRom(p) => p.display().to_string(),
        };
        let _ = app.emit(
            "scan://progress",
            ScanProgress {
                current: i + 1,
                total,
                current_file: display_path,
            },
        );

        let conn = state.db.lock().map_err(|e| e.to_string())?;

        match target {
            ScanTarget::FolderRom(path) => {
                // Whole-folder ROM dumps (e.g. extracted Wii U titles) contain far too many
                // internal files to hash meaningfully, and have no equivalent DAT to match
                // against — list the folder itself as a single unmatched ROM.
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                repo::upsert_rom(
                    &conn,
                    *system_id,
                    &path.display().to_string(),
                    &file_name,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(|e| e.to_string())?;
                summary.unmatched += 1;
                summary.scanned_files += 1;
            }
            ScanTarget::File(path) if is_zip(path) => match archive::hash_zip_entries(path) {
                Ok(entries) => {
                    for entry in entries {
                        let file_path = format!("{}::{}", path.display(), entry.inner_name);
                        let dat_rom_id = repo::find_dat_rom_match(
                            &conn,
                            *system_id,
                            &entry.hashes.crc32,
                            &entry.hashes.sha1,
                            &entry.hashes.md5,
                        )
                        .map_err(|e| e.to_string())?;
                        if dat_rom_id.is_some() {
                            summary.matched += 1;
                        } else {
                            summary.unmatched += 1;
                        }
                        repo::upsert_rom(
                            &conn,
                            *system_id,
                            &file_path,
                            &entry.inner_name,
                            Some(entry.hashes.size as i64),
                            Some(&entry.hashes.crc32),
                            Some(&entry.hashes.md5),
                            Some(&entry.hashes.sha1),
                            Some(&entry.inner_name),
                            dat_rom_id,
                        )
                        .map_err(|e| e.to_string())?;
                        summary.scanned_files += 1;
                    }
                }
                Err(e) => summary.errors.push(format!("{}: {}", path.display(), e)),
            },
            ScanTarget::File(path) => match hashing::hash_file(path) {
                Ok(hashes) => {
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_string();
                    let dat_rom_id = repo::find_dat_rom_match(
                        &conn,
                        *system_id,
                        &hashes.crc32,
                        &hashes.sha1,
                        &hashes.md5,
                    )
                    .map_err(|e| e.to_string())?;
                    if dat_rom_id.is_some() {
                        summary.matched += 1;
                    } else {
                        summary.unmatched += 1;
                    }
                    repo::upsert_rom(
                        &conn,
                        *system_id,
                        &path.display().to_string(),
                        &file_name,
                        Some(hashes.size as i64),
                        Some(&hashes.crc32),
                        Some(&hashes.md5),
                        Some(&hashes.sha1),
                        None,
                        dat_rom_id,
                    )
                    .map_err(|e| e.to_string())?;
                    summary.scanned_files += 1;
                }
                Err(e) => summary.errors.push(format!("{}: {}", path.display(), e)),
            },
        }
    }

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for folder in &system_folders {
            if let Ok(system_id) = repo::get_or_create_system_by_folder(&conn, &folder.folder_name) {
                summary.removed += repo::prune_system_unseen(&conn, system_id).map_err(|e| e.to_string())?;
            }
        }
    }

    let _ = app.emit("scan://done", summary.clone());
    Ok(summary)
}
