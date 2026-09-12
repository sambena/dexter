use std::collections::HashMap;
use std::io::{Cursor, Read};

/// Best-effort mapping from a system's folder name (case-insensitive) to a
/// Redump datfile slug. Redump is the only DAT source we know of that exposes
/// a stable, unauthenticated, directly-downloadable URL per system; No-Intro's
/// Dat-o-Matic is account/UI-gated and has no such endpoint, so systems not
/// listed here need either a manually-imported file or a user-supplied custom
/// DAT URL (see fetch_dat_from_url).
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

/// Extracts DAT/XML text from a byte buffer, transparently unwrapping the first
/// .dat/.xml entry if the buffer is a zip archive (Dat-o-Matic, Redump, and most
/// DAT mirrors distribute datfiles zipped) — otherwise treats it as raw text.
pub fn extract_dat_text(bytes: &[u8], fallback_name: &str) -> anyhow::Result<(String, String)> {
    if bytes.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name.to_lowercase().ends_with(".dat") || name.to_lowercase().ends_with(".xml") {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                return Ok((name, text));
            }
        }
        anyhow::bail!("The zip file didn't contain a .dat or .xml file");
    }

    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| anyhow::anyhow!("File isn't valid text or a zip archive containing a .dat/.xml file"))?;
    Ok((fallback_name.to_string(), text))
}

/// Downloads and returns (filename, text) for whatever a URL points at.
pub fn fetch_dat_from_url(url: &str) -> anyhow::Result<(String, String)> {
    let response = reqwest::blocking::get(url)?.error_for_status()?;
    let bytes = response.bytes()?;
    let fallback_name = url.rsplit('/').find(|s| !s.is_empty()).unwrap_or("import.dat");
    extract_dat_text(&bytes, fallback_name)
}

/// Downloads a DAT for the given system folder name from its known direct
/// source (currently only Redump).
pub fn fetch_dat_text(folder_name: &str) -> anyhow::Result<(String, String)> {
    let slug = redump_slug(folder_name).ok_or_else(|| {
        anyhow::anyhow!(
            "No known automatic DAT source for system \"{}\". Set a custom DAT URL in Settings, or use \"Import DAT\" to pick a file yourself.",
            folder_name
        )
    })?;
    let url = format!("http://redump.org/datfile/{}/", slug);
    fetch_dat_from_url(&url)
}
