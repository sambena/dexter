use std::collections::HashMap;
use std::io::{Cursor, Read};

/// Best-effort mapping from a system's folder name (case-insensitive) to a
/// Redump datfile slug. Redump is the only DAT source we know of that exposes
/// a stable, unauthenticated, directly-downloadable URL per system; No-Intro's
/// Dat-o-Matic is account/UI-gated and has no such endpoint, so systems not
/// listed here (or any No-Intro set) must be imported manually from a file.
fn redump_slug(folder_name: &str) -> Option<&'static str> {
    let key = folder_name.to_lowercase();
    let map: HashMap<&str, &str> = HashMap::from([
        ("psx", "psx"),
        ("ps1", "psx"),
        ("playstation", "psx"),
        ("ps2", "ps2"),
        ("playstation2", "ps2"),
        ("psp", "psp"),
        ("gc", "gc"),
        ("gamecube", "gc"),
        ("wii", "wii"),
        ("dc", "dc"),
        ("dreamcast", "dc"),
        ("saturn", "saturn"),
        ("segacd", "segacd"),
        ("3do", "3do"),
        ("pcengine", "pce-cd"),
        ("pcenginecd", "pce-cd"),
    ]);
    map.get(key.as_str()).copied()
}

pub fn has_known_source(folder_name: &str) -> bool {
    redump_slug(folder_name).is_some()
}

/// Downloads a DAT for the given system folder name from its known direct
/// source (currently only Redump). Redump serves its datfiles zipped, so we
/// transparently unzip if the response looks like a zip archive; otherwise we
/// assume the response is already raw DAT/XML text.
pub fn fetch_dat_text(folder_name: &str) -> anyhow::Result<(String, String)> {
    let slug = redump_slug(folder_name).ok_or_else(|| {
        anyhow::anyhow!(
            "No known automatic DAT source for system \"{}\". Please use \"Import DAT\" and pick a file yourself (e.g. from No-Intro's Dat-o-Matic).",
            folder_name
        )
    })?;

    let url = format!("http://redump.org/datfile/{}/", slug);
    let response = reqwest::blocking::get(&url)?.error_for_status()?;
    let bytes = response.bytes()?;

    if bytes.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_ref()))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name.to_lowercase().ends_with(".dat") || name.to_lowercase().ends_with(".xml") {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                return Ok((name, text));
            }
        }
        anyhow::bail!("Redump archive for \"{}\" did not contain a .dat file", folder_name);
    }

    let text = String::from_utf8(bytes.to_vec())?;
    Ok((format!("{}.dat", slug), text))
}
