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

const SCHEMA_V6: &str = r#"
-- Bytes also left off the end for the headerless hashes (NES title tags
-- appended past the last whole 8 KiB block).
ALTER TABLE roms ADD COLUMN trailer_size INTEGER;

-- NES dumps hashed under v5 with leftover bytes at the end were hashed
-- without trimming them, so queue the unmatched ones again.
UPDATE roms SET match_status = 'pending'
 WHERE match_status IN ('unmatched', 'error')
   AND header_size = 16 AND size > 16 AND (size - 16) % 8192 <> 0
   AND LOWER(file_name) NOT LIKE '%.fds';
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
    if version < 6 {
        conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 6; COMMIT;", SCHEMA_V6))?;
    }
    if version < 7 {
        migrate_v7_unverifiable(conn)?;
    }
    if version < 8 {
        conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 8; COMMIT;", SCHEMA_V8))?;
    }
    if version < 9 {
        migrate_v9_repairs(conn)?;
    }
    if version < 10 {
        conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 10; COMMIT;", SCHEMA_V10))?;
    }
    Ok(())
}

const SCHEMA_V10: &str = r#"
-- What an extracted title folder (Wii U) says about itself, read from its
-- code/app.xml and meta/meta.xml on each scan (scanner::wiiu). NULL for
-- everything else.
ALTER TABLE roms ADD COLUMN title_id TEXT;
ALTER TABLE roms ADD COLUMN title_version INTEGER;
ALTER TABLE roms ADD COLUMN title_kind TEXT;   -- 'game', 'update', 'dlc' or 'demo'
ALTER TABLE roms ADD COLUMN title_name TEXT;
ALTER TABLE roms ADD COLUMN product_code TEXT;

-- The DAT game a file was identified as by its title rather than its hash,
-- for dumps that can't be verified. Unlike dat_rom_id this doesn't make the
-- file "matched"; it supplies the name, region and box art.
ALTER TABLE roms ADD COLUMN identified_game_id INTEGER REFERENCES dat_games(id) ON DELETE SET NULL;
"#;

const SCHEMA_V9: &str = r#"
-- How a match was found when the file isn't byte-for-byte the DAT's dump
-- ("overdump", "header", "mirrored", "cue-tracks"). NULL for exact matches.
ALTER TABLE roms ADD COLUMN match_note TEXT;

-- Hash & Match can now find the dump inside overdumped or oddly headered
-- cartridge files, and match cue sheets by their tracks, so unmatched ones
-- get another try. 64 MiB is the largest cartridge (scanner::repair).
UPDATE roms SET match_status = 'pending'
 WHERE match_status IN ('unmatched', 'error')
   AND (size <= 67108864 OR LOWER(file_name) LIKE '%.cue' OR LOWER(file_name) LIKE '%.gdi');
"#;

/// v9: repairs and cue sheets (see SCHEMA_V9), plus two format changes:
/// .ecm files are now decoded and hashed, and Switch files can't be verified.
fn migrate_v9_repairs(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN;")?;
    let result = (|| {
        conn.execute_batch(SCHEMA_V9)?;
        let rows: Vec<(i64, String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT id, file_name, match_status FROM roms
                 WHERE match_status IN ('unmatched', 'pending', 'error', 'unverifiable') AND size IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, file_name, status) in rows {
            let unverifiable = crate::scanner::formats::is_unverifiable(&file_name);
            if unverifiable && status != "unverifiable" {
                conn.execute("UPDATE roms SET match_status = 'unverifiable', dat_rom_id = NULL WHERE id = ?1", [id])?;
            } else if !unverifiable && status == "unverifiable" && crate::scanner::ecm::decoded_name(&file_name).is_some() {
                conn.execute("UPDATE roms SET match_status = 'pending' WHERE id = ?1", [id])?;
            }
        }
        conn.execute_batch("PRAGMA user_version = 9;")
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT;"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
}

const SCHEMA_V8: &str = r#"
-- A RetroArch core for the system, e.g. "mgba". When set, the system launches
-- through the one RetroArch install in settings (key retroarch_path) instead
-- of its own emulator_path/emulator_args, so RetroArch is configured once.
ALTER TABLE systems ADD COLUMN emulator_core TEXT;
"#;

/// Moves ROMs that hashing can never verify out of "unmatched"/"pending" into
/// the new "unverifiable" status. Done in Rust rather than SQL so the list of
/// formats lives in one place (scanner::formats), shared with the scanner.
fn migrate_v7_unverifiable(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN;")?;
    let result = (|| {
        let candidates: Vec<(i64, String, bool)> = {
            let mut stmt = conn.prepare(
                "SELECT id, file_name,
                        -- Folder dumps were the only unmatched rows never hashed.
                        match_status = 'unmatched' AND crc32 IS NULL AND size IS NULL AND archive_member IS NULL
                 FROM roms WHERE match_status IN ('unmatched', 'pending', 'error')",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, file_name, is_folder_dump) in candidates {
            if is_folder_dump || crate::scanner::formats::is_unverifiable(&file_name) {
                conn.execute(
                    "UPDATE roms SET match_status = 'unverifiable', dat_rom_id = NULL WHERE id = ?1",
                    [id],
                )?;
            }
        }
        conn.execute_batch("PRAGMA user_version = 7;")
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT;"),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
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
             INSERT INTO roms (file_path, file_name, crc32, match_status, last_scanned_at, system_id) VALUES
               ('a.zip::1942.nes', '1942.nes', '42c89db5', 'unmatched', '', 1),
               ('b.smc', 'Aerobiz.SMC', NULL, 'error', '', 1),
               ('c.nes', 'Contra.nes', '11111111', 'matched', '', 1),
               ('d.gba', 'Metroid.gba', '22222222', 'unmatched', '', 1);",
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
        assert_eq!(version, 10);
    }

    #[test]
    fn v9_requeues_repairable_roms_and_reclassifies_formats() {
        let conn = Connection::open_in_memory().unwrap();
        for sql in [SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V8] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch(
            "PRAGMA user_version = 8;
             INSERT INTO roms (file_path, file_name, size, crc32, match_status, last_scanned_at) VALUES
               ('a', 'Asteroids Hyper 64 (U) [!].z64', 8388608, 'aa', 'unmatched', ''),
               ('b', 'Pokemon X.3ds', 2147483648, 'bb', 'unmatched', ''),
               ('c', 'FF5.cue', 120, 'cc', 'unmatched', ''),
               ('d', 'FF7 (Disc 1).bin.ecm', 400000000, 'dd', 'unverifiable', ''),
               ('e', 'TOTK 1.2.0.nsp', 336947536, 'ee', 'unmatched', ''),
               ('f', 'F-Zero GX (USA).rvz', 1000000, 'ff', 'unverifiable', ''),
               ('g', 'Tetris (World).gb', 65536, '11', 'matched', '');
             INSERT INTO roms (file_path, file_name, match_status, last_scanned_at) VALUES
               ('h', 'MARIO KART 8 [AMKE01]', 'unverifiable', '');",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let status = |name: &str| -> String {
            conn.query_row("SELECT match_status FROM roms WHERE file_name = ?1", [name], |r| r.get(0)).unwrap()
        };
        assert_eq!(status("Asteroids Hyper 64 (U) [!].z64"), "pending");
        assert_eq!(status("Pokemon X.3ds"), "unmatched");
        assert_eq!(status("FF5.cue"), "pending");
        assert_eq!(status("FF7 (Disc 1).bin.ecm"), "pending");
        assert_eq!(status("TOTK 1.2.0.nsp"), "unverifiable");
        assert_eq!(status("F-Zero GX (USA).rvz"), "unverifiable");
        assert_eq!(status("Tetris (World).gb"), "matched");
        assert_eq!(status("MARIO KART 8 [AMKE01]"), "unverifiable");
    }

    #[test]
    fn v7_marks_folder_dumps_and_unhashable_formats_unverifiable() {
        let conn = Connection::open_in_memory().unwrap();
        for sql in [SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch(
            "PRAGMA user_version = 6;
             INSERT INTO roms (file_path, file_name, size, crc32, archive_member, match_status, last_scanned_at) VALUES
               ('a', 'F-Zero GX (USA).rvz', 100, 'abcd1234', NULL, 'unmatched', ''),
               ('b', 'MARIO KART 8 [AMKE01]', NULL, NULL, NULL, 'unmatched', ''),
               ('c', 'FF7 (Disc 1).bin.ecm', 100, NULL, NULL, 'pending', ''),
               ('d', 'Super Mario 64.z64', 100, 'abcd1234', NULL, 'unmatched', ''),
               ('e', 'Pending Game.sfc', 100, NULL, NULL, 'pending', ''),
               ('f', 'Weird.rvz', 100, 'abcd1234', NULL, 'matched', '');",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let status = |name: &str| -> String {
            conn.query_row("SELECT match_status FROM roms WHERE file_name = ?1", [name], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(status("F-Zero GX (USA).rvz"), "unverifiable");
        assert_eq!(status("MARIO KART 8 [AMKE01]"), "unverifiable");
        // v9 made .ecm hashable again, and requeues small unmatched files.
        assert_eq!(status("FF7 (Disc 1).bin.ecm"), "pending");
        assert_eq!(status("Super Mario 64.z64"), "pending");
        assert_eq!(status("Pending Game.sfc"), "pending");
        assert_eq!(status("Weird.rvz"), "matched");
    }

    #[test]
    fn v6_requeues_unmatched_nes_with_leftover_bytes() {
        let conn = Connection::open_in_memory().unwrap();
        for sql in [SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute_batch(
            "PRAGMA user_version = 5;
             INSERT INTO roms (file_path, file_name, size, header_size, match_status, last_scanned_at) VALUES
               ('a', 'tagged.nes', 41104, 16, 'unmatched', ''),
               ('b', 'clean.nes', 40976, 16, 'unmatched', ''),
               ('c', 'tagged-but-matched.nes', 41104, 16, 'matched', ''),
               ('d', 'disk.fds', 131016, 16, 'unmatched', ''),
               ('e', 'copier.smc', 1049088, 512, 'unmatched', '');",
        )
        .unwrap();

        // Just v6: v9 requeues every small unmatched file anyway.
        conn.execute_batch(SCHEMA_V6).unwrap();

        let status = |name: &str| -> String {
            conn.query_row("SELECT match_status FROM roms WHERE file_name = ?1", [name], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(status("tagged.nes"), "pending");
        assert_eq!(status("clean.nes"), "unmatched");
        assert_eq!(status("tagged-but-matched.nes"), "matched");
        assert_eq!(status("disk.fds"), "unmatched");
        assert_eq!(status("copier.smc"), "unmatched");
    }

    #[test]
    fn fresh_database_migrates_to_latest() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        conn.execute_batch("SELECT header_size, headerless_crc32 FROM roms").unwrap();
    }
}
