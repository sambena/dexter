use crate::db::repo;
use crate::models::{DuplicateGroupDto, MaintenanceSummary, RenamePlanEntryDto};
use crate::state::AppState;
use std::collections::{HashMap, HashSet};
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
pub(crate) fn on_disk_path(file_path: &str, archive_member: Option<&str>) -> String {
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

/// Saves an emulator keeps beside a ROM under the same name. They're renamed
/// with the ROM, since a save only loads next to a ROM of the same name.
const COMPANION_EXTENSIONS: &[&str] = &["sav", "srm", "eep", "sra", "fla", "mpk", "dsv", "mcr", "rtc"];

/// Files that load other files by name (cue sheets, playlists). Renaming a
/// file one of these lists would break it, so such files are left alone.
const REFERENCING_EXTENSIONS: &[&str] = &["cue", "gdi", "ccd", "m3u"];

/// Cue sheets and playlists are a few KB; anything bigger isn't one.
const MAX_REFERENCE_FILE_BYTES: u64 = 1024 * 1024;

fn is_companion_extension(ext: &str) -> bool {
    let ext = ext.to_ascii_lowercase();
    COMPANION_EXTENSIONS.contains(&ext.as_str())
        || ext.strip_prefix("state").is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
}

fn split_name(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    }
}

/// One folder's contents, read once however many ROMs in it are renamed.
struct FolderListing {
    names: Vec<String>,
    /// Lowercased, since Windows and SMB shares compare names case-insensitively.
    names_lower: HashSet<String>,
    /// Cue sheets and playlists: (file name, lowercased contents).
    references: Vec<(String, String)>,
}

fn list_folder(dir: &Path) -> FolderListing {
    let mut listing = FolderListing { names: Vec::new(), names_lower: HashSet::new(), references: Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return listing;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let is_reference = split_name(&name)
            .1
            .is_some_and(|ext| REFERENCING_EXTENSIONS.iter().any(|r| r.eq_ignore_ascii_case(ext)));
        let small = entry.metadata().is_ok_and(|m| m.is_file() && m.len() <= MAX_REFERENCE_FILE_BYTES);
        if is_reference && small {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                listing.references.push((name.clone(), String::from_utf8_lossy(&bytes).to_lowercase()));
            }
        }
        listing.names_lower.insert(name.to_lowercase());
        listing.names.push(name);
    }
    listing
}

struct PlannedRename {
    rom_id: i64,
    /// The file actually renamed: for an archive member, the archive.
    disk_path: String,
    current_name: String,
    new_name: String,
    /// Saves renamed alongside, as (current, new) file names.
    companions: Vec<(String, String)>,
    blocked_reason: Option<String>,
}

/// Works out every rename, and why any can't happen, without touching disk.
/// Preview and apply share it, so apply only does what the preview showed.
fn plan_renames(
    candidates: &[repo::RenameCandidate],
    members_in_archive: impl Fn(&str) -> Result<i64, String>,
) -> Result<Vec<PlannedRename>, String> {
    let mut folders: HashMap<PathBuf, FolderListing> = HashMap::new();
    // Lowercased paths already promised to an earlier entry, so two copies
    // of a game can't both be planned onto the same name.
    let mut claimed: HashSet<String> = HashSet::new();
    let mut plan = Vec::new();

    for c in candidates {
        let disk_path = on_disk_path(&c.file_path, c.archive_member.as_deref());
        let path = PathBuf::from(&disk_path);
        let Some(current_name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        let new_name = proposed_name(&c.dat_rom_name, &c.dat_game_name, c.archive_member.as_deref(), &current_name);
        if new_name.is_empty() || new_name == current_name {
            continue;
        }

        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let listing = folders.entry(dir.clone()).or_insert_with(|| list_folder(&dir));

        let (current_stem, _) = split_name(&current_name);
        let (new_stem, _) = split_name(&new_name);
        let companions: Vec<(String, String)> = listing
            .names
            .iter()
            .filter_map(|name| match split_name(name) {
                (stem, Some(ext)) if is_companion_extension(ext) && stem.eq_ignore_ascii_case(current_stem) => {
                    Some((name.clone(), format!("{}.{}", new_stem, ext)))
                }
                _ => None,
            })
            .collect();

        let mut blocked_reason = None;
        if c.archive_member.is_some() {
            let members = members_in_archive(&disk_path)?;
            if members > 1 {
                blocked_reason = Some(format!("Archive holds {} ROMs, so no single DAT name fits it", members));
            }
        }
        if blocked_reason.is_none() {
            let needle = current_name.to_lowercase();
            if let Some((reference, _)) = listing.references.iter().find(|(_, text)| text.contains(&needle)) {
                blocked_reason = Some(format!("{} loads this file by name; renaming it would break {}", reference, reference));
            }
        }
        if blocked_reason.is_none() {
            let renaming: Vec<&str> = std::iter::once(current_name.as_str())
                .chain(companions.iter().map(|(from, _)| from.as_str()))
                .collect();
            let targets: Vec<&str> =
                std::iter::once(new_name.as_str()).chain(companions.iter().map(|(_, to)| to.as_str())).collect();
            for target in &targets {
                let lower = target.to_lowercase();
                let is_own_name = renaming.iter().any(|r| r.eq_ignore_ascii_case(target));
                if listing.names_lower.contains(&lower) && !is_own_name {
                    blocked_reason = Some(format!("A file named {} already exists", target));
                    break;
                }
                if claimed.contains(&dir.join(&lower).to_string_lossy().to_string()) {
                    blocked_reason = Some(format!("Another copy is already being renamed to {}", target));
                    break;
                }
            }
            if blocked_reason.is_none() {
                for target in targets {
                    claimed.insert(dir.join(target.to_lowercase()).to_string_lossy().to_string());
                }
            }
        }

        plan.push(PlannedRename { rom_id: c.rom_id, disk_path, current_name, new_name, companions, blocked_reason });
    }
    Ok(plan)
}

/// Renames the selected, unblocked entries. A ROM and its saves move
/// together: if a save can't be renamed, everything for that ROM is put back.
fn apply_plan(conn: &rusqlite::Connection, plan: &[PlannedRename], rom_ids: &HashSet<i64>) -> Result<MaintenanceSummary, String> {
    let mut summary = MaintenanceSummary::default();
    for p in plan.iter().filter(|p| rom_ids.contains(&p.rom_id)) {
        if p.blocked_reason.is_some() {
            summary.skipped += 1;
            continue;
        }
        let path = PathBuf::from(&p.disk_path);
        let target = path.with_file_name(&p.new_name);
        if let Err(e) = std::fs::rename(&path, &target) {
            summary.errors.push(format!("{}: {}", p.current_name, e));
            continue;
        }

        let mut moved: Vec<&(String, String)> = Vec::new();
        let mut failure = None;
        for companion in &p.companions {
            match std::fs::rename(path.with_file_name(&companion.0), path.with_file_name(&companion.1)) {
                Ok(()) => moved.push(companion),
                Err(e) => {
                    failure = Some(format!("{}: {}", companion.0, e));
                    break;
                }
            }
        }
        if let Some(failure) = failure {
            for (from, to) in moved.into_iter().rev() {
                let _ = std::fs::rename(path.with_file_name(to), path.with_file_name(from));
            }
            let _ = std::fs::rename(&target, &path);
            summary.errors.push(format!("{}: left unchanged, its save couldn't be renamed ({})", p.current_name, failure));
            continue;
        }

        repo::update_rom_file_path(conn, &p.disk_path, &target.to_string_lossy(), &p.new_name)
            .map_err(|e| e.to_string())?;
        summary.succeeded += 1;
    }
    Ok(summary)
}

#[tauri::command]
pub async fn preview_renames(app: tauri::AppHandle) -> Result<Vec<RenamePlanEntryDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let candidates = repo::list_rename_candidates(&conn).map_err(|e| e.to_string())?;
        let plan = plan_renames(&candidates, |p| repo::count_roms_for_file(&conn, p).map_err(|e| e.to_string()))?;
        Ok(plan
            .into_iter()
            .map(|p| RenamePlanEntryDto {
                rom_id: p.rom_id,
                current_name: p.current_name,
                new_name: p.new_name,
                target_path: p.disk_path,
                also_renames: p.companions.into_iter().map(|(from, _)| from).collect(),
                blocked_reason: p.blocked_reason,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn apply_renames(rom_ids: Vec<i64>, app: tauri::AppHandle) -> Result<MaintenanceSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        // The plan is recomputed rather than trusting names sent from the UI,
        // so a stale preview can't rename a file to something unintended.
        let candidates = repo::list_rename_candidates(&conn).map_err(|e| e.to_string())?;
        let plan = plan_renames(&candidates, |p| repo::count_roms_for_file(&conn, p).map_err(|e| e.to_string()))?;
        apply_plan(&conn, &plan, &rom_ids.into_iter().collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use repo::RenameCandidate;

    /// A scratch folder that's removed when the test ends.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("dexter-rename-{}-{}", name, std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn file(&self, name: &str, contents: &str) -> String {
            let p = self.0.join(name);
            std::fs::write(&p, contents).unwrap();
            p.to_string_lossy().to_string()
        }
        fn has(&self, name: &str) -> bool {
            self.0.join(name).is_file()
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn loose(rom_id: i64, path: &str, dat_rom_name: &str) -> RenameCandidate {
        RenameCandidate {
            rom_id,
            file_path: path.to_string(),
            archive_member: None,
            dat_rom_name: dat_rom_name.to_string(),
            dat_game_name: Path::new(dat_rom_name).file_stem().unwrap().to_string_lossy().to_string(),
        }
    }

    fn plan(candidates: &[RenameCandidate]) -> Vec<PlannedRename> {
        plan_renames(candidates, |_| Ok(1)).unwrap()
    }

    #[test]
    fn saves_are_planned_with_their_rom() {
        let dir = TempDir::new("saves");
        let rom = dir.file("Pokemon - Red Version.gb", "rom");
        dir.file("Pokemon - Red Version.sav", "save");
        dir.file("pokemon - red version.state1", "state");
        dir.file("Pokemon - Blue Version.sav", "other game's save");

        let p = plan(&[loose(1, &rom, "Pokemon - Red Version (USA, Europe) (SGB Enhanced).gb")]);
        assert_eq!(p[0].blocked_reason, None);
        let mut companions = p[0].companions.clone();
        companions.sort();
        assert_eq!(
            companions,
            vec![
                ("Pokemon - Red Version.sav".to_string(), "Pokemon - Red Version (USA, Europe) (SGB Enhanced).sav".to_string()),
                ("pokemon - red version.state1".to_string(), "Pokemon - Red Version (USA, Europe) (SGB Enhanced).state1".to_string()),
            ]
        );
    }

    #[test]
    fn file_named_in_a_cue_sheet_is_blocked() {
        let dir = TempDir::new("cue");
        let bin = dir.file("Final Fantasy V (USA) (v1.1).bin", "data");
        dir.file("Final Fantasy V.cue", "FILE \"Final Fantasy V (USA) (v1.1).bin\" BINARY\n  TRACK 01 MODE2/2352");

        let p = plan(&[loose(1, &bin, "Final Fantasy Anthology - Final Fantasy V (USA) (Rev 1).bin")]);
        let reason = p[0].blocked_reason.as_deref().unwrap();
        assert!(reason.starts_with("Final Fantasy V.cue loads this file"), "{}", reason);
    }

    #[test]
    fn second_copy_of_a_game_is_blocked_not_silently_skipped() {
        let dir = TempDir::new("dupes");
        let a = dir.file("Mario Golf (U) [C][!].gbc", "a");
        let b = dir.file("Mario Golf.gbc", "b");

        let p = plan(&[loose(1, &a, "Mario Golf (USA).gbc"), loose(2, &b, "Mario Golf (USA).gbc")]);
        assert_eq!(p[0].blocked_reason, None);
        assert_eq!(p[1].blocked_reason.as_deref(), Some("Another copy is already being renamed to Mario Golf (USA).gbc"));
    }

    #[test]
    fn existing_file_with_the_new_name_blocks_including_for_saves() {
        let dir = TempDir::new("exists");
        let rom = dir.file("Game.gb", "rom");
        dir.file("Game.sav", "save");
        dir.file("Game (USA).sav", "someone else's save");

        let p = plan(&[loose(1, &rom, "Game (USA).gb")]);
        assert_eq!(p[0].blocked_reason.as_deref(), Some("A file named Game (USA).sav already exists"));
    }

    #[test]
    fn case_only_rename_is_not_blocked_by_itself() {
        let dir = TempDir::new("case");
        let rom = dir.file("tetris (world).gb", "rom");
        let p = plan(&[loose(1, &rom, "Tetris (World).gb")]);
        assert_eq!(p[0].blocked_reason, None);
    }

    #[test]
    fn multi_rom_archive_is_blocked() {
        let dir = TempDir::new("archive");
        let zip = dir.file("Compilation.zip", "zip");
        let mut c = loose(1, &format!("{}::a.nes", zip), "A (USA).nes");
        c.archive_member = Some("a.nes".to_string());
        let p = plan_renames(&[c], |_| Ok(2)).unwrap();
        assert_eq!(p[0].blocked_reason.as_deref(), Some("Archive holds 2 ROMs, so no single DAT name fits it"));
    }

    fn db_with_rom(path: &str) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO roms (id, file_path, file_name, match_status, last_scanned_at) VALUES (1, ?1, 'x', 'matched', '')",
            [path],
        )
        .unwrap();
        conn
    }

    #[test]
    fn apply_moves_rom_and_save_and_updates_the_library() {
        let dir = TempDir::new("apply");
        let rom = dir.file("Pokemon - Red Version.gb", "rom");
        dir.file("Pokemon - Red Version.sav", "save");
        let conn = db_with_rom(&rom);

        let p = plan(&[loose(1, &rom, "Pokemon - Red Version (USA, Europe).gb")]);
        let summary = apply_plan(&conn, &p, &HashSet::from([1])).unwrap();

        assert_eq!((summary.succeeded, summary.errors.len()), (1, 0));
        assert!(dir.has("Pokemon - Red Version (USA, Europe).gb"));
        assert!(dir.has("Pokemon - Red Version (USA, Europe).sav"));
        assert!(!dir.has("Pokemon - Red Version.gb") && !dir.has("Pokemon - Red Version.sav"));
        let stored: String = conn.query_row("SELECT file_path FROM roms WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert!(stored.ends_with("Pokemon - Red Version (USA, Europe).gb"));
    }

    #[test]
    fn apply_puts_everything_back_when_a_save_cannot_move() {
        let dir = TempDir::new("rollback");
        let rom = dir.file("Game.gb", "rom");
        dir.file("Game.sav", "save");
        let conn = db_with_rom(&rom);

        let mut p = plan(&[loose(1, &rom, "Game (USA).gb")]);
        // Simulate the save vanishing between preview and apply.
        std::fs::remove_file(dir.0.join("Game.sav")).unwrap();
        p[0].companions = vec![("Game.sav".to_string(), "Game (USA).sav".to_string())];
        let summary = apply_plan(&conn, &p, &HashSet::from([1])).unwrap();

        assert_eq!(summary.succeeded, 0);
        assert!(summary.errors[0].starts_with("Game.gb: left unchanged"), "{}", summary.errors[0]);
        assert!(dir.has("Game.gb") && !dir.has("Game (USA).gb"));
        let stored: String = conn.query_row("SELECT file_path FROM roms WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(stored, rom);
    }

    #[test]
    fn blocked_and_unselected_entries_are_not_touched() {
        let dir = TempDir::new("select");
        let a = dir.file("A.gb", "a");
        let b = dir.file("B.gb", "b");
        let conn = db_with_rom(&a);

        let p = plan(&[loose(1, &a, "Alpha (USA).gb"), loose(2, &b, "Beta (USA).gb")]);
        let summary = apply_plan(&conn, &p, &HashSet::from([1])).unwrap();
        assert_eq!(summary.succeeded, 1);
        assert!(dir.has("B.gb"));
    }
}
