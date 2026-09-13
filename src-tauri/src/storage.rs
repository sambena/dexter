//! Where the library database and box art are kept. Both default to the app
//! data folder and can each be moved to another folder from Settings. The
//! choice lives in locations.json in the app data folder, since the database
//! can't record its own location. api.json and the window state always stay
//! in the app data folder: they describe this PC's running app, not the
//! library.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const DATABASE_FILE_NAME: &str = "library.db";
const LOCATIONS_FILE_NAME: &str = "locations.json";
const DEFAULT_ART_FOLDER_NAME: &str = "art";

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Locations {
    /// Folder holding library.db. None is the app data folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_folder: Option<PathBuf>,
    /// Folder new box art is saved to. None is "art" in the app data folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_folder: Option<PathBuf>,
}

impl Locations {
    pub fn library_folder(&self, app_folder: &Path) -> PathBuf {
        self.library_folder.clone().unwrap_or_else(|| app_folder.to_path_buf())
    }

    pub fn database_path(&self, app_folder: &Path) -> PathBuf {
        self.library_folder(app_folder).join(DATABASE_FILE_NAME)
    }

    pub fn art_folder(&self, app_folder: &Path) -> PathBuf {
        self.art_folder
            .clone()
            .unwrap_or_else(|| default_art_folder(app_folder))
    }
}

pub fn default_art_folder(app_folder: &Path) -> PathBuf {
    app_folder.join(DEFAULT_ART_FOLDER_NAME)
}

pub fn app_folder(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create app data dir: {}", e))?;
    Ok(dir)
}

/// A missing file means the defaults. An unreadable one is an error rather
/// than the defaults, which would quietly open a new, empty library.
pub fn load(app_folder: &Path) -> Result<Locations, String> {
    let path = app_folder.join(LOCATIONS_FILE_NAME);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{} is unreadable: {}", path.display(), e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Locations::default()),
        Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
    }
}

pub fn save(app_folder: &Path, locations: &Locations) -> Result<(), String> {
    let path = app_folder.join(LOCATIONS_FILE_NAME);
    if *locations == Locations::default() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let text = serde_json::to_string_pretty(locations).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("failed to write {}: {}", path.display(), e))
}

/// The folder new box art is saved to, created if needed.
pub fn art_folder(app: &AppHandle) -> Result<PathBuf, String> {
    let app_folder = app_folder(app)?;
    let dir = load(&app_folder)?.art_folder(&app_folder);
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    Ok(dir)
}

/// Stored as None when it's the default, so the default follows the app data
/// folder rather than being pinned to today's path.
pub fn chosen(folder: Option<&Path>, default: &Path) -> Option<PathBuf> {
    folder.filter(|f| !same_path(f, default)).map(Path::to_path_buf)
}

/// Windows paths compare without regard to case or trailing separators.
pub fn same_path(a: &Path, b: &Path) -> bool {
    fn normalized(p: &Path) -> String {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    normalized(a) == normalized(b)
}

pub fn open_database(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("failed to open database: {}", e))?;
    // The window and dexter-cli can both have the database open, so wait for
    // a lock briefly rather than failing immediately.
    conn.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Writes a copy of the open database into `to_folder` and opens it. The
/// caller swaps it in and then removes the old file. Refuses to replace a
/// library already in that folder.
pub fn copy_database(conn: &Connection, to_folder: &Path) -> Result<(Connection, PathBuf), String> {
    let to = to_folder.join(DATABASE_FILE_NAME);
    if to.exists() {
        return Err(format!(
            "{} already has a {}. Move or rename it first, so it isn't replaced.",
            to_folder.display(),
            DATABASE_FILE_NAME
        ));
    }
    std::fs::create_dir_all(to_folder).map_err(|e| format!("failed to create {}: {}", to_folder.display(), e))?;
    // VACUUM INTO writes a consistent copy of a database that's still open.
    conn.execute("VACUUM INTO ?1", params![to.to_string_lossy()])
        .map_err(|e| format!("failed to copy the library to {}: {}", to.display(), e))?;
    Ok((open_database(&to)?, to))
}

/// Removes a database file left behind by a move, with its rollback journal.
pub fn remove_database(path: &Path) -> Result<(), String> {
    let journal = PathBuf::from(format!("{}-journal", path.to_string_lossy()));
    let _ = std::fs::remove_file(journal);
    std::fs::remove_file(path).map_err(|e| format!("failed to remove {}: {}", path.display(), e))
}

#[derive(Debug, Default)]
pub struct ArtMove {
    pub moved: i64,
    /// Rows whose image file no longer exists.
    pub missing: i64,
    pub errors: Vec<String>,
}

/// Moves every stored box art image into `to_folder`, updating each row as
/// its file lands so the library never points at a file that isn't there.
/// Images left in `previous_folder` that nothing refers to are moved too, and
/// that folder is removed if it ends up empty.
pub fn move_art(
    conn: &std::sync::Mutex<Connection>,
    previous_folder: &Path,
    to_folder: &Path,
    mut progress: impl FnMut(usize, usize, &str),
) -> Result<ArtMove, String> {
    std::fs::create_dir_all(to_folder).map_err(|e| format!("failed to create {}: {}", to_folder.display(), e))?;
    let rows: Vec<(i64, String)> = {
        let conn = conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT id, file_path FROM box_art ORDER BY id").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    let mut result = ArtMove::default();
    let total = rows.len();
    for (i, (id, stored)) in rows.iter().enumerate() {
        let from = PathBuf::from(stored);
        let Some(name) = from.file_name() else { continue };
        progress(i + 1, total, &name.to_string_lossy());
        let dest = to_folder.join(name);
        if from.parent().is_some_and(|p| same_path(p, to_folder)) {
            continue;
        }
        if !from.is_file() {
            result.missing += 1;
            continue;
        }
        if let Err(e) = move_file(&from, &dest) {
            result.errors.push(format!("{}: {}", from.display(), e));
            continue;
        }
        let updated = conn
            .lock()
            .map_err(|e| e.to_string())
            .and_then(|c| {
                c.execute("UPDATE box_art SET file_path = ?1 WHERE id = ?2", params![dest.to_string_lossy(), id])
                    .map_err(|e| e.to_string())
            });
        match updated {
            Ok(_) => result.moved += 1,
            Err(e) => {
                // Put the file back where the row still points.
                let _ = move_file(&dest, &from);
                result.errors.push(format!("{}: {}", from.display(), e));
            }
        }
    }

    if !same_path(previous_folder, to_folder) {
        if let Ok(entries) = std::fs::read_dir(previous_folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_ours = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_art_file_name);
                if path.is_file() && is_ours && !to_folder.join(entry.file_name()).exists() {
                    if let Err(e) = move_file(&path, &to_folder.join(entry.file_name())) {
                        result.errors.push(format!("{}: {}", path.display(), e));
                    }
                }
            }
        }
        // Only succeeds if nothing else is in it.
        let _ = std::fs::remove_dir(previous_folder);
    }
    Ok(result)
}

/// Names Dexter gives box art: game_<id>.<ext> or rom_<id>.<ext>.
fn is_art_file_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    let id = stem.strip_prefix("game_").or_else(|| stem.strip_prefix("rom_"));
    id.is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())) && !ext.is_empty()
}

/// Renames when both paths are on one volume; otherwise copies and then
/// removes the original.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn temp_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dexter-storage-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn locations_default_when_missing_and_round_trip() {
        let app = temp_folder("locations");
        assert_eq!(load(&app).unwrap(), Locations::default());
        assert_eq!(Locations::default().database_path(&app), app.join("library.db"));
        assert_eq!(Locations::default().art_folder(&app), app.join("art"));

        let chosen = Locations { library_folder: Some(PathBuf::from(r"D:\Dexter")), art_folder: None };
        save(&app, &chosen).unwrap();
        assert_eq!(load(&app).unwrap(), chosen);
        assert_eq!(chosen.database_path(&app), PathBuf::from(r"D:\Dexter\library.db"));

        // Back to the defaults removes the file.
        save(&app, &Locations::default()).unwrap();
        assert!(!app.join(LOCATIONS_FILE_NAME).exists());

        std::fs::write(app.join(LOCATIONS_FILE_NAME), "not json").unwrap();
        assert!(load(&app).is_err());
        let _ = std::fs::remove_dir_all(&app);
    }

    #[test]
    fn default_folder_is_stored_as_none() {
        let default = Path::new(r"C:\Users\x\AppData\Roaming\dexter\art");
        assert_eq!(chosen(Some(Path::new(r"c:\users\X\AppData\Roaming\dexter\art\")), default), None);
        assert_eq!(chosen(None, default), None);
        assert_eq!(chosen(Some(Path::new(r"D:\Art")), default), Some(PathBuf::from(r"D:\Art")));
    }

    #[test]
    fn database_is_copied_but_never_over_another_library() {
        let from = temp_folder("db-from");
        let to = temp_folder("db-to");
        let conn = open_database(&from.join(DATABASE_FILE_NAME)).unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        crate::db::repo::set_setting(&conn, "rom_root_path", r"\\nas\Roms").unwrap();

        let (copy, path) = copy_database(&conn, &to).unwrap();
        assert_eq!(path, to.join(DATABASE_FILE_NAME));
        assert_eq!(
            crate::db::repo::get_setting(&copy, "rom_root_path").unwrap().as_deref(),
            Some(r"\\nas\Roms")
        );
        assert!(copy_database(&conn, &to).unwrap_err().contains("already has"));

        drop(conn);
        remove_database(&from.join(DATABASE_FILE_NAME)).unwrap();
        assert!(!from.join(DATABASE_FILE_NAME).exists());
        drop(copy);
        let _ = std::fs::remove_dir_all(&from);
        let _ = std::fs::remove_dir_all(&to);
    }

    #[test]
    fn art_files_move_with_their_rows() {
        let root = temp_folder("art");
        let old = root.join("art");
        let new = root.join("elsewhere");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("game_1.png"), b"one").unwrap();
        std::fs::write(old.join("rom_7.jpg"), b"seven").unwrap();
        std::fs::write(old.join("game_2.png"), b"orphan").unwrap();

        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::migrate(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        for (id, name) in [(1, "game_1.png"), (2, "rom_7.jpg"), (3, "game_9.png")] {
            conn.execute(
                "INSERT INTO box_art (id, dat_game_id, file_path, source) VALUES (?1, ?1, ?2, 'libretro')",
                params![id, old.join(name).to_string_lossy()],
            )
            .unwrap();
        }
        let conn = Mutex::new(conn);

        let mut seen = 0;
        let result = move_art(&conn, &old, &new, |_, _, _| seen += 1).unwrap();
        assert_eq!((result.moved, result.missing, result.errors.len()), (2, 1, 0));
        assert_eq!(seen, 3);
        assert_eq!(std::fs::read(new.join("rom_7.jpg")).unwrap(), b"seven");
        assert!(new.join("game_2.png").exists(), "unreferenced art moves too");
        assert!(!old.exists(), "the emptied folder is removed");

        let stored: String = conn
            .lock()
            .unwrap()
            .query_row("SELECT file_path FROM box_art WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(PathBuf::from(stored), new.join("game_1.png"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recognises_art_file_names() {
        assert!(is_art_file_name("game_12.png"));
        assert!(is_art_file_name("rom_3.jpeg"));
        assert!(!is_art_file_name("game_.png"));
        assert!(!is_art_file_name("notes.txt"));
        assert!(!is_art_file_name("game_12"));
    }
}
