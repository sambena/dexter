use crate::models::SystemDto;
use regex::Regex;
use std::sync::OnceLock;

fn normalize(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// No-Intro/DAT header names are usually "Manufacturer - System Name" —
/// strip the manufacturer so "Nintendo - Game Boy Color" compares against "Game Boy Color".
fn strip_manufacturer_prefix(name: &str) -> &str {
    name.rsplit(" - ").next().unwrap_or(name)
}

/// Dat-o-Matic often appends a variant qualifier in parens, e.g. "Nintendo -
/// Nintendo Entertainment System (Headered)" or "... Nintendo 64 (BigEndian)" —
/// strip trailing "(...)" groups so they don't break comparison against a
/// system's plain name.
fn strip_trailing_qualifiers(name: &str) -> String {
    fn paren_suffix_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\s*\([^)]*\)\s*$").unwrap())
    }
    let mut s = name.to_string();
    loop {
        let trimmed = paren_suffix_re().replace(&s, "").into_owned();
        if trimmed.len() == s.len() {
            return s;
        }
        s = trimmed;
    }
}

/// A handful of very common short folder names that don't otherwise overlap
/// with their official DAT name as a substring (e.g. "nes" vs "Nintendo
/// Entertainment System").
fn common_alias(folder_key: &str) -> Option<&'static str> {
    match folder_key {
        "nes" => Some("nintendoentertainmentsystem"),
        "snes" => Some("supernintendoentertainmentsystem"),
        "gb" => Some("gameboy"),
        "gbc" => Some("gameboycolor"),
        "gba" => Some("gameboyadvance"),
        "n64" => Some("nintendo64"),
        "gc" => Some("nintendogamecube"),
        "ds" => Some("nintendods"),
        "3ds" => Some("nintendo3ds"),
        "psx" | "ps1" => Some("sonyplaystation"),
        "ps2" => Some("sonyplaystation2"),
        "psp" => Some("sonyplaystationportable"),
        "genesis" | "megadrive" => Some("segamegadrivegenesis"),
        "mastersystem" => Some("segamastersystemmarkiii"),
        "gamegear" => Some("segagamegear"),
        "dc" => Some("segadreamcast"),
        _ => None,
    }
}

/// Best-effort match of a DAT's header/game-set name to one of the user's
/// existing systems (created from their ROM folder names). Returns None when
/// no confident match is found, rather than guessing wrong.
///
/// Exact matches are always checked first across every system before any
/// substring fallback runs — otherwise a short name like "Game Boy" can
/// falsely claim a DAT meant for "Game Boy Advance"/"Game Boy Color" simply
/// because its normalized form is a substring of theirs. The substring
/// fallback only fires when it identifies exactly one candidate, so an
/// ambiguous partial match reports "unmatched" instead of guessing wrong.
pub fn find_matching_system(systems: &[SystemDto], dat_name: &str) -> Option<i64> {
    let candidate = normalize(&strip_trailing_qualifiers(strip_manufacturer_prefix(dat_name)));
    if candidate.is_empty() {
        return None;
    }

    if let Some(s) = systems.iter().find(|s| {
        let name_norm = normalize(&s.name);
        let folder_norm = normalize(&s.folder_name);
        let alias = common_alias(&folder_norm);
        name_norm == candidate || folder_norm == candidate || alias == Some(candidate.as_str())
    }) {
        return Some(s.id);
    }

    let mut matches = systems.iter().filter(|s| {
        let name_norm = normalize(&s.name);
        let folder_norm = normalize(&s.folder_name);
        name_norm.contains(&candidate)
            || candidate.contains(&name_norm)
            || folder_norm.contains(&candidate)
            || candidate.contains(&folder_norm)
    });

    let first = matches.next()?;
    if matches.next().is_some() {
        return None; // ambiguous — don't guess
    }
    Some(first.id)
}
