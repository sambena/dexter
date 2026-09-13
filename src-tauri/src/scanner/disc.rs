//! Reads the header every GameCube and Wii disc starts with: a six-character
//! game ID ("SOUE01": game SOU, region E, maker 01), the disc revision and the
//! game's title. Containers that can't be hashed against Redump (WBFS) or
//! copies that don't match still name themselves this way, so they can be
//! identified by title (see scanner::wiiu::identify).

use crate::scanner::wiiu::{TitleInfo, TitleKind};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const WII_MAGIC: u32 = 0x5D1C_9EA3;
const GAMECUBE_MAGIC: u32 = 0xC233_9F3D;
const HEADER_LEN: usize = 0x60;

/// Where the disc header sits inside each container format.
fn header_offset(file_name: &str) -> Option<u64> {
    let ext = Path::new(file_name).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "iso" | "gcm" => Some(0),
        // WBFS: a 0x200-byte container header, then the disc header.
        "wbfs" => Some(0x200),
        // GameCube CISO: a 0x8000-byte block map, then the disc.
        "ciso" => Some(0x8000),
        // RVZ keeps a copy in its second header (scanner::rvz).
        "rvz" => Some(0x48 + 0x10),
        _ => None,
    }
}

pub fn is_disc_image(file_name: &str) -> bool {
    header_offset(file_name).is_some()
}

pub fn parse_disc_header(header: &[u8]) -> Option<TitleInfo> {
    if header.len() < HEADER_LEN {
        return None;
    }
    let magic = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap());
    if magic(0x18) != WII_MAGIC && magic(0x1C) != GAMECUBE_MAGIC {
        return None;
    }
    let id = std::str::from_utf8(&header[..6]).ok()?;
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let title_bytes = &header[0x20..HEADER_LEN];
    let end = title_bytes.iter().position(|&b| b == 0).unwrap_or(title_bytes.len());
    let name = String::from_utf8_lossy(&title_bytes[..end]).split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    Some(TitleInfo {
        title_id: id.to_string(),
        version: header[7] as u32,
        kind: TitleKind::Game,
        name,
        product_code: Some(id.to_string()),
    })
}

pub fn read_disc_title(path: &Path) -> Option<TitleInfo> {
    let offset = header_offset(path.file_name()?.to_str()?)?;
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut header = [0u8; HEADER_LEN];
    file.read_exact(&mut header).ok()?;
    parse_disc_header(&header)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(id: &[u8; 6], revision: u8, title: &str, wii: bool) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_LEN];
        h[..6].copy_from_slice(id);
        h[7] = revision;
        if wii {
            h[0x18..0x1C].copy_from_slice(&WII_MAGIC.to_be_bytes());
        } else {
            h[0x1C..0x20].copy_from_slice(&GAMECUBE_MAGIC.to_be_bytes());
        }
        h[0x20..0x20 + title.len()].copy_from_slice(title.as_bytes());
        h
    }

    #[test]
    fn wii_and_gamecube_headers_name_the_game() {
        let info = parse_disc_header(&header(b"SOUE01", 0, "The Legend of Zelda Skyward Sword", true)).unwrap();
        assert_eq!((info.title_id.as_str(), info.name.as_str(), info.version), ("SOUE01", "The Legend of Zelda Skyward Sword", 0));
        assert_eq!(info.region(), Some("USA"));

        let gc = parse_disc_header(&header(b"GALP01", 2, "Super Smash Bros. Melee", false)).unwrap();
        assert_eq!((gc.version, gc.region()), (2, Some("Europe")));
    }

    #[test]
    fn other_data_is_not_a_disc() {
        assert!(parse_disc_header(&[0u8; HEADER_LEN]).is_none());
        assert!(!is_disc_image("Pokemon X.3ds") && is_disc_image("Game.WBFS"));
    }
}
