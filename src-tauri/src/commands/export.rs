use crate::commands::emulators::{environment_for, platforms_of, snapshot};
use crate::db::repo;
use crate::emulators::playlist::{self, PlaylistRom};
use crate::models::{RetroArchExportSummary, ScanProgress};
use crate::state::AppState;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// Writes a RetroArch playlist for every system that launches with RetroArch,
/// and copies each game's box art to where RetroArch looks for it, so
/// RetroArch shows the library as Dexter names it. Systems that launch with
/// another emulator are left out.
#[tauri::command]
pub async fn export_retroarch_playlists(app: AppHandle) -> Result<RetroArchExportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || export_blocking(&app))
        .await
        .map_err(|e| e.to_string())?
}

fn export_blocking(app: &AppHandle) -> Result<RetroArchExportSummary, String> {
    let snapshot = snapshot(app)?;
    let (env, _) = environment_for(&snapshot);
    let retroarch = env
        .retroarch
        .as_ref()
        .ok_or("RetroArch wasn't found. Set where it's installed in Settings → Emulators.")?;

    let mut summary = RetroArchExportSummary {
        playlists_dir: retroarch.playlists_dir.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let state = app.state::<AppState>();
    let mut exports: Vec<(String, Option<(String, String)>, Vec<PlaylistRom>)> = Vec::new();
    for system in &snapshot.systems {
        let Some(core) = system.emulator_core.as_deref().filter(|c| !c.is_empty()) else {
            if system.emulator_path.as_deref().is_some_and(|p| !p.trim().is_empty()) {
                summary.skipped_systems.push(format!("{}: launches with another emulator", system.name));
            }
            continue;
        };
        let roms = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            repo::list_playlist_roms(&conn, system.id).map_err(|e| e.to_string())?
        };
        if roms.is_empty() {
            continue;
        }
        let name = platforms_of(&snapshot, system).into_iter().next().unwrap_or_else(|| system.name.clone());
        // An entry naming a core that isn't there fails to start, where
        // DETECT lets RetroArch offer the installed ones.
        let core_path = retroarch.core_path(core);
        let core = core_path
            .is_file()
            .then(|| (core_path.to_string_lossy().into_owned(), env.core_name(core)));
        exports.push((name, core, roms));
    }

    let total: usize = exports.iter().map(|(_, _, roms)| roms.len()).sum();
    let mut done = 0;
    std::fs::create_dir_all(&retroarch.playlists_dir)
        .map_err(|e| format!("couldn't create {}: {}", retroarch.playlists_dir.display(), e))?;
    for (name, core, roms) in &exports {
        let skip = playlist::entries_to_skip(roms, read_cue_sheet);
        let entries: Vec<&PlaylistRom> = roms.iter().filter(|r| !skip.contains(&r.rom_id)).collect();

        let path = retroarch.playlists_dir.join(playlist::file_name(name));
        if let Err(e) = write_playlist(&path, &playlist::render(name, &entries, core.as_ref().map(|(p, n)| (p.as_str(), n.as_str())))) {
            summary.errors.push(e);
            continue;
        }
        summary.playlists.push(format!("{}: {} games", name, entries.len()));
        summary.games += entries.len() as i64;

        let art_dir = retroarch.thumbnails_dir.join(name).join("Named_Boxarts");
        let mut written: HashSet<String> = HashSet::new();
        for rom in &entries {
            done += 1;
            let _ = app.emit(
                "export://progress",
                ScanProgress { current: done, total, current_file: rom.label.clone() },
            );
            let image = playlist::thumbnail_file_name(&rom.label);
            // Copies of one game share a label, and so a thumbnail.
            if !written.insert(image.clone()) {
                continue;
            }
            let Some(art) = rom.box_art.as_deref().filter(|a| is_png(a)) else {
                summary.without_art += 1;
                continue;
            };
            match copy_if_changed(Path::new(art), &art_dir.join(&image)) {
                Ok(true) => summary.thumbnails_copied += 1,
                Ok(false) => summary.thumbnails_unchanged += 1,
                Err(e) => summary.errors.push(format!("{}: {}", rom.label, e)),
            }
        }
        done += roms.len() - entries.len();
    }
    Ok(summary)
}

fn read_cue_sheet(path: &str) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    (size <= crate::scanner::cue::MAX_CUE_BYTES).then(|| std::fs::read_to_string(path).ok()).flatten()
}

/// RetroArch only loads PNG thumbnails unless told otherwise.
fn is_png(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// Replaces the playlist, keeping the first one it replaces as .lpl.bak in
/// case it was the user's own.
fn write_playlist(path: &Path, text: &str) -> Result<(), String> {
    let backup = PathBuf::from(format!("{}.bak", path.to_string_lossy()));
    if path.is_file() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| format!("couldn't back up {}: {}", path.display(), e))?;
    }
    std::fs::write(path, text).map_err(|e| format!("couldn't write {}: {}", path.display(), e))
}

/// Skips images already copied, so exporting again after a change is quick.
fn copy_if_changed(from: &Path, to: &Path) -> Result<bool, String> {
    let size = std::fs::metadata(from).map_err(|e| e.to_string())?.len();
    if std::fs::metadata(to).is_ok_and(|m| m.len() == size) {
        return Ok(false);
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::copy(from, to).map_err(|e| e.to_string())?;
    Ok(true)
}
