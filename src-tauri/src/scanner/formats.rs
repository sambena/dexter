use std::path::Path;

/// Formats whose bytes can never equal the dump a DAT describes, so hashing
/// them can't verify anything:
///
/// - Compressed or scrubbed disc images (WIA, GCZ, WBFS, CISO, CSO, CHD).
///   Redump hashes the raw disc, and these store it re-encoded, or with
///   unused space thrown away.
/// - Switch game files (NSP, XCI and their compressed NSZ/XCZ forms). Each
///   dump carries console- or dumper-specific data (tickets, trimmed or
///   untrimmed cartridge padding), so no public DAT can list their hashes.
/// - RAR and 7z archives, which Dexter can't look inside.
///
/// ECM and RVZ aren't here: they're decoded back to the raw image while
/// hashing (scanner::ecm, scanner::rvz). An RVZ variant that can't be decoded
/// is marked unverifiable when hashing finds out.
const UNVERIFIABLE_EXTENSIONS: &[&str] =
    &["wia", "gcz", "wbfs", "ciso", "cso", "chd", "nsp", "xci", "nsz", "xcz", "rar", "7z"];

/// Whether a file (by name) can't be checked against a DAT. Archive members
/// are judged by their own name, so a .rvz inside a .zip still counts.
pub fn is_unverifiable(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| UNVERIFIABLE_EXTENSIONS.iter().any(|u| u.eq_ignore_ascii_case(ext)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_images_switch_files_and_opaque_archives() {
        for name in [
            "Game.gcz",
            "The Legend of Zelda Skyward Sword.wbfs",
            "TOTK 1.2.0.nsp",
            "TOTK [0100F2C0115B6000][v0].xci",
            "The Legend of Zelda Twilight Princess HD [AZAP01].part03.rar",
            "Game.CHD",
        ] {
            assert!(is_unverifiable(name), "{}", name);
        }
    }

    #[test]
    fn hashable_formats() {
        for name in [
            "The Legend of Zelda Skyward Sword.iso",
            "Game (USA).bin",
            "Final Fantasy VII (Europe) (Disc 1).bin.ecm",
            "F-Zero GX (USA).rvz",
            "Mario.zip",
            "Super Mario 64.z64",
            "aoc",
        ] {
            assert!(!is_unverifiable(name), "{}", name);
        }
    }
}
