//! Reads what an extracted Wii U title folder says about itself. A folder
//! dump can't be verified by hash, but its XML files name the title, and
//! tell a game apart from an update or DLC for it, which folder names only
//! sometimes do.
//!
//! - `code/app.xml` is written for the installed title itself, so its title
//!   ID and version are authoritative: an update's ID starts 0005000E.
//! - `meta/meta.xml` has the display name and product code, but an update's
//!   copy often still carries the base game's title ID and version.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TitleKind {
    Game,
    Update,
    Dlc,
    Demo,
}

impl TitleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TitleKind::Game => "game",
            TitleKind::Update => "update",
            TitleKind::Dlc => "dlc",
            TitleKind::Demo => "demo",
        }
    }

    pub fn from_str(s: &str) -> Option<TitleKind> {
        match s {
            "game" => Some(TitleKind::Game),
            "update" => Some(TitleKind::Update),
            "dlc" => Some(TitleKind::Dlc),
            "demo" => Some(TitleKind::Demo),
            _ => None,
        }
    }

    /// The high half of a title ID says what kind of title it is.
    fn from_title_id(title_id: &str) -> Option<TitleKind> {
        match title_id.get(..8)?.to_ascii_uppercase().as_str() {
            "00050000" => Some(TitleKind::Game),
            "0005000E" => Some(TitleKind::Update),
            "0005000C" => Some(TitleKind::Dlc),
            "00050002" => Some(TitleKind::Demo),
            _ => None,
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            TitleKind::Game => "00050000",
            TitleKind::Update => "0005000E",
            TitleKind::Dlc => "0005000C",
            TitleKind::Demo => "00050002",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TitleInfo {
    /// 16 hex digits, uppercase.
    pub title_id: String,
    pub version: u32,
    pub kind: TitleKind,
    /// English long name, on one line.
    pub name: String,
    /// e.g. "WUP-P-AMKE".
    pub product_code: Option<String>,
}

/// The DAT region a product code's region letter stands for: the last letter
/// of a Wii U code ("WUP-P-AMKE" is USA), or the fourth character of a
/// GameCube/Wii game ID ("SOUE01").
pub fn region_for_product_code(code: &str) -> Option<&'static str> {
    let letter = if code.len() == 6 && !code.contains('-') { code.chars().nth(3)? } else { code.chars().last()? };
    match letter {
        'E' => Some("USA"),
        'P' => Some("Europe"),
        'J' => Some("Japan"),
        'K' => Some("Korea"),
        'D' => Some("Germany"),
        'F' => Some("France"),
        'S' => Some("Spain"),
        'I' => Some("Italy"),
        'U' => Some("Australia"),
        _ => None,
    }
}

impl TitleInfo {
    pub fn region(&self) -> Option<&'static str> {
        region_for_product_code(self.product_code.as_deref()?)
    }
}

/// The text of the first `<tag ...>text</tag>` in `xml`, with entities decoded.
fn xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let mut search = 0;
    let start = loop {
        let at = search + xml[search..].find(&open)?;
        let after = &xml[at + open.len()..];
        // "<title_id" must not match "<title_idx".
        if after.starts_with('>') || after.starts_with(char::is_whitespace) {
            break at + open.len() + after.find('>')? + 1;
        }
        search = at + open.len();
    };
    let end = start + xml[start..].find(&format!("</{}>", tag))?;
    let text = xml[start..end]
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    Some(text)
}

fn parse_version(text: &str, hex: bool) -> Option<u32> {
    let text = text.trim();
    if hex {
        u32::from_str_radix(text, 16).ok()
    } else {
        text.parse().ok()
    }
}

/// Folder names are the only hint when app.xml is missing.
fn kind_from_folder_name(name: &str) -> Option<TitleKind> {
    let lower = name.to_lowercase();
    if lower.contains("update") {
        Some(TitleKind::Update)
    } else if lower.contains("dlc") || lower == "aoc" {
        Some(TitleKind::Dlc)
    } else {
        None
    }
}

pub fn parse_title_info(folder_name: &str, app_xml: Option<&str>, meta_xml: &str) -> Option<TitleInfo> {
    let name = xml_text(meta_xml, "longname_en")?.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    let product_code = xml_text(meta_xml, "product_code").map(|c| c.trim().to_string()).filter(|c| !c.is_empty());

    let app_id = app_xml.and_then(|x| xml_text(x, "title_id")).map(|id| id.trim().to_ascii_uppercase());
    let (title_id, version, kind) = match (app_id, app_xml) {
        (Some(id), Some(app)) if id.len() == 16 => {
            let kind = TitleKind::from_title_id(&id).unwrap_or(TitleKind::Game);
            let version = xml_text(app, "title_version").and_then(|v| parse_version(&v, true)).unwrap_or(0);
            (id, version, kind)
        }
        _ => {
            let meta_id = xml_text(meta_xml, "title_id")?.trim().to_ascii_uppercase();
            if meta_id.len() != 16 {
                return None;
            }
            let kind = kind_from_folder_name(folder_name).unwrap_or(TitleKind::Game);
            let version = xml_text(meta_xml, "title_version").and_then(|v| parse_version(&v, false)).unwrap_or(0);
            (format!("{}{}", kind.id_prefix(), &meta_id[8..]), version, kind)
        }
    };
    Some(TitleInfo { title_id, version, kind, name, product_code })
}

/// Reads a title folder's XML files. None if it has no readable meta.xml.
pub fn read_title_info(dir: &Path) -> Option<TitleInfo> {
    let meta = std::fs::read(dir.join("meta").join("meta.xml")).ok()?;
    let app = std::fs::read(dir.join("code").join("app.xml")).ok();
    let folder_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    parse_title_info(
        folder_name,
        app.as_deref().map(|a| String::from_utf8_lossy(a)).as_deref(),
        &String::from_utf8_lossy(&meta),
    )
}

/// A title for comparing names across sources: DATs write "Legend of Zelda,
/// The - Majora's Mask (USA) (N64) (Virtual Console)", while the console
/// says "The Legend of Zelda: Majora's Mask".
pub fn comparable_title(name: &str) -> String {
    let base = name.split(" (").next().unwrap_or(name);
    let base = match base.split_once(", The") {
        Some((head, tail)) if tail.is_empty() || tail.starts_with(" - ") => format!("The {}{}", head, tail),
        _ => base.to_string(),
    };
    base.replace('&', "and").chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// What kind of title a DAT game name describes.
pub fn dat_name_kind(name: &str) -> TitleKind {
    if name.contains("(Update)") {
        TitleKind::Update
    } else if name.contains("(DLC)") {
        TitleKind::Dlc
    } else if name.contains("(Demo)") {
        TitleKind::Demo
    } else {
        TitleKind::Game
    }
}

/// Picks the DAT game a title folder is: same title and kind, preferring the
/// region its product code names. None if the DAT doesn't list it.
pub fn identify<'a>(info: &TitleInfo, dat_games: impl IntoIterator<Item = (i64, &'a str)>) -> Option<i64> {
    let key = comparable_title(&info.name);
    let candidates: Vec<(i64, &str)> = dat_games
        .into_iter()
        .filter(|(_, name)| dat_name_kind(name) == info.kind && comparable_title(name) == key)
        .collect();
    let in_region: Vec<&(i64, &str)> = match info.region() {
        Some(region) => candidates
            .iter()
            .filter(|(_, name)| {
                // The first parenthesised group is the region list, e.g. "(USA, Europe)".
                name.split(" (").nth(1).is_some_and(|group| group.trim_end_matches(')').split(", ").any(|r| r == region))
            })
            .collect(),
        None => Vec::new(),
    };
    let pool: Vec<&(i64, &str)> = if in_region.is_empty() { candidates.iter().collect() } else { in_region };
    // Discs name their revision; "(Rev 1)" in a DAT name is revision 1.
    let revision_of = |name: &str| {
        name.split("(Rev ").nth(1).and_then(|rest| rest.split(')').next()).and_then(|n| n.trim().parse::<u32>().ok()).unwrap_or(0)
    };
    pool.iter()
        .find(|(_, name)| revision_of(name) == info.version)
        .or(pool.first())
        .map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MK8_UPDATE_META: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<menu type="complex" access="777">
  <version type="unsignedInt" length="4">33</version>
  <product_code type="string" length="32">WUP-P-AMKE</product_code>
  <title_id type="hexBinary" length="8">000500001010EC00</title_id>
  <title_version type="unsignedInt" length="4">64</title_version>
  <longname_en type="string" length="512">MARIO KART 8</longname_en>
</menu>"#;

    const MK8_UPDATE_APP: &str = r#"<app type="complex" access="777">
  <version type="unsignedInt" length="4">16</version>
  <os_version type="hexBinary" length="8">000500101000400A</os_version>
  <title_id type="hexBinary" length="8">0005000E1010EC00</title_id>
  <title_version type="hexBinary" length="2">0040</title_version>
</app>"#;

    #[test]
    fn app_xml_decides_kind_and_version() {
        let info = parse_title_info("MARIO KART 8 (UPDATE DATA) (v64)", Some(MK8_UPDATE_APP), MK8_UPDATE_META).unwrap();
        assert_eq!(info.title_id, "0005000E1010EC00");
        assert_eq!((info.kind, info.version), (TitleKind::Update, 64));
        assert_eq!(info.name, "MARIO KART 8");
        assert_eq!(info.region(), Some("USA"));
    }

    #[test]
    fn without_app_xml_the_folder_name_decides_kind() {
        let meta = r#"<product_code>WUP-P-AZAP</product_code>
<title_id type="hexBinary" length="8">0005000E1019C800</title_id>
<title_version type="unsignedInt" length="4">0</title_version>
<longname_en type="string" length="512">THE LEGEND OF ZELDA
Twilight Princess HD</longname_en>"#;
        let game = parse_title_info("The Legend of Zelda Twilight Princess HD [AZAP01]", None, meta).unwrap();
        assert_eq!((game.kind, game.title_id.as_str()), (TitleKind::Game, "000500001019C800"));
        assert_eq!(game.name, "THE LEGEND OF ZELDA Twilight Princess HD");
        assert_eq!(game.region(), Some("Europe"));

        let update = parse_title_info("TP HD Update", None, meta).unwrap();
        assert_eq!(update.kind, TitleKind::Update);
    }

    #[test]
    fn names_are_decoded_and_joined_onto_one_line() {
        let meta = "<title_id>00050000101BAF00</title_id><longname_en length=\"512\">The Legend of Zelda: \n Majora&apos;s Mask &amp; More</longname_en>";
        let info = parse_title_info("x", None, meta).unwrap();
        assert_eq!(info.name, "The Legend of Zelda: Majora's Mask & More");
    }

    #[test]
    fn console_and_dat_titles_compare_equal() {
        assert_eq!(
            comparable_title("The Legend of Zelda: Majora's Mask"),
            comparable_title("Legend of Zelda, The - Majora's Mask (USA) (N64) (Virtual Console)")
        );
        assert_eq!(comparable_title("New SUPER MARIO BROS. U + New SUPER LUIGI U"), comparable_title("New Super Mario Bros. U + New Super Luigi U (USA) (En,Fr,Es)"));
        assert_ne!(comparable_title("Mario Kart 8"), comparable_title("Mario Kart 7 (USA)"));
    }

    #[test]
    fn identify_picks_kind_then_region() {
        let dat = [
            (1, "Mario Kart 8 (Europe)"),
            (2, "Mario Kart 8 (USA) (En,Fr,Es)"),
            (3, "Mario Kart 8 (USA) (En,Fr,Es) (Update)"),
            (4, "Mario Kart 8 (USA) (En,Fr,Es) (DLC)"),
            (5, "Mario Kart 7 (USA)"),
        ];
        let mut info = parse_title_info("MARIO KART 8 (UPDATE DATA)", Some(MK8_UPDATE_APP), MK8_UPDATE_META).unwrap();
        assert_eq!(identify(&info, dat), Some(3));
        info.kind = TitleKind::Game;
        assert_eq!(identify(&info, dat), Some(2));
        info.product_code = Some("WUP-P-AMKP".into());
        assert_eq!(identify(&info, dat), Some(1));
        info.kind = TitleKind::Demo;
        assert_eq!(identify(&info, dat), None);
    }

    #[test]
    fn identify_prefers_the_disc_revision() {
        let dat = [
            (1, "Legend of Zelda, The - Skyward Sword (USA) (En,Fr,Es)"),
            (2, "Legend of Zelda, The - Skyward Sword (USA) (En,Fr,Es) (Rev 1)"),
            (3, "Legend of Zelda, The - Skyward Sword (Europe) (En,Fr,De,Es,It)"),
        ];
        let mut info = TitleInfo {
            title_id: "SOUE01".into(),
            version: 1,
            kind: TitleKind::Game,
            name: "The Legend of Zelda Skyward Sword".into(),
            product_code: Some("SOUE01".into()),
        };
        assert_eq!(identify(&info, dat), Some(2));
        info.version = 0;
        assert_eq!(identify(&info, dat), Some(1));
        info.product_code = Some("SOUP01".into());
        assert_eq!(identify(&info, dat), Some(3));
    }
}
