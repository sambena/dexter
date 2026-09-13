//! Knowledge about emulators: which platforms a system folder is, which
//! RetroArch cores and standalone emulators run it, and how to launch them.

pub mod detect;
pub mod playlist;
pub mod retroarch;

/// A system's platform names as libretro and No-Intro/Redump spell them,
/// e.g. "Nintendo - Game Boy". RetroArch core info files list the platforms
/// each core supports under these names.
///
/// DAT names are the best source, since they come from the same naming
/// scheme; the variant suffixes No-Intro adds ("(Headered)", "(BigEndian)",
/// "(Digital) (CDN)") are dropped. The folder name is the fallback for
/// systems with no DAT imported, like Switch.
pub fn platform_names(folder_name: &str, dat_names: &[String]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut push = |name: String| {
        let name = PLATFORM_ALIASES
            .iter()
            .find(|(alias, _)| alias.eq_ignore_ascii_case(&name))
            .map_or(name, |(_, canonical)| canonical.to_string());
        if !names.contains(&name) {
            names.push(name);
        }
    };
    for dat in dat_names {
        push(strip_variant_suffixes(dat));
    }
    let key: String = folder_name.to_lowercase().chars().filter(char::is_ascii_alphanumeric).collect();
    if let Some((_, platform)) = FOLDER_PLATFORMS.iter().find(|(k, _)| *k == key) {
        push(platform.to_string());
    }
    names
}

fn strip_variant_suffixes(dat_name: &str) -> String {
    let mut name = dat_name.trim();
    while let Some(open) = name.rfind(" (") {
        if !name.ends_with(')') {
            break;
        }
        name = name[..open].trim_end();
    }
    name.to_string()
}

/// DAT platform names that libretro spells differently.
const PLATFORM_ALIASES: &[(&str, &str)] = &[("Atari - Atari 2600", "Atari - 2600")];

/// Common system folder names, lowercased with punctuation removed.
const FOLDER_PLATFORMS: &[(&str, &str)] = &[
    ("gb", "Nintendo - Game Boy"),
    ("gameboy", "Nintendo - Game Boy"),
    ("gbc", "Nintendo - Game Boy Color"),
    ("gameboycolor", "Nintendo - Game Boy Color"),
    ("gba", "Nintendo - Game Boy Advance"),
    ("gameboyadvance", "Nintendo - Game Boy Advance"),
    ("nes", "Nintendo - Nintendo Entertainment System"),
    ("famicom", "Nintendo - Nintendo Entertainment System"),
    ("snes", "Nintendo - Super Nintendo Entertainment System"),
    ("superfamicom", "Nintendo - Super Nintendo Entertainment System"),
    ("n64", "Nintendo - Nintendo 64"),
    ("nintendo64", "Nintendo - Nintendo 64"),
    ("nds", "Nintendo - Nintendo DS"),
    ("nintendods", "Nintendo - Nintendo DS"),
    ("3ds", "Nintendo - Nintendo 3DS"),
    ("nintendo3ds", "Nintendo - Nintendo 3DS"),
    ("gamecube", "Nintendo - GameCube"),
    ("gc", "Nintendo - GameCube"),
    ("ngc", "Nintendo - GameCube"),
    ("wii", "Nintendo - Wii"),
    ("wiiu", "Nintendo - Wii U"),
    ("switch", "Nintendo - Switch"),
    ("nintendoswitch", "Nintendo - Switch"),
    ("psx", "Sony - PlayStation"),
    ("ps1", "Sony - PlayStation"),
    ("playstation", "Sony - PlayStation"),
    ("ps2", "Sony - PlayStation 2"),
    ("playstation2", "Sony - PlayStation 2"),
    ("atari2600", "Atari - 2600"),
    ("2600", "Atari - 2600"),
];

/// Preferred RetroArch cores per platform, best first: accurate and
/// actively maintained, with a lighter fallback.
pub const RECOMMENDED_CORES: &[(&str, &[&str])] = &[
    ("Nintendo - Game Boy", &["gambatte", "sameboy", "mgba"]),
    ("Nintendo - Game Boy Color", &["gambatte", "sameboy", "mgba"]),
    ("Nintendo - Game Boy Advance", &["mgba", "vbam", "gpsp"]),
    ("Nintendo - Nintendo Entertainment System", &["mesen", "nestopia", "fceumm"]),
    ("Nintendo - Super Nintendo Entertainment System", &["snes9x", "bsnes", "mesen-s"]),
    ("Nintendo - Nintendo 64", &["mupen64plus_next", "parallel_n64"]),
    ("Nintendo - Nintendo DS", &["melondsds", "melonds", "desmume"]),
    ("Nintendo - Nintendo 3DS", &["azahar", "citra"]),
    ("Sony - PlayStation", &["swanstation", "mednafen_psx_hw", "pcsx_rearmed"]),
    ("Sony - PlayStation 2", &["pcsx2"]),
    ("Nintendo - GameCube", &["dolphin"]),
    ("Nintendo - Wii", &["dolphin"]),
    ("Atari - 2600", &["stella"]),
];

/// Platforms where a standalone emulator, when installed, runs better than
/// RetroArch's port of it.
pub const PREFER_STANDALONE: &[&str] = &[
    "Nintendo - GameCube",
    "Nintendo - Wii",
    "Nintendo - Wii U",
    "Nintendo - Switch",
    "Nintendo - Nintendo 3DS",
    "Sony - PlayStation 2",
];

pub struct KnownEmulator {
    pub id: &'static str,
    pub name: &'static str,
    /// Lowercased executable names; a trailing '*' matches any suffix, for
    /// builds named like "duckstation-qt-x64-ReleaseLTCG.exe".
    pub executables: &'static [&'static str],
    pub platforms: &'static [&'static str],
    /// Arguments in the same form as a system's emulator_args.
    pub args: &'static str,
}

pub const STANDALONE_EMULATORS: &[KnownEmulator] = &[
    KnownEmulator { id: "cemu", name: "Cemu", executables: &["cemu.exe"], platforms: &["Nintendo - Wii U"], args: "-g %ROM%" },
    KnownEmulator { id: "ryujinx", name: "Ryujinx", executables: &["ryujinx.exe"], platforms: &["Nintendo - Switch"], args: "%ROM%" },
    KnownEmulator { id: "yuzu", name: "yuzu", executables: &["yuzu.exe"], platforms: &["Nintendo - Switch"], args: "-g %ROM%" },
    KnownEmulator { id: "suyu", name: "suyu", executables: &["suyu.exe"], platforms: &["Nintendo - Switch"], args: "-g %ROM%" },
    KnownEmulator {
        id: "dolphin",
        name: "Dolphin",
        executables: &["dolphin.exe"],
        platforms: &["Nintendo - GameCube", "Nintendo - Wii"],
        args: "-e %ROM%",
    },
    KnownEmulator { id: "pcsx2", name: "PCSX2", executables: &["pcsx2-qt.exe", "pcsx2.exe"], platforms: &["Sony - PlayStation 2"], args: "%ROM%" },
    KnownEmulator { id: "duckstation", name: "DuckStation", executables: &["duckstation-qt*"], platforms: &["Sony - PlayStation"], args: "%ROM%" },
    KnownEmulator { id: "azahar", name: "Azahar", executables: &["azahar.exe"], platforms: &["Nintendo - Nintendo 3DS"], args: "%ROM%" },
    KnownEmulator { id: "lime3ds", name: "Lime3DS", executables: &["lime3ds-gui.exe", "lime3ds.exe"], platforms: &["Nintendo - Nintendo 3DS"], args: "%ROM%" },
    KnownEmulator { id: "citra", name: "Citra", executables: &["citra-qt.exe"], platforms: &["Nintendo - Nintendo 3DS"], args: "%ROM%" },
    KnownEmulator { id: "melonds", name: "melonDS", executables: &["melonds.exe"], platforms: &["Nintendo - Nintendo DS"], args: "%ROM%" },
    KnownEmulator {
        id: "mgba",
        name: "mGBA",
        executables: &["mgba.exe"],
        platforms: &["Nintendo - Game Boy", "Nintendo - Game Boy Color", "Nintendo - Game Boy Advance"],
        args: "%ROM%",
    },
    KnownEmulator { id: "project64", name: "Project64", executables: &["project64.exe"], platforms: &["Nintendo - Nintendo 64"], args: "%ROM%" },
    KnownEmulator {
        id: "mesen",
        name: "Mesen",
        executables: &["mesen.exe"],
        platforms: &[
            "Nintendo - Nintendo Entertainment System",
            "Nintendo - Super Nintendo Entertainment System",
            "Nintendo - Game Boy",
            "Nintendo - Game Boy Color",
        ],
        args: "%ROM%",
    },
    KnownEmulator {
        id: "snes9x",
        name: "Snes9x",
        executables: &["snes9x*"],
        platforms: &["Nintendo - Super Nintendo Entertainment System"],
        args: "%ROM%",
    },
    KnownEmulator { id: "stella", name: "Stella", executables: &["stella.exe"], platforms: &["Atari - 2600"], args: "%ROM%" },
];

pub fn known_emulator(id: &str) -> Option<&'static KnownEmulator> {
    STANDALONE_EMULATORS.iter().find(|e| e.id == id)
}

pub fn executable_matches(pattern: &str, file_name: &str) -> bool {
    let file_name = file_name.to_ascii_lowercase();
    match pattern.strip_suffix('*') {
        Some(prefix) => file_name.starts_with(prefix) && file_name.ends_with(".exe"),
        None => file_name == pattern,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dats(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn dat_variants_collapse_to_the_platform() {
        assert_eq!(
            platform_names(
                "NES",
                &dats(&[
                    "Nintendo - Nintendo Entertainment System (Headered)",
                    "Nintendo - Nintendo Entertainment System (Headerless)"
                ])
            ),
            vec!["Nintendo - Nintendo Entertainment System"]
        );
        assert_eq!(platform_names("Wii U", &dats(&["Nintendo - Wii U (Digital) (CDN)"])), vec!["Nintendo - Wii U"]);
    }

    #[test]
    fn libretro_spelling_is_used() {
        assert_eq!(platform_names("atari2600", &dats(&["Atari - Atari 2600"])), vec!["Atari - 2600"]);
    }

    #[test]
    fn folder_name_covers_systems_without_a_dat() {
        assert_eq!(platform_names("Switch", &[]), vec!["Nintendo - Switch"]);
        assert_eq!(platform_names("PlayStation 2", &[]), vec!["Sony - PlayStation 2"]);
        assert!(platform_names("My Homebrew", &[]).is_empty());
    }

    #[test]
    fn executable_patterns() {
        assert!(executable_matches("cemu.exe", "Cemu.exe"));
        assert!(executable_matches("duckstation-qt*", "duckstation-qt-x64-ReleaseLTCG.exe"));
        assert!(!executable_matches("duckstation-qt*", "duckstation-qt.pdb"));
        assert!(!executable_matches("yuzu.exe", "yuzu-cmd.exe"));
    }
}
