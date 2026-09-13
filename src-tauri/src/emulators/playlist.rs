//! RetroArch playlists (.lpl) and the thumbnail names RetroArch looks for.
//!
//! RetroArch lists games from JSON playlists in its playlists folder, one per
//! system, and shows box art from
//! `thumbnails/<playlist name>/Named_Boxarts/<entry label>.png`, with a few
//! characters in the label replaced by "_".

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// One library file as it goes into a playlist.
pub struct PlaylistRom {
    pub rom_id: i64,
    /// On disk: the archive, for a file inside one.
    pub path: String,
    pub archive_member: Option<String>,
    pub file_name: String,
    pub label: String,
    /// Only for verified files, whose CRC is the dump's.
    pub crc32: Option<String>,
    pub title_kind: Option<String>,
    pub box_art: Option<String>,
}

#[derive(Serialize)]
struct Playlist<'a> {
    version: &'static str,
    default_core_path: &'a str,
    default_core_name: &'a str,
    label_display_mode: u8,
    right_thumbnail_mode: u8,
    left_thumbnail_mode: u8,
    sort_mode: u8,
    items: Vec<Item<'a>>,
}

#[derive(Serialize)]
struct Item<'a> {
    path: String,
    label: &'a str,
    core_path: &'a str,
    core_name: &'a str,
    crc32: String,
    db_name: &'a str,
}

/// RetroArch's own placeholder, for entries it should work out itself.
const DETECT: &str = "DETECT";

/// The playlist file for `name`, e.g. "Nintendo - Game Boy.lpl".
pub fn file_name(name: &str) -> String {
    format!("{}.lpl", name)
}

/// A file inside an archive is addressed as "archive.zip#member".
pub fn entry_path(rom: &PlaylistRom) -> String {
    match &rom.archive_member {
        Some(member) => format!("{}#{}", rom.path, member),
        None => rom.path.clone(),
    }
}

/// The image name RetroArch looks for, given an entry's label.
pub fn thumbnail_file_name(label: &str) -> String {
    let safe: String = label
        .chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect();
    format!("{}.png", safe)
}

/// The playlist as RetroArch writes it. `core` is (dll path, name), or None to
/// let RetroArch pick a core when the game is started.
pub fn render(name: &str, roms: &[&PlaylistRom], core: Option<(&str, &str)>) -> String {
    let (core_path, core_name) = core.unwrap_or((DETECT, DETECT));
    let db_name = file_name(name);
    let playlist = Playlist {
        version: "1.5",
        default_core_path: core.map_or("", |c| c.0),
        default_core_name: core.map_or("", |c| c.1),
        label_display_mode: 0,
        right_thumbnail_mode: 0,
        left_thumbnail_mode: 0,
        sort_mode: 0,
        items: roms
            .iter()
            .map(|rom| Item {
                path: entry_path(rom),
                label: &rom.label,
                core_path,
                core_name,
                crc32: format!("{}|crc", rom.crc32.as_deref().map_or("00000000".to_string(), str::to_uppercase)),
                db_name: &db_name,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&playlist).expect("playlist serializes")
}

/// Files that don't belong in a playlist of their own: a disc's track files
/// (listed by a cue sheet or .gdi that is itself in the playlist), ECM images
/// (no core reads them), and Wii U-style updates and DLC. `read` returns a
/// cue sheet's text.
pub fn entries_to_skip(roms: &[PlaylistRom], read: impl Fn(&str) -> Option<String>) -> HashSet<i64> {
    let mut skip: HashSet<i64> = roms
        .iter()
        .filter(|r| matches!(r.title_kind.as_deref(), Some("update" | "dlc")) || has_extension(&r.file_name, "ecm"))
        .map(|r| r.rom_id)
        .collect();

    for sheet in roms.iter().filter(|r| r.archive_member.is_none() && (has_extension(&r.file_name, "cue") || has_extension(&r.file_name, "gdi"))) {
        let Some(text) = read(&sheet.path) else { continue };
        let folder = Path::new(&sheet.path).parent();
        let tracks: HashSet<String> = crate::scanner::cue::referenced_files(&sheet.file_name, &text)
            .into_iter()
            .map(|t| t.to_lowercase())
            .collect();
        for rom in roms {
            let same_folder = Path::new(&rom.path).parent() == folder;
            if rom.rom_id != sheet.rom_id && same_folder && tracks.contains(&rom.file_name.to_lowercase()) {
                skip.insert(rom.rom_id);
            }
        }
    }
    skip
}

fn has_extension(name: &str, ext: &str) -> bool {
    Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(id: i64, path: &str, member: Option<&str>, label: &str) -> PlaylistRom {
        let file_name = member.unwrap_or_else(|| path.rsplit('\\').next().unwrap()).to_string();
        PlaylistRom {
            rom_id: id,
            path: path.into(),
            archive_member: member.map(Into::into),
            file_name,
            label: label.into(),
            crc32: None,
            title_kind: None,
            box_art: None,
        }
    }

    #[test]
    fn thumbnail_names_replace_what_retroarch_replaces() {
        assert_eq!(thumbnail_file_name("Mario & Luigi: Superstar Saga (USA)"), "Mario _ Luigi_ Superstar Saga (USA).png");
        assert_eq!(thumbnail_file_name("Legend of Zelda, The - Link's Awakening (USA)"), "Legend of Zelda, The - Link's Awakening (USA).png");
    }

    #[test]
    fn playlist_matches_retroarch_format() {
        let mut zipped = rom(1, r"\\nas\Roms\NES\1942 (Japan, USA) (En).zip", Some("1942.nes"), "1942 (Japan, USA) (En)");
        zipped.crc32 = Some("a9a4ea4c".into());
        let plain = rom(2, r"\\nas\Roms\NES\Goonies, The.nes", None, "Goonies, The");
        let text = render(
            "Nintendo - Nintendo Entertainment System",
            &[&zipped, &plain],
            Some((r"D:\RetroArch\cores\fceumm_libretro.dll", "FCEUmm")),
        );
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["version"], "1.5");
        assert_eq!(json["default_core_name"], "FCEUmm");
        let items = json["items"].as_array().unwrap();
        assert_eq!(items[0]["path"], r"\\nas\Roms\NES\1942 (Japan, USA) (En).zip#1942.nes");
        assert_eq!(items[0]["crc32"], "A9A4EA4C|crc");
        assert_eq!(items[0]["db_name"], "Nintendo - Nintendo Entertainment System.lpl");
        assert_eq!(items[1]["crc32"], "00000000|crc");
        assert_eq!(items[1]["core_path"], r"D:\RetroArch\cores\fceumm_libretro.dll");

        let detect = render("Nintendo - Game Boy", &[&plain], None);
        let json: serde_json::Value = serde_json::from_str(&detect).unwrap();
        assert_eq!(json["items"][0]["core_name"], "DETECT");
        assert_eq!(json["default_core_path"], "");
    }

    #[test]
    fn tracks_ecm_images_and_add_ons_are_skipped() {
        let mut update = rom(6, r"\\nas\Roms\Wii U\Game (Update)", None, "Game (Update)");
        update.title_kind = Some("update".into());
        let roms = vec![
            rom(1, r"\\nas\Roms\PS\FF7 (Disc 1).cue", None, "FF7 (Disc 1)"),
            rom(2, r"\\nas\Roms\PS\FF7 (Disc 1).bin", None, "FF7 (Disc 1)"),
            rom(3, r"\\nas\Roms\PS\FF7 (Disc 1).ecm", None, "FF7 (Disc 1)"),
            rom(4, r"\\nas\Roms\PS\Other.bin", None, "Other"),
            rom(5, r"\\nas\Roms\PS\Elsewhere\FF7 (Disc 1).bin", None, "FF7 (Disc 1)"),
            update,
        ];
        let skip = entries_to_skip(&roms, |path| {
            path.ends_with(".cue").then(|| "FILE \"ff7 (disc 1).BIN\" BINARY\n".to_string())
        });
        let mut skipped: Vec<i64> = skip.into_iter().collect();
        skipped.sort();
        assert_eq!(skipped, vec![2, 3, 6]);
    }
}
