use crate::db::repo;
use crate::models::{DuplicateGroupDto, MaintenanceSummary, RenamePlanEntryDto};
use crate::state::AppState;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[tauri::command]
pub async fn list_duplicates(app: tauri::AppHandle) -> Result<Vec<DuplicateGroupDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::list_duplicate_groups(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Windows rejects these in file names, and a DAT name can legitimately
/// contain ':' (e.g. "Game: Subtitle"), so they're replaced rather than
/// letting the rename fail at the filesystem.
fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect::<String>()
        .trim_end_matches([' ', '.'])
        .to_string()
}

/// ROM rows key archive members as "<archive path>::<member>", so the value in
/// file_path is not a path on disk for those rows. Everything that touches the
/// filesystem goes through here first.
fn on_disk_path(file_path: &str, archive_member: Option<&str>) -> String {
    match archive_member {
        Some(member) => file_path
            .strip_suffix(&format!("::{}", member))
            .unwrap_or(file_path)
            .to_string(),
        None => file_path.to_string(),
    }
}

fn extension_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_string())
}

/// The canonical file name for a ROM, given what it is currently called on
/// disk. The extension is deliberately taken from the existing file rather
/// than the DAT: No-Intro lists Atari 2600 dumps as .a26 while real libraries
/// keep them as .bin, and silently changing an extension breaks whatever
/// emulator association the file already has. Only the name is canonicalised.
fn proposed_name(
    dat_rom_name: &str,
    dat_game_name: &str,
    archive_member: Option<&str>,
    current_on_disk: &str,
) -> String {
    let ext = extension_of(current_on_disk);
    match archive_member {
        // The file on disk is the archive, so it takes the DAT game name.
        Some(_) => format!(
            "{}.{}",
            sanitize_file_name(dat_game_name),
            ext.unwrap_or_else(|| "zip".into())
        ),
        None => {
            let stem = Path::new(dat_rom_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(dat_rom_name);
            match ext {
                Some(ext) => format!("{}.{}", sanitize_file_name(stem), ext),
                None => sanitize_file_name(dat_rom_name),
            }
        }
    }
}

#[tauri::command]
pub async fn preview_renames(app: tauri::AppHandle) -> Result<Vec<RenamePlanEntryDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let candidates = repo::list_rename_candidates(&conn).map_err(|e| e.to_string())?;

        let mut plan = Vec::new();
        for c in candidates {
            let disk_path = on_disk_path(&c.file_path, c.archive_member.as_deref());
            let path = PathBuf::from(&disk_path);
            let Some(current_on_disk) = path.file_name().and_then(|n| n.to_str()).map(|n| n.to_string())
            else {
                continue;
            };

            let proposed = proposed_name(
                &c.dat_rom_name,
                &c.dat_game_name,
                c.archive_member.as_deref(),
                &current_on_disk,
            );

            if proposed.is_empty() || proposed == current_on_disk {
                continue;
            }

            let mut blocked_reason = None;
            if c.archive_member.is_some() {
                let members = repo::count_roms_for_file(&conn, &disk_path).map_err(|e| e.to_string())?;
                if members > 1 {
                    blocked_reason = Some(format!(
                        "Archive holds {} ROMs, so no single DAT name fits it",
                        members
                    ));
                }
            }
            if blocked_reason.is_none() {
                let target = path.with_file_name(&proposed);
                if target.exists() {
                    blocked_reason = Some("A file with that name already exists".to_string());
                }
            }

            plan.push(RenamePlanEntryDto {
                rom_id: c.rom_id,
                current_name: current_on_disk,
                new_name: proposed,
                target_path: disk_path,
                blocked_reason,
            });
        }
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn apply_renames(rom_ids: Vec<i64>, app: tauri::AppHandle) -> Result<MaintenanceSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut summary = MaintenanceSummary::default();

        // The plan is recomputed rather than trusting names sent from the UI,
        // so a stale preview can't rename a file to something unintended.
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let candidates = repo::list_rename_candidates(&conn).map_err(|e| e.to_string())?;

        for c in candidates.iter().filter(|c| rom_ids.contains(&c.rom_id)) {
            let disk_path = on_disk_path(&c.file_path, c.archive_member.as_deref());
            let path = PathBuf::from(&disk_path);
            let current_on_disk = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => {
                    summary.errors.push(format!("{}: unreadable path", c.file_name));
                    continue;
                }
            };

            let proposed = proposed_name(
                &c.dat_rom_name,
                &c.dat_game_name,
                c.archive_member.as_deref(),
                &current_on_disk,
            );

            if proposed.is_empty() || proposed == current_on_disk {
                summary.skipped += 1;
                continue;
            }
            if c.archive_member.is_some()
                && repo::count_roms_for_file(&conn, &disk_path).map_err(|e| e.to_string())? > 1
            {
                summary.skipped += 1;
                continue;
            }

            let target = path.with_file_name(&proposed);
            if target.exists() {
                summary.skipped += 1;
                continue;
            }
            match std::fs::rename(&path, &target) {
                Ok(()) => {
                    repo::update_rom_file_path(&conn, &disk_path, &target.to_string_lossy(), &proposed)
                        .map_err(|e| e.to_string())?;
                    summary.succeeded += 1;
                }
                Err(e) => summary.errors.push(format!("{}: {}", current_on_disk, e)),
            }
        }
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Deletes the files backing the given ROMs by sending them to the Recycle
/// Bin, so a mistake stays recoverable from Windows.
#[tauri::command]
pub async fn delete_roms(rom_ids: Vec<i64>, app: tauri::AppHandle) -> Result<MaintenanceSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let mut summary = MaintenanceSummary::default();
        let mut handled: Vec<String> = Vec::new();

        for rom_id in rom_ids {
            let Some((file_path, archive_member, file_name)) =
                repo::get_rom_file_info(&conn, rom_id).map_err(|e| e.to_string())?
            else {
                summary.skipped += 1;
                continue;
            };
            let disk_path = on_disk_path(&file_path, archive_member.as_deref());
            if handled.contains(&disk_path) {
                continue;
            }

            // Deleting a member of a multi-ROM archive would take the other
            // games with it, so that needs an explicit decision, not a guess.
            if archive_member.is_some() {
                let members = repo::count_roms_for_file(&conn, &disk_path).map_err(|e| e.to_string())?;
                if members > 1 {
                    summary.skipped += 1;
                    summary.errors.push(format!(
                        "{}: skipped, the archive holds {} ROMs",
                        file_name, members
                    ));
                    continue;
                }
            }

            match trash::delete(&disk_path) {
                Ok(()) => {
                    repo::delete_rom_rows_for_file(&conn, &disk_path).map_err(|e| e.to_string())?;
                    handled.push(disk_path);
                    summary.succeeded += 1;
                }
                Err(e) => summary.errors.push(format!("{}: {}", file_name, e)),
            }
        }
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}
