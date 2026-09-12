use std::path::Path;

/// Formats whose bytes can never equal the dump a DAT describes, so hashing
/// them can't verify anything:
///
/// - Compressed or scrubbed disc images (RVZ, WIA, GCZ, WBFS, CISO, CSO,
///   CHD). Redump hashes the raw disc, and these store it re-encoded.
/// - ECM, which strips error-correction data from a raw CD image.
/// - RAR and 7z archives, which Dexter can't look inside.
const UNVERIFIABLE_EXTENSIONS: &[&str] = &["rvz", "wia", "gcz", "wbfs", "ciso", "cso", "chd", "ecm", "rar", "7z"];

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
    fn disc_images_and_opaque_archives() {
        for name in [
            "F-Zero GX (USA).rvz",
            "The Legend of Zelda Skyward Sword.wbfs",
            "Final Fantasy VII (Europe) (Disc 1).bin.ecm",
            "The Legend of Zelda Twilight Princess HD [AZAP01].part03.rar",
            "Game.CHD",
        ] {
            assert!(is_unverifiable(name), "{}", name);
        }
    }

    #[test]
    fn hashable_formats() {
        for name in ["The Legend of Zelda Skyward Sword.iso", "Game (USA).bin", "Mario.zip", "Super Mario 64.z64", "aoc"] {
            assert!(!is_unverifiable(name), "{}", name);
        }
    }
}
