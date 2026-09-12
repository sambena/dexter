use crate::models::{RomDetailsDto, RomFilter, RomListItemDto, Settings, SystemDto};
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
        "SELECT s.id, s.name, s.folder_name, s.emulator_path, s.emulator_args,
                d.dat_name, d.dat_version, d.imported_at
         FROM systems s
         LEFT JOIN dat_sources d ON d.system_id = s.id
         ORDER BY s.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SystemDto {
            id: r.get(0)?,
            name: r.get(1)?,
            folder_name: r.get(2)?,
            emulator_path: r.get(3)?,
            emulator_args: r.get(4)?,
            dat_name: r.get(5)?,
            dat_version: r.get(6)?,
            dat_imported_at: r.get(7)?,
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

pub fn get_system_folder_name(conn: &Connection, system_id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT folder_name FROM systems WHERE id = ?1",
        params![system_id],
        |r| r.get(0),
    )
    .optional()
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

pub fn replace_dat_source(
    conn: &mut Connection,
    system_id: i64,
    file_name: &str,
    dat_name: Option<&str>,
    dat_version: Option<&str>,
    games: &[DatGameImport],
) -> rusqlite::Result<(i64, i64)> {
    let tx = conn.transaction()?;
    // Removing the old dat_source cascades to dat_games/dat_roms (and NULLs matched roms.dat_rom_id)
    tx.execute(
        "DELETE FROM dat_sources WHERE system_id = ?1",
        params![system_id],
    )?;
    let now = chrono::Utc::now().to_rfc3339();
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

#[allow(clippy::too_many_arguments)]
pub fn upsert_rom(
    conn: &Connection,
    system_id: i64,
    file_path: &str,
    file_name: &str,
    size: i64,
    crc32: &str,
    md5: &str,
    sha1: &str,
    archive_member: Option<&str>,
    dat_rom_id: Option<i64>,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let match_status = if dat_rom_id.is_some() { "matched" } else { "unmatched" };
    conn.execute(
        "INSERT INTO roms (system_id, file_path, file_name, size, crc32, md5, sha1, archive_member, dat_rom_id, match_status, last_scanned_at, seen_this_scan)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1)
         ON CONFLICT(file_path) DO UPDATE SET
            system_id = excluded.system_id,
            file_name = excluded.file_name,
            size = excluded.size,
            crc32 = excluded.crc32,
            md5 = excluded.md5,
            sha1 = excluded.sha1,
            archive_member = excluded.archive_member,
            dat_rom_id = excluded.dat_rom_id,
            match_status = excluded.match_status,
            last_scanned_at = excluded.last_scanned_at,
            seen_this_scan = 1",
        params![system_id, file_path, file_name, size, crc32, md5, sha1, archive_member, dat_rom_id, match_status, now],
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

// ---------- browsing ----------

pub fn list_roms(conn: &Connection, filter: &RomFilter) -> rusqlite::Result<Vec<RomListItemDto>> {
    let mut sql = String::from(
        "SELECT r.id, r.file_name, r.system_id, s.name,
                COALESCE(dg.name, r.file_name), r.match_status
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
        Ok(RomListItemDto {
            id: r.get(0)?,
            file_name: r.get(1)?,
            system_id: r.get(2)?,
            system_name: r.get(3)?,
            display_name: r.get(4)?,
            match_status: r.get(5)?,
        })
    })?;
    rows.collect()
}

pub fn get_rom_details(conn: &Connection, rom_id: i64) -> rusqlite::Result<Option<RomDetailsDto>> {
    conn.query_row(
        "SELECT r.id, r.file_name, r.file_path, r.archive_member, r.system_id, s.name,
                dg.year, dg.region, COALESCE(dg.name, r.file_name), r.match_status,
                r.crc32, r.md5, r.sha1, s.emulator_path, s.emulator_args
         FROM roms r
         LEFT JOIN systems s ON s.id = r.system_id
         LEFT JOIN dat_roms dr ON dr.id = r.dat_rom_id
         LEFT JOIN dat_games dg ON dg.id = dr.dat_game_id
         WHERE r.id = ?1",
        params![rom_id],
        |r| {
            Ok(RomDetailsDto {
                id: r.get(0)?,
                file_name: r.get(1)?,
                file_path: r.get(2)?,
                archive_member: r.get(3)?,
                system_id: r.get(4)?,
                system_name: r.get(5)?,
                year: r.get(6)?,
                region: r.get(7)?,
                display_name: r.get(8)?,
                match_status: r.get(9)?,
                crc32: r.get(10)?,
                md5: r.get(11)?,
                sha1: r.get(12)?,
                emulator_path: r.get(13)?,
                emulator_args: r.get(14)?,
            })
        },
    )
    .optional()
}
