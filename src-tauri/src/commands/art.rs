use crate::art::thumbnails;
use crate::db::repo;
use crate::models::{ArtFetchSummary, ScanProgress};
use crate::state::AppState;
use base64::Engine;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

fn art_dir(app: &AppHandle) -> Result<PathBuf, String> {
    crate::storage::art_folder(app)
}

fn to_data_url(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

#[tauri::command]
pub fn has_known_box_art_source(folder_name: String) -> bool {
    thumbnails::has_known_thumbnail_source(&folder_name)
}

#[tauri::command]
pub fn get_box_art(rom_id: i64, state: State<AppState>) -> Result<Option<String>, String> {
    let path = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::get_box_art_path(&conn, rom_id).map_err(|e| e.to_string())?
    };
    match path {
        Some(p) => to_data_url(Path::new(&p)).map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn fetch_box_art(rom_id: i64, app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_box_art_blocking(rom_id, &app))
        .await
        .map_err(|e| e.to_string())?
}

fn fetch_box_art_blocking(rom_id: i64, app: &AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let ctx = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::get_rom_art_context(&conn, rom_id).map_err(|e| e.to_string())?
    };
    let ctx = ctx.ok_or_else(|| "ROM not found".to_string())?;
    let (dat_game_id, game_name, folder_name) = match (ctx.dat_game_id, ctx.dat_game_name, ctx.system_folder_name) {
        (Some(id), Some(name), Some(folder)) => (id, name, folder),
        _ => {
            return Err(
                "This ROM isn't matched to a DAT entry, so there's no known title to look up box art for. Use \"Add Box Art\" instead.".to_string(),
            )
        }
    };

    let client = thumbnails::http_client().map_err(|e| e.to_string())?;
    let mut index = thumbnails::ThumbnailIndex::default();
    let bytes =
        thumbnails::fetch_box_art(&client, &mut index, &folder_name, &game_name).map_err(|e| e.to_string())?;

    let dir = art_dir(app)?;
    let file_path = dir.join(format!("game_{}.png", dat_game_id));
    std::fs::write(&file_path, &bytes).map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::set_game_box_art(&conn, dat_game_id, &file_path.to_string_lossy(), "libretro")
            .map_err(|e| e.to_string())?;
    }

    to_data_url(&file_path)
}

/// Downloads box art for every matched game in the library that doesn't have any
/// yet. Shares the same cancel flag as scan_library/hash_pending_roms, so the
/// existing Stop button works here too.
#[tauri::command]
pub async fn fetch_all_box_art(app: AppHandle) -> Result<ArtFetchSummary, String> {
    // reqwest's blocking client panics if it's used or dropped on an async
    // runtime thread, so the whole download loop runs on a blocking thread.
    tauri::async_runtime::spawn_blocking(move || fetch_all_box_art_blocking(&app))
        .await
        .map_err(|e| e.to_string())?
}

fn fetch_all_box_art_blocking(app: &AppHandle) -> Result<ArtFetchSummary, String> {
    let state = app.state::<AppState>();
    let targets = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::list_matched_games_missing_art(&conn).map_err(|e| e.to_string())?
    };

    state.cancel_flag.store(false, Ordering::SeqCst);
    let total = targets.len();
    let mut summary = ArtFetchSummary::default();
    let dir = art_dir(app)?;
    let client = thumbnails::http_client().map_err(|e| e.to_string())?;
    let mut index = thumbnails::ThumbnailIndex::default();

    for (i, target) in targets.iter().enumerate() {
        if state.cancel_flag.load(Ordering::SeqCst) {
            break;
        }
        let _ = app.emit(
            "art://progress",
            ScanProgress { current: i + 1, total, current_file: target.game_name.clone() },
        );

        match thumbnails::fetch_box_art(&client, &mut index, &target.folder_name, &target.game_name) {
            Ok(bytes) => {
                let file_path = dir.join(format!("game_{}.png", target.dat_game_id));
                std::fs::write(&file_path, &bytes).map_err(|e| e.to_string())?;
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                repo::set_game_box_art(&conn, target.dat_game_id, &file_path.to_string_lossy(), "libretro")
                    .map_err(|e| e.to_string())?;
                summary.downloaded += 1;
            }
            Err(e) => {
                summary.not_found += 1;
                summary.errors.push(format!("{}: {}", target.game_name, e));
            }
        }
        summary.attempted += 1;
    }

    let _ = app.emit("art://done", summary.clone());
    Ok(summary)
}

#[tauri::command]
pub async fn pick_and_set_box_art(rom_id: i64, app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
        .pick_file(move |f| {
            let _ = tx.send(f);
        });
    let picked = rx.recv().map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Err("No file selected".to_string());
    };
    set_box_art_from_file(&app, &state, rom_id, Path::new(&picked.to_string()))
}

/// Copies an image into the art cache as this ROM's manual box art and
/// returns it as a data URL.
pub fn set_box_art_from_file(app: &AppHandle, state: &AppState, rom_id: i64, src_path: &Path) -> Result<String, String> {
    if !src_path.is_file() {
        return Err(format!("No image found at {}", src_path.display()));
    }
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_string();

    let dir = art_dir(app)?;
    let dest = dir.join(format!("rom_{}.{}", rom_id, ext));
    std::fs::copy(src_path, &dest).map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::set_rom_box_art(&conn, rom_id, &dest.to_string_lossy(), "manual").map_err(|e| e.to_string())?;
    }

    to_data_url(&dest)
}
