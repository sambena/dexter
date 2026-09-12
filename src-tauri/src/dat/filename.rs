use regex::Regex;
use std::sync::OnceLock;

/// A parenthesised or bracketed tag, e.g. "(USA)" or "[!]".
fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\(([^)]*)\)|\[([^\]]*)\]").unwrap())
}

pub struct FilenameMetadata {
    pub title: String,
    pub region: Option<String>,
    pub year: Option<String>,
}

/// Wrappers around another file type, so "Game (Disc 1).bin.ecm" loses both.
const CONTAINER_EXTENSIONS: &[&str] = &["ecm", "rar", "zip", "7z", "gz", "xz", "zst"];

/// Region names used in No-Intro/Redump tags such as "(USA, Europe)".
const REGION_NAMES: &[&str] = &[
    "USA", "Europe", "Japan", "World", "Australia", "Korea", "China", "Taiwan", "Hong Kong", "Asia",
    "Germany", "France", "Spain", "Italy", "Netherlands", "Sweden", "Denmark", "Norway", "Finland",
    "Scandinavia", "Portugal", "Greece", "Poland", "Russia", "UK", "Canada", "Brazil", "Mexico",
    "Latin America",
];

/// GoodTools single-tag region codes, e.g. "(U)" or "(JUE)".
fn goodtools_region(code: &str) -> Option<String> {
    let names: Option<Vec<&str>> = code
        .chars()
        .map(|c| match c {
            'U' => Some("USA"),
            'E' => Some("Europe"),
            'J' => Some("Japan"),
            'W' => Some("World"),
            'A' => Some("Australia"),
            'K' => Some("Korea"),
            'C' => Some("China"),
            'G' => Some("Germany"),
            'F' => Some("France"),
            'S' => Some("Spain"),
            'I' => Some("Italy"),
            'B' => Some("Brazil"),
            _ => None,
        })
        .collect();
    // Combined codes are only ever USA/Europe/Japan ("(UE)", "(JUE)"); other
    // letter runs like "(SGB)" are something else entirely.
    let combinable = code.len() == 1 || (code.len() <= 3 && code.chars().all(|c| "UEJ".contains(c)));
    names.filter(|n| !n.is_empty() && combinable).map(|n| n.join(", "))
}

fn region_of(tag: &str) -> Option<String> {
    let parts: Vec<&str> = tag.split(',').map(str::trim).collect();
    if parts.iter().all(|p| REGION_NAMES.iter().any(|r| r.eq_ignore_ascii_case(p))) {
        return Some(parts.join(", "));
    }
    goodtools_region(tag)
}

/// Splits off a real file extension: short, alphanumeric and containing a
/// letter. That rules out the dots inside names like "Super Mario Bros. U",
/// "(1.740 GB)" or "Game v1.2", which folder names in particular are full of.
fn split_extension(name: &str) -> Option<(&str, &str)> {
    let dot = name.rfind('.')?;
    let (stem, ext) = (&name[..dot], &name[dot + 1..]);
    let looks_like_extension = !stem.is_empty()
        && (1..=6).contains(&ext.len())
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && ext.chars().any(|c| c.is_ascii_alphabetic());
    looks_like_extension.then_some((stem, ext))
}

fn strip_extensions(name: &str) -> &str {
    let Some((stem, ext)) = split_extension(name) else {
        return name;
    };
    let is_container = CONTAINER_EXTENSIONS.iter().any(|c| c.eq_ignore_ascii_case(ext));
    match split_extension(stem) {
        Some((inner_stem, _)) if is_container => inner_stem,
        _ => stem,
    }
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

enum Tag {
    Year(String),
    Region(String),
    /// Distinguishes files that would otherwise share a title: discs, DLC
    /// and updates.
    Keep(String),
    Version(String),
    Other,
}

fn classify(tag: &str) -> Tag {
    let tag = tag.trim();
    let lower = tag.to_ascii_lowercase();
    if tag.len() == 4 && tag.chars().all(|c| c.is_ascii_digit()) {
        return Tag::Year(tag.to_string());
    }
    if let Some(region) = region_of(tag) {
        return Tag::Region(region);
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        ["disc" | "disk", n] => return Tag::Keep(format!("Disc {}", n.to_ascii_uppercase())),
        ["side", s] => return Tag::Keep(format!("Side {}", s.to_ascii_uppercase())),
        ["dlc"] => return Tag::Keep("DLC".to_string()),
        ["update"] | ["update", "data"] => return Tag::Keep("Update".to_string()),
        _ => {}
    }
    let is_version = lower.len() > 1
        && lower.starts_with('v')
        && lower[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
        && lower[1..].chars().next().is_some_and(|c| c.is_ascii_digit());
    if is_version {
        return Tag::Version(tag.to_string());
    }
    Tag::Other
}

/// Best-effort title/region/year extraction from a raw filename, for ROMs with
/// no DAT match — purely cosmetic (never treated as verified identification).
/// Handles No-Intro, Redump and GoodTools naming, e.g.
/// "007 - GoldenEye (U) [!].z64" -> title "007 - GoldenEye", region "USA", and
/// Wii U folder names like "MARIO KART 8 (UPDATE DATA) (v64) (499.352 MB) (USA)".
pub fn parse_filename_metadata(file_name: &str) -> FilenameMetadata {
    let stem = strip_extensions(file_name);

    let mut year = None;
    let mut region = None;
    let mut kept: Vec<String> = Vec::new();
    let mut version = None;
    for cap in tag_re().captures_iter(stem) {
        let is_paren = cap.get(1).is_some();
        let text = cap.get(1).or_else(|| cap.get(2)).map_or("", |m| m.as_str());
        match classify(text) {
            Tag::Year(y) if is_paren => {
                year.get_or_insert(y);
            }
            // Brackets hold GoodTools dump flags ("[S]", "[C]"), not regions.
            Tag::Region(r) if is_paren => {
                region.get_or_insert(r);
            }
            Tag::Keep(k) => {
                if !kept.contains(&k) {
                    kept.push(k);
                }
            }
            Tag::Version(v) => {
                version.get_or_insert(v);
            }
            _ => {}
        }
    }

    // A version only tells updates apart; on anything else it's noise.
    if let (Some(update), Some(v)) = (kept.iter_mut().find(|k| *k == "Update"), version) {
        *update = format!("Update {}", v);
    }

    let bare = collapse_whitespace(&tag_re().replace_all(stem, " "));
    let mut title = bare.trim_end_matches([' ', '-', ',']).to_string();
    if title.is_empty() {
        title = collapse_whitespace(stem);
    }
    for k in kept {
        title.push_str(&format!(" ({})", k));
    }

    FilenameMetadata { title, region, year }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(name: &str) -> String {
        parse_filename_metadata(name).title
    }

    #[test]
    fn no_intro_style() {
        let m = parse_filename_metadata("007 - GoldenEye (USA) [C][!].n64");
        assert_eq!(m.title, "007 - GoldenEye");
        assert_eq!(m.region.as_deref(), Some("USA"));
    }

    #[test]
    fn goodtools_region_codes() {
        assert_eq!(parse_filename_metadata("Pang (U).gb").region.as_deref(), Some("USA"));
        assert_eq!(parse_filename_metadata("Game (JUE).sfc").region.as_deref(), Some("Japan, USA, Europe"));
        // Dump flags in brackets are not regions, nor are other letter runs.
        assert_eq!(parse_filename_metadata("Game & Watch Gallery 2 [S].gb").region, None);
        assert_eq!(parse_filename_metadata("Tetris (BPS).nes").region, None);
        assert_eq!(parse_filename_metadata("Game (SGB).gb").region, None);
    }

    #[test]
    fn non_region_tags_are_not_regions() {
        let m = parse_filename_metadata("Ms. Pac Man (Tengen).nes");
        assert_eq!(m.title, "Ms. Pac Man");
        assert_eq!(m.region, None);
        let m = parse_filename_metadata("Super Mario Bros. Deluxe (U) (V1.0) [C][f1].gbc");
        assert_eq!(m.title, "Super Mario Bros. Deluxe");
        assert_eq!(m.region.as_deref(), Some("USA"));
    }

    #[test]
    fn dots_inside_folder_names_are_not_extensions() {
        assert_eq!(title("New Super Mario Bros. [DADE01]"), "New Super Mario Bros.");
        assert_eq!(title("Super Smash Bros. for Wii U [AXFE01]"), "Super Smash Bros. for Wii U");
        assert_eq!(title("Emulator Pack v1.2"), "Emulator Pack v1.2");
        assert_eq!(title("M.U.S.C.L.E..nes"), "M.U.S.C.L.E.");
    }

    #[test]
    fn wii_u_update_and_dlc_folders_stay_distinct() {
        let m = parse_filename_metadata("MARIO KART 8 (UPDATE DATA) (v64) (499.352 MB) (USA) (unpacked)");
        assert_eq!(m.title, "MARIO KART 8 (Update v64)");
        assert_eq!(m.region.as_deref(), Some("USA"));
        assert_eq!(title("MARIO KART 8 (DLC) (1.740 GB) (USA) (unpacked)"), "MARIO KART 8 (DLC)");
        assert_eq!(title("MARIO KART 8 [AMKE01]"), "MARIO KART 8");
        assert_eq!(
            title("THE LEGEND OF ZELDA Twilight Princess HD [Update] [0005000e1019e500]"),
            "THE LEGEND OF ZELDA Twilight Princess HD (Update)"
        );
    }

    #[test]
    fn discs_and_double_extensions() {
        let m = parse_filename_metadata("Final Fantasy VII (Europe) (Disc 1).bin.ecm");
        assert_eq!(m.title, "Final Fantasy VII (Disc 1)");
        assert_eq!(m.region.as_deref(), Some("Europe"));
        assert_eq!(
            title("The Legend of Zelda Twilight Princess HD [AZAP01].part03.rar"),
            "The Legend of Zelda Twilight Princess HD"
        );
    }

    #[test]
    fn whitespace_left_by_removed_tags_is_collapsed() {
        let m = parse_filename_metadata(
            "Legend of Zelda, The - Ocarina of Time 3D (USA) (En,Fr,Es) (Rev 1) Decrypted.3ds",
        );
        assert_eq!(m.title, "Legend of Zelda, The - Ocarina of Time 3D Decrypted");
        assert_eq!(m.region.as_deref(), Some("USA"));
        assert_eq!(title("New SUPER MARIO BROS. U +  New SUPER LUIGI U [ATWE01]"), "New SUPER MARIO BROS. U + New SUPER LUIGI U");
    }

    #[test]
    fn year_and_multi_region() {
        let m = parse_filename_metadata("Some Game (1995) (USA, Europe).md");
        assert_eq!(m.title, "Some Game");
        assert_eq!(m.year.as_deref(), Some("1995"));
        assert_eq!(m.region.as_deref(), Some("USA, Europe"));
    }

    #[test]
    fn name_that_is_only_tags_falls_back_to_stem() {
        assert_eq!(title("[BIOS].bin"), "[BIOS]");
    }
}
