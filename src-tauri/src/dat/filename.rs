use regex::Regex;
use std::sync::OnceLock;

fn paren_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\(([^)]+)\)").unwrap())
}

fn bracket_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]*\]").unwrap())
}

pub struct FilenameMetadata {
    pub title: String,
    pub region: Option<String>,
    pub year: Option<String>,
}

/// Best-effort title/region/year extraction from a raw filename, for ROMs with
/// no DAT match — purely cosmetic (never treated as verified identification).
/// Handles common No-Intro/TOSEC-style naming, e.g.
/// "007 - GoldenEye (USA) [C][!].n64" -> title "007 - GoldenEye", region "USA".
pub fn parse_filename_metadata(file_name: &str) -> FilenameMetadata {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);

    let mut year = None;
    let mut region = None;
    for cap in paren_re().captures_iter(stem) {
        let val = cap[1].to_string();
        if year.is_none() && val.len() == 4 && val.chars().all(|c| c.is_ascii_digit()) {
            year = Some(val);
        } else if region.is_none() {
            region = Some(val);
        }
    }

    let without_brackets = bracket_re().replace_all(stem, "");
    let without_parens = paren_re().replace_all(&without_brackets, "");
    let mut title = without_parens.trim().trim_end_matches('-').trim().to_string();
    if title.is_empty() {
        title = stem.to_string();
    }

    FilenameMetadata { title, region, year }
}
