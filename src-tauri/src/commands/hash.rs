use crate::db::repo::{self, PendingRom};
use crate::models::{ScanProgress, ScanSummary};
use crate::scanner::{archive, hashing, repair};
use crate::scanner::hashing::FileHashes;
use crate::state::AppState;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use tauri::{Emitter, State};

enum HashFailure {
    /// A format variant Dexter can't decode (e.g. a Wii RVZ image): it can't
    /// be verified, which isn't an error worth retrying.
    Unsupported,
    Failed(String),
}

fn compute_hash(rom: &PendingRom) -> Result<FileHashes, HashFailure> {
    match &rom.archive_member {
        Some(member) => {
            let zip_path_len = rom.file_path.len().saturating_sub(member.len() + 2);
            let zip_path = &rom.file_path[..zip_path_len];
            archive::hash_zip_member(Path::new(zip_path), member).map_err(|e| HashFailure::Failed(e.to_string()))
        }
        None => hashing::hash_file(Path::new(&rom.file_path)).map_err(|e| match e.kind() {
            std::io::ErrorKind::Unsupported => HashFailure::Unsupported,
            _ => HashFailure::Failed(e.to_string()),
        }),
    }
}

/// The whole ROM in memory, for trying repairs on a file that didn't match.
fn read_rom_bytes(file_path: &str, archive_member: Option<&str>) -> Result<Vec<u8>, String> {
    match archive_member {
        Some(member) => {
            let zip_path = &file_path[..file_path.len().saturating_sub(member.len() + 2)];
            archive::read_zip_member(Path::new(zip_path), member, repair::MAX_REPAIR_BYTES).map_err(|e| e.to_string())
        }
        None => std::fs::read(file_path).map_err(|e| e.to_string()),
    }
}

/// Looks for the DAT dump inside an unmatched cartridge file (see
/// scanner::repair). DAT sizes are looked up once per system.
fn find_repaired_match(
    conn: &std::sync::Mutex<rusqlite::Connection>,
    dat_sizes: &mut HashMap<i64, Vec<u64>>,
    system_id: i64,
    file_path: &str,
    archive_member: Option<&str>,
) -> Result<Option<(repair::Repaired, i64)>, String> {
    let sizes = match dat_sizes.get(&system_id) {
        Some(sizes) => sizes,
        None => {
            let conn = conn.lock().map_err(|e| e.to_string())?;
            let sizes = repo::dat_rom_sizes(&conn, system_id).map_err(|e| e.to_string())?;
            dat_sizes.entry(system_id).or_insert(sizes)
        }
    };
    if sizes.is_empty() {
        return Ok(None);
    }
    // A file that can't be read again is simply left unmatched.
    let Ok(bytes) = read_rom_bytes(file_path, archive_member) else {
        return Ok(None);
    };
    let candidates = repair::candidates(&bytes, sizes);
    let conn = conn.lock().map_err(|e| e.to_string())?;
    for candidate in candidates {
        if let Some(dat_rom_id) = repo::find_dat_rom_exact(&conn, system_id, &candidate.digests).map_err(|e| e.to_string())? {
            return Ok(Some((candidate, dat_rom_id)));
        }
    }
    Ok(None)
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
    archive_member: Option<String>,
    outcome: Result<FileHashes, HashFailure>,
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
        // Names from file names still refresh, e.g. after a DAT was added.
        {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            repo::name_unmatched_files(&conn, None).map_err(|e| e.to_string())?;
        }
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
                        archive_member: rom.archive_member,
                        outcome,
                    });
                }
            })
        })
        .collect();
    drop(tx);

    let mut processed = 0usize;
    let mut dat_sizes: HashMap<i64, Vec<u64>> = HashMap::new();
    for received in rx.iter() {
        processed += 1;
        let _ = app.emit(
            "hash://progress",
            ScanProgress { current: processed, total, current_file: received.file_path.clone() },
        );

        match received.outcome {
            Ok(h) => {
                let dat_rom_id = match received.system_id {
                    Some(system_id) => {
                        let conn = state.db.lock().map_err(|e| e.to_string())?;
                        let headerless = h.headerless.as_ref().map(|hl| &hl.digests);
                        repo::match_rom_hashes(&conn, system_id, &h.full, headerless).map_err(|e| e.to_string())?
                    }
                    None => None,
                };
                // Re-reading happens without holding the database lock.
                let repaired = match (dat_rom_id, received.system_id) {
                    (None, Some(system_id)) if h.size <= repair::MAX_REPAIR_BYTES => find_repaired_match(
                        &state.db,
                        &mut dat_sizes,
                        system_id,
                        &received.file_path,
                        received.archive_member.as_deref(),
                    )?,
                    _ => None,
                };

                let conn = state.db.lock().map_err(|e| e.to_string())?;
                repo::update_rom_hash(&conn, received.rom_id, &h, dat_rom_id).map_err(|e| e.to_string())?;
                if let Some((repaired, repaired_dat_rom_id)) = &repaired {
                    repo::set_repaired_match(&conn, received.rom_id, repaired, *repaired_dat_rom_id)
                        .map_err(|e| e.to_string())?;
                }
                if dat_rom_id.is_some() || repaired.is_some() {
                    summary.matched += 1;
                } else {
                    summary.unmatched += 1;
                }
            }
            Err(HashFailure::Unsupported) => {
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                repo::mark_rom_unverifiable(&conn, received.rom_id).map_err(|e| e.to_string())?;
                summary.unverifiable += 1;
            }
            Err(HashFailure::Failed(e)) => {
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                repo::mark_rom_hash_error(&conn, received.rom_id).map_err(|e| e.to_string())?;
                summary.errors.push(format!("{}: {}", received.file_path, e));
            }
        }
        summary.scanned_files += 1;
    }

    for handle in handles {
        let _ = handle.join();
    }

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let sheets = repo::match_track_lists(&conn, None).map_err(|e| e.to_string())?;
        summary.matched += sheets;
        summary.unmatched = (summary.unmatched - sheets).max(0);
        repo::name_unmatched_files(&conn, None).map_err(|e| e.to_string())?;
    }

    let _ = app.emit("hash://done", summary.clone());
    Ok(summary)
}
