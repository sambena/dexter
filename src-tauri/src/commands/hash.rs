use crate::db::repo::{self, PendingRom};
use crate::models::{ScanProgress, ScanSummary};
use crate::scanner::{archive, hashing};
use crate::scanner::hashing::FileHashes;
use crate::state::AppState;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use tauri::{Emitter, State};

fn compute_hash(rom: &PendingRom) -> Result<FileHashes, String> {
    match &rom.archive_member {
        Some(member) => {
            let zip_path_len = rom.file_path.len().saturating_sub(member.len() + 2);
            let zip_path = &rom.file_path[..zip_path_len];
            archive::hash_zip_member(Path::new(zip_path), member).map_err(|e| e.to_string())
        }
        None => hashing::hash_file(Path::new(&rom.file_path)).map_err(|e| e.to_string()),
    }
}

fn partition_round_robin(items: Vec<PendingRom>, worker_count: usize) -> Vec<Vec<PendingRom>> {
    let mut chunks: Vec<Vec<PendingRom>> = (0..worker_count).map(|_| Vec::new()).collect();
    for (i, item) in items.into_iter().enumerate() {
        chunks[i % worker_count].push(item);
    }
    chunks
}

struct HashResult {
    rom_id: i64,
    system_id: Option<i64>,
    file_path: String,
    outcome: Result<FileHashes, String>,
}

/// Hashes and DAT-matches every ROM discovered by a previous `scan_library` quick
/// scan that hasn't been hashed yet (match_status = 'pending'). Hashing (the slow,
/// I/O-bound part) is spread across a small worker pool; DB matching/writes stay on
/// this command's own thread since rusqlite::Connection isn't shared across threads.
#[tauri::command]
pub async fn hash_pending_roms(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let pending = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        repo::list_pending_roms(&conn).map_err(|e| e.to_string())?
    };

    let total = pending.len();
    let mut summary = ScanSummary::default();
    if total == 0 {
        let _ = app.emit("hash://done", summary.clone());
        return Ok(summary);
    }

    state.cancel_flag.store(false, Ordering::SeqCst);

    let worker_count = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let chunks = partition_round_robin(pending, worker_count);

    let (tx, rx) = mpsc::channel::<HashResult>();
    let handles: Vec<_> = chunks
        .into_iter()
        .filter(|c| !c.is_empty())
        .map(|chunk| {
            let tx = tx.clone();
            let cancel_flag: Arc<AtomicBool> = state.cancel_flag.clone();
            thread::spawn(move || {
                for rom in chunk {
                    if cancel_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let outcome = compute_hash(&rom);
                    let _ = tx.send(HashResult {
                        rom_id: rom.id,
                        system_id: rom.system_id,
                        file_path: rom.file_path,
                        outcome,
                    });
                }
            })
        })
        .collect();
    drop(tx);

    let mut processed = 0usize;
    for received in rx.iter() {
        processed += 1;
        let _ = app.emit(
            "hash://progress",
            ScanProgress { current: processed, total, current_file: received.file_path.clone() },
        );

        let conn = state.db.lock().map_err(|e| e.to_string())?;
        match received.outcome {
            Ok(h) => {
                let dat_rom_id = match received.system_id {
                    Some(system_id) => {
                        let headerless = h.headerless.as_ref().map(|hl| &hl.digests);
                        repo::match_rom_hashes(&conn, system_id, &h.full, headerless).map_err(|e| e.to_string())?
                    }
                    None => None,
                };
                if dat_rom_id.is_some() {
                    summary.matched += 1;
                } else {
                    summary.unmatched += 1;
                }
                repo::update_rom_hash(&conn, received.rom_id, &h, dat_rom_id).map_err(|e| e.to_string())?;
            }
            Err(e) => {
                repo::mark_rom_hash_error(&conn, received.rom_id).map_err(|e| e.to_string())?;
                summary.errors.push(format!("{}: {}", received.file_path, e));
            }
        }
        summary.scanned_files += 1;
    }

    for handle in handles {
        let _ = handle.join();
    }

    let _ = app.emit("hash://done", summary.clone());
    Ok(summary)
}
