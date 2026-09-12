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

const SCHEMA_V2: &str = r#"
-- Box art is keyed to whichever is more specific: a single ROM (manual override,
-- or a fetch for an unmatched file) or a matched DAT game (shared by every file
-- that resolves to that game, e.g. multiple regional dumps).
CREATE TABLE box_art (
    id INTEGER PRIMARY KEY,
    rom_id INTEGER UNIQUE REFERENCES roms(id) ON DELETE CASCADE,
    dat_game_id INTEGER UNIQUE REFERENCES dat_games(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    source TEXT NOT NULL
);
"#;

const SCHEMA_V3: &str = r#"
-- A user-supplied direct DAT download URL, for systems with no built-in known
-- source (i.e. anything not covered by Redump's direct datfile URLs).
ALTER TABLE systems ADD COLUMN dat_url TEXT;
"#;

const SCHEMA_V4: &str = r#"
-- Some systems (e.g. Nintendo 64, split into BigEndian/ByteSwapped/LittleEndian
-- DATs by No-Intro) need more than one DAT to get good match coverage, so a
-- system can no longer own just one dat_source. SQLite can't drop a UNIQUE
-- constraint directly, so the table is recreated without it.
PRAGMA foreign_keys = OFF;

CREATE TABLE dat_sources_new (
    id INTEGER PRIMARY KEY,
    system_id INTEGER NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    dat_name TEXT,
    dat_version TEXT,
    imported_at TEXT NOT NULL
);
INSERT INTO dat_sources_new (id, system_id, file_name, dat_name, dat_version, imported_at)
    SELECT id, system_id, file_name, dat_name, dat_version, imported_at FROM dat_sources;
DROP TABLE dat_sources;
ALTER TABLE dat_sources_new RENAME TO dat_sources;

PRAGMA foreign_keys = ON;
"#;

const SCHEMA_V5: &str = r#"
-- Hashes of the ROM data without its header (iNES/fwNES 16 bytes, SNES
-- copier 512 bytes), for DATs that exclude headers. NULL when the file has
-- no header.
ALTER TABLE roms ADD COLUMN header_size INTEGER;
ALTER TABLE roms ADD COLUMN headerless_crc32 TEXT;
ALTER TABLE roms ADD COLUMN headerless_md5 TEXT;
ALTER TABLE roms ADD COLUMN headerless_sha1 TEXT;

-- Unmatched ROMs of the affected systems were hashed before headers were
-- detected, so queue them for Hash & Match again. Matched ones are left
-- alone: they already found their DAT entry.
UPDATE roms SET match_status = 'pending'
 WHERE match_status IN ('unmatched', 'error')
   AND (LOWER(file_name) LIKE '%.nes' OR LOWER(file_name) LIKE '%.fds'
     OR LOWER(file_name) LIKE '%.smc' OR LOWER(file_name) LIKE '%.sfc'
     OR LOWER(file_name) LIKE '%.swc' OR LOWER(file_name) LIKE '%.fig');
"#;

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute_batch("PRAGMA user_version = 1;")?;
    }
    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
        conn.execute_batch("PRAGMA user_version = 2;")?;
    }
    if version < 3 {
        conn.execute_batch(SCHEMA_V3)?;
        conn.execute_batch("PRAGMA user_version = 3;")?;
    }
    if version < 4 {
        conn.execute_batch(SCHEMA_V4)?;
        conn.execute_batch("PRAGMA user_version = 4;")?;
    }
    if version < 5 {
        conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 5; COMMIT;", SCHEMA_V5))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v5_requeues_only_unmatched_header_prone_roms() {
        let conn = Connection::open_in_memory().unwrap();
        for sql in [SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch(
            "PRAGMA user_version = 4;
             INSERT INTO systems (id, name, folder_name) VALUES (1, 'NES', 'NES');
             INSERT INTO roms (file_path, file_name, match_status, last_scanned_at, system_id) VALUES
               ('a.zip::1942.nes', '1942.nes', 'unmatched', '', 1),
               ('b.smc', 'Aerobiz.SMC', 'error', '', 1),
               ('c.nes', 'Contra.nes', 'matched', '', 1),
               ('d.gba', 'Metroid.gba', 'unmatched', '', 1);",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let status = |name: &str| -> String {
            conn.query_row("SELECT match_status FROM roms WHERE file_name = ?1", [name], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(status("1942.nes"), "pending");
        assert_eq!(status("Aerobiz.SMC"), "pending");
        assert_eq!(status("Contra.nes"), "matched");
        assert_eq!(status("Metroid.gba"), "unmatched");
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 5);
    }

    #[test]
    fn fresh_database_migrates_to_latest() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        conn.execute_batch("SELECT header_size, headerless_crc32 FROM roms").unwrap();
    }
}
