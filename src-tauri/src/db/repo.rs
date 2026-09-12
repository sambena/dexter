use crate::models::{
    DuplicateFileDto, DuplicateGroupDto, RomDetailsDto, RomFilter, RomListItemDto, Settings, SystemDto,
};
use crate::scanner::hashing::{Digests, FileHashes};
use rusqlite::{params, Connection, OptionalExtension};

// ---------- settings ----------

pub fn get_settings(conn: &Connection) -> rusqlite::Result<Settings> {
    let rom_root_path = get_setting(conn, "rom_root_path")?;
    Ok(Settings { rom_root_path })
}

pub fn save_settings(conn: &Connection, settings: &Settings) -> rusqlite::Result<()> {
    if let Some(path) = &settings.rom_root_path {
        set_setting(conn, "rom_root_path", path)?;
    }
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------- systems ----------

pub fn list_systems(conn: &Connection) -> rusqlite::Result<Vec<SystemDto>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.name, s.folder_name, s.emulator_path, s.emulator_args, s.dat_url,
                EXISTS(SELECT 1 FROM dat_sources d WHERE d.system_id = s.id)
         FROM systems s
         ORDER BY s.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SystemDto {
            id: r.get(0)?,
            name: r.get(1)?,
            folder_name: r.get(2)?,
            emulator_path: r.get(3)?,
            emulator_args: r.get(4)?,
            dat_url: r.get(5)?,
            has_dat: r.get(6)?,
        })
    })?;
    rows.collect()
}

pub fn get_or_create_system_by_folder(conn: &Connection, folder_name: &str) -> rusqlite::Result<i64> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM systems WHERE folder_name = ?1",
            params![folder_name],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO systems (name, folder_name) VALUES (?1, ?2)",
        params![folder_name, folder_name],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn set_system_emulator(
    conn: &Connection,
    system_id: i64,
    emulator_path: Option<&str>,
    emulator_args: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE systems SET emulator_path = ?1, emulator_args = ?2 WHERE id = ?3",
        params![emulator_path, emulator_args, system_id],
    )?;
    Ok(())
}

pub struct SystemDatSource {
    pub folder_name: String,
    pub dat_url: Option<String>,
}

pub fn get_system_dat_source(conn: &Connection, system_id: i64) -> rusqlite::Result<Option<SystemDatSource>> {
    conn.query_row(
        "SELECT folder_name, dat_url FROM systems WHERE id = ?1",
        params![system_id],
        |r| {
            Ok(SystemDatSource {
                folder_name: r.get(0)?,
                dat_url: r.get(1)?,
            })
        },
    )
    .optional()
}

pub fn set_system_dat_url(conn: &Connection, system_id: i64, dat_url: Option<&str>) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE systems SET dat_url = ?1 WHERE id = ?2",
        params![dat_url, system_id],
    )?;
    Ok(())
}

// ---------- dat import ----------

pub struct DatGameImport {
    pub name: String,
    pub category: Option<String>,
    pub year: Option<String>,
    pub region: Option<String>,
    pub roms: Vec<DatRomImport>,
}

pub struct DatRomImport {
    pub name: String,
    pub size: Option<i64>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
}

/// Adds a DAT as another source for a system rather than replacing whatever's
/// already there — some systems (e.g. N64's BigEndian/ByteSwapped/LittleEndian
/// split) need multiple DATs imported to get good match coverage. Use
/// remove_dat_source to take one back out.
///
/// Re-importing the *same* DAT still replaces its previous copy: identity is
/// the DAT's internal name when it has one (so a re-download with a bumped
/// version supersedes the old one), else the file name.
pub fn add_dat_source(
    conn: &mut Connection,
    system_id: i64,
    file_name: &str,
    dat_name: Option<&str>,
    dat_version: Option<&str>,
    games: &[DatGameImport],
) -> rusqlite::Result<(i64, i64)> {
    let tx = conn.transaction()?;
    let now = chrono::Utc::now().to_rfc3339();
    match dat_name {
        Some(name) => tx.execute(
            "DELETE FROM dat_sources WHERE system_id = ?1 AND dat_name = ?2",
            params![system_id, name],
        )?,
        None => tx.execute(
            "DELETE FROM dat_sources WHERE system_id = ?1 AND dat_name IS NULL AND file_name = ?2",
            params![system_id, file_name],
        )?,
    };
    tx.execute(
        "INSERT INTO dat_sources (system_id, file_name, dat_name, dat_version, imported_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![system_id, file_name, dat_name, dat_version, now],
    )?;
    let dat_source_id = tx.last_insert_rowid();

    let mut games_imported = 0i64;
    let mut roms_imported = 0i64;
    for game in games {
        tx.execute(
            "INSERT INTO dat_games (dat_source_id, name, category, year, region) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![dat_source_id, game.name, game.category, game.year, game.region],
        )?;
        let dat_game_id = tx.last_insert_rowid();
        games_imported += 1;
        for rom in &game.roms {
            tx.execute(
                "INSERT INTO dat_roms (dat_game_id, name, size, crc32, md5, sha1) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    dat_game_id,
                    rom.name,
                    rom.size,
                    rom.crc32.as_ref().map(|s| s.to_lowercase()),
                    rom.md5.as_ref().map(|s| s.to_lowercase()),
                    rom.sha1.as_ref().map(|s| s.to_lowercase()),
                ],
            )?;
            roms_imported += 1;
        }
    }
    tx.commit()?;
    Ok((games_imported, roms_imported))
}

pub fn list_dat_sources(conn: &Connection) -> rusqlite::Result<Vec<crate::models::DatSourceDto>> {
    let mut stmt = conn.prepare(
        "SELECT id, system_id, file_name, dat_name, dat_version, imported_at
         FROM dat_sources
         ORDER BY system_id, imported_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::models::DatSourceDto {
            id: r.get(0)?,
            system_id: r.get(1)?,
            file_name: r.get(2)?,
            dat_name: r.get(3)?,
            dat_version: r.get(4)?,
            imported_at: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// Removing a dat_source cascades to its dat_games/dat_roms and NULLs
/// dat_rom_id on any roms that pointed into it — but their match_status
/// wouldn't otherwise reflect that, so it's fixed up here too.
pub fn remove_dat_source(conn: &mut Connection, dat_source_id: i64) -> rusqlite::Result<()> {
    let system_id: Option<i64> = conn
        .query_row(
            "SELECT system_id FROM dat_sources WHERE id = ?1",
            params![dat_source_id],
            |r| r.get(0),
        )
        .optional()?;
    conn.execute("DELETE FROM dat_sources WHERE id = ?1", params![dat_source_id])?;
    if let Some(system_id) = system_id {
        rematch_system(conn, system_id)?;
    }
    Ok(())
}

/// Re-runs matching for a system using hashes already stored in the DB, with
/// no file I/O. Importing or removing a DAT changes what a ROM *would* match,
/// but Hash & Match only revisits 'pending'/'error' rows — so without this,
/// adding a DAT would appear to do nothing to an already-scanned library.
/// Returns (newly_matched, now_unmatched).
pub fn rematch_system(conn: &mut Connection, system_id: i64) -> rusqlite::Result<(i64, i64)> {
    struct Hashed {
        id: i64,
        full: Digests,
        headerless: Option<Digests>,
        was_matched: bool,
    }

    let hashed: Vec<Hashed> = {
        let mut stmt = conn.prepare(
            "SELECT id, crc32, sha1, md5, dat_rom_id, headerless_crc32, headerless_sha1, headerless_md5 FROM roms
             WHERE system_id = ?1 AND crc32 IS NOT NULL
               AND match_status IN ('matched', 'unmatched')",
        )?;
        let rows = stmt.query_map(params![system_id], |r| {
            Ok(Hashed {
                id: r.get(0)?,
                full: Digests {
                    crc32: r.get(1)?,
                    sha1: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    md5: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                },
                was_matched: r.get::<_, Option<i64>>(4)?.is_some(),
                headerless: match r.get::<_, Option<String>>(5)? {
                    Some(crc32) => Some(Digests {
                        crc32,
                        sha1: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                        md5: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    }),
                    None => None,
                },
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut newly_matched = 0i64;
    let mut now_unmatched = 0i64;
    let tx = conn.transaction()?;
    for rom in &hashed {
        let dat_rom_id = match_rom_hashes(&tx, system_id, &rom.full, rom.headerless.as_ref())?;
        let is_matched = dat_rom_id.is_some();
        if is_matched && !rom.was_matched {
            newly_matched += 1;
        } else if !is_matched && rom.was_matched {
            now_unmatched += 1;
        }
        tx.execute(
            "UPDATE roms SET dat_rom_id = ?1, match_status = ?2 WHERE id = ?3",
            params![
                dat_rom_id,
                if is_matched { "matched" } else { "unmatched" },
                rom.id
            ],
        )?;
    }
    tx.commit()?;
    Ok((newly_matched, now_unmatched))
}

// ---------- scanning ----------

pub fn mark_system_unseen(conn: &Connection, system_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE roms SET seen_this_scan = 0 WHERE system_id = ?1",
        params![system_id],
    )?;
    Ok(())
}

pub struct DatRomMatch {
    pub dat_rom_id: i64,
    pub sha1: Option<String>,
    pub md5: Option<String>,
}

/// Finds candidate dat_roms for a system by CRC32, disambiguating on sha1/md5 when multiple share the CRC32.
pub fn find_dat_rom_match(
    conn: &Connection,
    system_id: i64,
    crc32: &str,
    sha1: &str,
    md5: &str,
) -> rusqlite::Result<Option<i64>> {
    let mut stmt = conn.prepare(
        "SELECT dr.id, dr.sha1, dr.md5
         FROM dat_roms dr
         JOIN dat_games dg ON dg.id = dr.dat_game_id
         JOIN dat_sources ds ON ds.id = dg.dat_source_id
         WHERE ds.system_id = ?1 AND dr.crc32 = ?2",
    )?;
    let candidates: Vec<DatRomMatch> = stmt
        .query_map(params![system_id, crc32], |r| {
            Ok(DatRomMatch {
                dat_rom_id: r.get(0)?,
                sha1: r.get(1)?,
                md5: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    if candidates.len() <= 1 {
        return Ok(candidates.into_iter().next().map(|c| c.dat_rom_id));
    }
    // Disambiguate CRC32 collisions with sha1, then md5.
    if let Some(m) = candidates.iter().find(|c| c.sha1.as_deref() == Some(sha1)) {
        return Ok(Some(m.dat_rom_id));
    }
    if let Some(m) = candidates.iter().find(|c| c.md5.as_deref() == Some(md5)) {
        return Ok(Some(m.dat_rom_id));
    }
    Ok(candidates.into_iter().next().map(|c| c.dat_rom_id))
}

/// Matches a ROM against the system's DATs using the whole file first, then
/// the data without its header. DATs hash one form or the other (No-Intro's
/// SNES and headerless NES DATs exclude headers; its headered NES DAT
/// includes a clean one), so trying both works whichever is imported.
pub fn match_rom_hashes(
    conn: &Connection,
    system_id: i64,
    full: &Digests,
    headerless: Option<&Digests>,
) -> rusqlite::Result<Option<i64>> {
    if let Some(id) = find_dat_rom_match(conn, system_id, &full.crc32, &full.sha1, &full.md5)? {
        return Ok(Some(id));
    }
    match headerless {
        Some(h) => find_dat_rom_match(conn, system_id, &h.crc32, &h.sha1, &h.md5),
        None => Ok(None),
    }
}

/// Quick-scan insert for a file (or archive entry) that hasn't been hashed yet.
/// Deliberately leaves crc32/md5/sha1/dat_rom_id/match_status untouched on conflict
/// so re-running a quick scan never throws away hash/match results from a previous
/// hashing pass over the same file.
pub fn upsert_pending_rom(
    conn: &Connection,
    system_id: i64,
    file_path: &str,
    file_name: &str,
    size: Option<i64>,
    archive_member: Option<&str>,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO roms (system_id, file_path, file_name, size, archive_member, match_status, last_scanned_at, seen_this_scan)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, 1)
         ON CONFLICT(file_path) DO UPDATE SET
            system_id = excluded.system_id,
            file_name = excluded.file_name,
            size = COALESCE(roms.size, excluded.size),
            archive_member = excluded.archive_member,
            last_scanned_at = ?6,
            seen_this_scan = 1",
        params![system_id, file_path, file_name, size, archive_member, now],
    )?;
    Ok(())
}

/// Quick-scan insert for something hashing can't verify: a whole-folder dump
/// (e.g. an extracted Wii U title) or a format like .rvz (see
/// scanner::formats). These skip Hash & Match and go straight to
/// "unverifiable"; a row recorded under an older rule is converted too.
pub fn upsert_unverifiable_rom(
    conn: &Connection,
    system_id: i64,
    file_path: &str,
    file_name: &str,
    size: Option<i64>,
    archive_member: Option<&str>,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO roms (system_id, file_path, file_name, size, archive_member, match_status, last_scanned_at, seen_this_scan)
         VALUES (?1, ?2, ?3, ?4, ?5, 'unverifiable', ?6, 1)
         ON CONFLICT(file_path) DO UPDATE SET
            system_id = excluded.system_id,
            file_name = excluded.file_name,
            size = COALESCE(roms.size, excluded.size),
            archive_member = excluded.archive_member,
            match_status = 'unverifiable',
            dat_rom_id = NULL,
            last_scanned_at = ?6,
            seen_this_scan = 1",
        params![system_id, file_path, file_name, size, archive_member, now],
    )?;
    Ok(())
}

pub fn prune_system_unseen(conn: &Connection, system_id: i64) -> rusqlite::Result<i64> {
    let removed = conn.execute(
        "DELETE FROM roms WHERE system_id = ?1 AND seen_this_scan = 0",
        params![system_id],
    )?;
    Ok(removed as i64)
}

pub struct PendingRom {
    pub id: i64,
    pub system_id: Option<i64>,
    pub file_path: String,
    pub archive_member: Option<String>,
}

/// Rows still awaiting a hash, including ones from a previous hashing pass that
/// failed (e.g. a transient network read error) — those are automatically retried.
pub fn list_pending_roms(conn: &Connection) -> rusqlite::Result<Vec<PendingRom>> {
    let mut stmt = conn.prepare(
        "SELECT id, system_id, file_path, archive_member FROM roms WHERE match_status IN ('pending', 'error')",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PendingRom {
            id: r.get(0)?,
            system_id: r.get(1)?,
            file_path: r.get(2)?,
            archive_member: r.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn update_rom_hash(
    conn: &Connection,
    rom_id: i64,
    hashes: &FileHashes,
    dat_rom_id: Option<i64>,
) -> rusqlite::Result<()> {
    let match_status = if dat_rom_id.is_some() { "matched" } else { "unmatched" };
    let now = chrono::Utc::now().to_rfc3339();
    let headerless = hashes.headerless.as_ref();
    conn.execute(
        "UPDATE roms SET size = ?1, crc32 = ?2, md5 = ?3, sha1 = ?4, dat_rom_id = ?5, match_status = ?6, last_scanned_at = ?7,
                header_size = ?8, headerless_crc32 = ?9, headerless_md5 = ?10, headerless_sha1 = ?11,
                trailer_size = ?13
         WHERE id = ?12",
        params![
            hashes.size as i64,
            hashes.full.crc32,
            hashes.full.md5,
            hashes.full.sha1,
            dat_rom_id,
            match_status,
            now,
            headerless.map(|h| h.header_size as i64),
            headerless.map(|h| &h.digests.crc32),
            headerless.map(|h| &h.digests.md5),
            headerless.map(|h| &h.digests.sha1),
            rom_id,
            headerless.map(|h| h.trailer_size as i64),
        ],
    )?;
    Ok(())
}

pub fn mark_rom_hash_error(conn: &Connection, rom_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE roms SET match_status = 'error' WHERE id = ?1",
        params![rom_id],
    )?;
    Ok(())
}

// ---------- browsing ----------

pub fn list_roms(conn: &Connection, filter: &RomFilter) -> rusqlite::Result<Vec<RomListItemDto>> {
    let mut sql = String::from(
        "SELECT r.id, r.file_name, r.system_id, s.name, dg.name, r.match_status
         FROM roms r
         LEFT JOIN systems s ON s.id = r.system_id
         LEFT JOIN dat_roms dr ON dr.id = r.dat_rom_id
         LEFT JOIN dat_games dg ON dg.id = dr.dat_game_id
         WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(text) = filter.search_text.as_ref().filter(|t| !t.is_empty()) {
        sql.push_str(" AND (r.file_name LIKE ?1 OR dg.name LIKE ?1)");
        args.push(Box::new(format!("%{}%", text)));
    }
    if let Some(system_id) = filter.system_id {
        sql.push_str(&format!(" AND r.system_id = ?{}", args.len() + 1));
        args.push(Box::new(system_id));
    }
    if let Some(status) = filter.match_status.as_ref().filter(|s| !s.is_empty() && *s != "all") {
        sql.push_str(&format!(" AND r.match_status = ?{}", args.len() + 1));
        args.push(Box::new(status.clone()));
    }

    let sort_col = match filter.sort_by.as_deref() {
        Some("system") => "s.name",
        _ => "COALESCE(dg.name, r.file_name)",
    };
    let sort_dir = match filter.sort_dir.as_deref() {
        Some("desc") => "DESC",
        _ => "ASC",
    };
    sql.push_str(&format!(" ORDER BY {} {}", sort_col, sort_dir));

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |r| {
        let file_name: String = r.get(1)?;
        let dat_name: Option<String> = r.get(4)?;
        let display_name = dat_name
            .unwrap_or_else(|| crate::dat::filename::parse_filename_metadata(&file_name).title);
        Ok(RomListItemDto {
            id: r.get(0)?,
            file_name,
            system_id: r.get(2)?,
            system_name: r.get(3)?,
            display_name,
            match_status: r.get(5)?,
        })
    })?;
    rows.collect()
}

pub fn get_rom_details(conn: &Connection, rom_id: i64) -> rusqlite::Result<Option<RomDetailsDto>> {
    conn.query_row(
        "SELECT r.id, r.file_name, r.file_path, r.archive_member, r.system_id, s.name,
                dg.year, dg.region, dg.name, r.match_status,
                r.crc32, r.md5, r.sha1, s.emulator_path, s.emulator_args,
                r.header_size, r.headerless_crc32, r.trailer_size
         FROM roms r
         LEFT JOIN systems s ON s.id = r.system_id
         LEFT JOIN dat_roms dr ON dr.id = r.dat_rom_id
         LEFT JOIN dat_games dg ON dg.id = dr.dat_game_id
         WHERE r.id = ?1",
        params![rom_id],
        |r| {
            let file_name: String = r.get(1)?;
            let dat_year: Option<String> = r.get(6)?;
            let dat_region: Option<String> = r.get(7)?;
            let dat_name: Option<String> = r.get(8)?;

            let (display_name, year, region, metadata_guessed) = match dat_name {
                Some(name) => (name, dat_year, dat_region, false),
                None => {
                    let guessed = crate::dat::filename::parse_filename_metadata(&file_name);
                    (guessed.title, guessed.year, guessed.region, true)
                }
            };

            Ok(RomDetailsDto {
                id: r.get(0)?,
                file_name,
                file_path: r.get(2)?,
                archive_member: r.get(3)?,
                system_id: r.get(4)?,
                system_name: r.get(5)?,
                year,
                region,
                display_name,
                metadata_guessed,
                match_status: r.get(9)?,
                crc32: r.get(10)?,
                md5: r.get(11)?,
                sha1: r.get(12)?,
                emulator_path: r.get(13)?,
                emulator_args: r.get(14)?,
                header_size: r.get(15)?,
                headerless_crc32: r.get(16)?,
                trailer_size: r.get(17)?,
            })
        },
    )
    .optional()
}

// ---------- box art ----------

pub struct RomArtContext {
    pub dat_game_id: Option<i64>,
    pub dat_game_name: Option<String>,
    pub system_folder_name: Option<String>,
}

pub fn get_rom_art_context(conn: &Connection, rom_id: i64) -> rusqlite::Result<Option<RomArtContext>> {
    conn.query_row(
        "SELECT dg.id, dg.name, s.folder_name
         FROM roms r
         LEFT JOIN dat_roms dr ON dr.id = r.dat_rom_id
         LEFT JOIN dat_games dg ON dg.id = dr.dat_game_id
         LEFT JOIN systems s ON s.id = r.system_id
         WHERE r.id = ?1",
        params![rom_id],
        |row| {
            Ok(RomArtContext {
                dat_game_id: row.get(0)?,
                dat_game_name: row.get(1)?,
                system_folder_name: row.get(2)?,
            })
        },
    )
    .optional()
}

/// Rom-level box art (manual override) takes priority; otherwise falls back to
/// whatever's stored for the matched DAT game, if any.
pub fn get_box_art_path(conn: &Connection, rom_id: i64) -> rusqlite::Result<Option<String>> {
    if let Some(path) = conn
        .query_row(
            "SELECT file_path FROM box_art WHERE rom_id = ?1",
            params![rom_id],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(Some(path));
    }
    conn.query_row(
        "SELECT ba.file_path
         FROM box_art ba
         JOIN dat_roms dr ON dr.dat_game_id = ba.dat_game_id
         JOIN roms r ON r.dat_rom_id = dr.id
         WHERE r.id = ?1",
        params![rom_id],
        |r| r.get(0),
    )
    .optional()
}

pub fn set_rom_box_art(conn: &Connection, rom_id: i64, file_path: &str, source: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO box_art (rom_id, file_path, source) VALUES (?1, ?2, ?3)
         ON CONFLICT(rom_id) DO UPDATE SET file_path = excluded.file_path, source = excluded.source",
        params![rom_id, file_path, source],
    )?;
    Ok(())
}

pub fn set_game_box_art(conn: &Connection, dat_game_id: i64, file_path: &str, source: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO box_art (dat_game_id, file_path, source) VALUES (?1, ?2, ?3)
         ON CONFLICT(dat_game_id) DO UPDATE SET file_path = excluded.file_path, source = excluded.source",
        params![dat_game_id, file_path, source],
    )?;
    Ok(())
}

pub struct GameArtTarget {
    pub dat_game_id: i64,
    pub game_name: String,
    pub folder_name: String,
}

/// Every matched DAT game (that at least one scanned ROM actually resolves to)
/// which doesn't have box art cached yet — the working set for a bulk art download.
pub fn list_matched_games_missing_art(conn: &Connection) -> rusqlite::Result<Vec<GameArtTarget>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT dg.id, dg.name, s.folder_name
         FROM dat_games dg
         JOIN dat_sources ds ON ds.id = dg.dat_source_id
         JOIN systems s ON s.id = ds.system_id
         WHERE EXISTS (
             SELECT 1 FROM dat_roms dr JOIN roms r ON r.dat_rom_id = dr.id WHERE dr.dat_game_id = dg.id
         )
         AND NOT EXISTS (SELECT 1 FROM box_art ba WHERE ba.dat_game_id = dg.id)",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(GameArtTarget {
            dat_game_id: r.get(0)?,
            game_name: r.get(1)?,
            folder_name: r.get(2)?,
        })
    })?;
    rows.collect()
}

// ---------- maintenance ----------

/// Groups of files that are byte-identical (same SHA1). Only exact copies are
/// reported: any one of them is safely disposable, which isn't true of files
/// that merely match the same DAT game.
pub fn list_duplicate_groups(conn: &Connection) -> rusqlite::Result<Vec<DuplicateGroupDto>> {
    let mut stmt = conn.prepare(
        "SELECT r.sha1, r.id, r.file_name, r.file_path, r.archive_member, r.size,
                s.name, dg.name
         FROM roms r
         LEFT JOIN systems s ON s.id = r.system_id
         LEFT JOIN dat_roms dr ON dr.id = r.dat_rom_id
         LEFT JOIN dat_games dg ON dg.id = dr.dat_game_id
         WHERE r.sha1 IS NOT NULL AND r.sha1 <> ''
           AND r.sha1 IN (SELECT sha1 FROM roms
                          WHERE sha1 IS NOT NULL AND sha1 <> ''
                          GROUP BY sha1 HAVING COUNT(*) > 1)
         ORDER BY r.sha1, r.file_path",
    )?;

    let rows = stmt.query_map([], |r| {
        let sha1: String = r.get(0)?;
        let file_name: String = r.get(2)?;
        let dat_name: Option<String> = r.get(7)?;
        let display_name = dat_name
            .unwrap_or_else(|| crate::dat::filename::parse_filename_metadata(&file_name).title);
        Ok((
            sha1,
            DuplicateFileDto {
                id: r.get(1)?,
                display_name,
                file_name,
                file_path: r.get(3)?,
                archive_member: r.get(4)?,
                size: r.get(5)?,
                system_name: r.get(6)?,
            },
        ))
    })?;

    let mut groups: Vec<DuplicateGroupDto> = Vec::new();
    for row in rows {
        let (sha1, file) = row?;
        match groups.last_mut() {
            Some(g) if g.sha1 == sha1 => g.files.push(file),
            _ => groups.push(DuplicateGroupDto { sha1, files: vec![file] }),
        }
    }
    Ok(groups)
}

pub struct RenameCandidate {
    pub rom_id: i64,
    pub file_path: String,
    pub file_name: String,
    pub archive_member: Option<String>,
    pub dat_rom_name: String,
    pub dat_game_name: String,
}

/// Matched ROMs together with the DAT's canonical names, for building a
/// rename plan. Unmatched ROMs have no authoritative name so are excluded.
pub fn list_rename_candidates(conn: &Connection) -> rusqlite::Result<Vec<RenameCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.file_path, r.file_name, r.archive_member, dr.name, dg.name
         FROM roms r
         JOIN dat_roms dr ON dr.id = r.dat_rom_id
         JOIN dat_games dg ON dg.id = dr.dat_game_id
         WHERE r.match_status = 'matched'
         ORDER BY r.file_path",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(RenameCandidate {
            rom_id: r.get(0)?,
            file_path: r.get(1)?,
            file_name: r.get(2)?,
            archive_member: r.get(3)?,
            dat_rom_name: r.get(4)?,
            dat_game_name: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// ROM rows key archive members as "<archive path>::<member>", so file_path is
/// a composite key rather than something that exists on disk. Anything that
/// touches the filesystem has to collapse to the archive path first, and these
/// helpers match every row backed by that one file.
pub fn count_roms_for_file(conn: &Connection, file_path: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM roms
         WHERE file_path = ?1 OR SUBSTR(file_path, 1, LENGTH(?1) + 2) = ?1 || '::'",
        params![file_path],
        |r| r.get(0),
    )
}

pub fn get_rom_file_info(
    conn: &Connection,
    rom_id: i64,
) -> rusqlite::Result<Option<(String, Option<String>, String)>> {
    conn.query_row(
        "SELECT file_path, archive_member, file_name FROM roms WHERE id = ?1",
        params![rom_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

pub fn delete_rom_rows_for_file(conn: &Connection, file_path: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM roms
         WHERE file_path = ?1 OR SUBSTR(file_path, 1, LENGTH(?1) + 2) = ?1 || '::'",
        params![file_path],
    )?;
    Ok(())
}

/// Repoints rom rows after the underlying file has been renamed on disk. A
/// loose ROM takes the new name directly; archive members keep their own
/// file_name (the member) and only have the archive part of the key rewritten.
pub fn update_rom_file_path(
    conn: &Connection,
    old_path: &str,
    new_path: &str,
    new_file_name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE roms SET file_path = ?2, file_name = ?3 WHERE file_path = ?1",
        params![old_path, new_path, new_file_name],
    )?;
    conn.execute(
        "UPDATE roms SET file_path = ?2 || SUBSTR(file_path, LENGTH(?1) + 1)
         WHERE SUBSTR(file_path, 1, LENGTH(?1) + 2) = ?1 || '::'",
        params![old_path, new_path],
    )?;
    Ok(())
}
