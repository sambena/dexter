//! Cue sheets (and GD-ROM .gdi files) are a few lines of text naming a disc's
//! track files. Redump hashes its own cue text, which differs from any cue
//! written with other file names, so a cue can't be verified by its hash.
//! What can be verified is the tracks it loads: if every one is a matched dump
//! of the same game, the cue sheet is that game.

use std::path::Path;

/// Cue sheets are tiny; a bigger "cue" isn't one.
pub const MAX_CUE_BYTES: u64 = 1024 * 1024;

/// The file names a cue sheet or .gdi loads, in order.
pub fn referenced_files(file_name: &str, text: &str) -> Vec<String> {
    let is_gdi = Path::new(file_name).extension().is_some_and(|e| e.eq_ignore_ascii_case("gdi"));
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if is_gdi {
                // "<track> <lba> <type> <sector size> <file name> <offset>",
                // where the name may be quoted.
                let mut rest = line;
                for _ in 0..4 {
                    rest = rest.trim_start().split_once(char::is_whitespace)?.1;
                }
                parse_name(rest.trim_start())
            } else {
                let rest = line.strip_prefix("FILE").or_else(|| line.strip_prefix("file"))?;
                if !rest.starts_with(char::is_whitespace) {
                    return None;
                }
                parse_name(rest.trim_start())
            }
        })
        .collect()
}

/// A quoted name, or the first whitespace-separated word.
fn parse_name(s: &str) -> Option<String> {
    let name = match s.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"')?.0,
        None => s.split_whitespace().next()?,
    };
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_files_quoted_and_bare() {
        let cue = "FILE \"Final Fantasy V (USA) (v1.1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\nFILE track2.bin BINARY\n  TRACK 02 AUDIO\n";
        assert_eq!(referenced_files("FF5.cue", cue), vec!["Final Fantasy V (USA) (v1.1).bin", "track2.bin"]);
    }

    #[test]
    fn gdi_tracks() {
        let gdi = "3\n1 0 4 2352 track01.bin 0\n2 756 0 2352 \"track 02.raw\" 0\n3 45000 4 2352 track03.bin 0\n";
        assert_eq!(referenced_files("Game.GDI", gdi), vec!["track01.bin", "track 02.raw", "track03.bin"]);
    }
}
