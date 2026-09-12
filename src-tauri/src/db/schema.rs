use rusqlite::Connection;

const SCHEMA_V1: &str = r#"
CREATE TABLE systems (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    folder_name TEXT NOT NULL UNIQUE,
    emulator_path TEXT,
    emulator_args TEXT
);

CREATE TABLE dat_sources (
    id INTEGER PRIMARY KEY,
    system_id INTEGER NOT NULL UNIQUE REFERENCES systems(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    dat_name TEXT,
    dat_version TEXT,
    imported_at TEXT NOT NULL
);

CREATE TABLE dat_games (
    id INTEGER PRIMARY KEY,
    dat_source_id INTEGER NOT NULL REFERENCES dat_sources(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    category TEXT,
    year TEXT,
    region TEXT
);

CREATE TABLE dat_roms (
    id INTEGER PRIMARY KEY,
    dat_game_id INTEGER NOT NULL REFERENCES dat_games(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    size INTEGER,
    crc32 TEXT,
    md5 TEXT,
    sha1 TEXT
);
CREATE INDEX idx_dat_roms_crc32 ON dat_roms(crc32);
CREATE INDEX idx_dat_roms_sha1 ON dat_roms(sha1);
CREATE INDEX idx_dat_roms_game ON dat_roms(dat_game_id);

CREATE TABLE roms (
    id INTEGER PRIMARY KEY,
    system_id INTEGER REFERENCES systems(id) ON DELETE SET NULL,
    file_path TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    size INTEGER,
    crc32 TEXT,
    md5 TEXT,
    sha1 TEXT,
    archive_member TEXT,
    dat_rom_id INTEGER REFERENCES dat_roms(id) ON DELETE SET NULL,
    match_status TEXT NOT NULL DEFAULT 'unmatched',
    last_scanned_at TEXT NOT NULL,
    seen_this_scan INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_roms_crc32 ON roms(crc32);
CREATE INDEX idx_roms_system ON roms(system_id);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute_batch("PRAGMA user_version = 1;")?;
    }
    Ok(())
}
