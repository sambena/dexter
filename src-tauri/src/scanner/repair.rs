//! Old cartridge dumps (GoodTools-era sets especially) that contain a DAT's
//! exact ROM data inside a differently shaped file. Hashing the file as-is,
//! or minus its header, can't match these, but the data inside is intact:
//!
//! - Overdumps: the cartridge was read past the end of its ROM, so the real
//!   data is followed by padding or a repeat, e.g. an 8 MiB Nintendo 64 file
//!   holding a 4 MiB game.
//! - Stray copier headers: 512 bytes in front of a Game Boy ROM, a rule the
//!   normal SNES-only header detection doesn't apply elsewhere.
//! - Mirrored NES chips: PRG or CHR stored twice over, with the iNES header
//!   describing the doubled size.
//!
//! Only unmatched files are tried, and only against sizes the system's DATs
//! actually list, so a candidate is never invented out of thin air.

use crate::scanner::hashing::{Digests, Hasher};

/// Cartridge ROMs are at most 64 MiB (Nintendo 64). Anything bigger is a
/// disc image, which these repairs don't apply to, and too big to load whole.
pub const MAX_REPAIR_BYTES: u64 = 64 * 1024 * 1024;

/// Overdumps are at most double the real data. Requiring the prefix to be at
/// least half the file keeps a multi-game cartridge from matching as
/// whichever small game happens to come first.
const MIN_PREFIX_FRACTION: u64 = 2;

const INES_TRAINER_FLAG: u8 = 0x04;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RepairKind {
    /// Real data followed by extra bytes.
    Overdump,
    /// A 512-byte header on a system whose DATs don't expect one.
    Header,
    /// NES PRG/CHR chips stored doubled.
    Mirrored,
}

impl RepairKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RepairKind::Overdump => "overdump",
            RepairKind::Header => "header",
            RepairKind::Mirrored => "mirrored",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Repaired {
    pub kind: RepairKind,
    /// Bytes skipped at the start.
    pub header_size: u64,
    /// Bytes of the file left out after the data that matched.
    pub trailer_size: u64,
    pub digests: Digests,
}

fn is_ines(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && bytes.starts_with(b"NES\x1a")
}

/// Every alternative reading of `bytes` worth looking up, given the distinct
/// ROM sizes in the system's DATs.
pub fn candidates(bytes: &[u8], dat_sizes: &[u64]) -> Vec<Repaired> {
    let mut sizes: Vec<u64> = dat_sizes.iter().copied().filter(|&s| s > 0).collect();
    sizes.sort_unstable();
    sizes.dedup();

    let len = bytes.len() as u64;
    let starts: Vec<u64> = if is_ines(bytes) {
        if bytes[6] & INES_TRAINER_FLAG != 0 { vec![] } else { vec![16] }
    } else if len > 512 && len % 1024 == 512 {
        vec![0, 512]
    } else {
        vec![0]
    };

    let mut out = Vec::new();
    for start in starts {
        let body = &bytes[start as usize..];
        let body_len = body.len() as u64;
        let wanted: Vec<u64> = sizes
            .iter()
            .copied()
            .filter(|&s| s <= body_len && s * MIN_PREFIX_FRACTION >= body_len)
            // The file itself (or itself minus a header the normal pass
            // already strips) was hashed by the normal pass.
            .filter(|&s| !(s == body_len && (start == 0 || is_ines(bytes))))
            .collect();

        // One pass over the body, finishing a copy of the hash state at each
        // candidate size, so a 32 MiB file with a dozen candidate sizes is
        // still read once.
        let mut hasher = Hasher::new();
        let mut position = 0u64;
        for size in wanted {
            hasher.update(&body[position as usize..size as usize]);
            position = size;
            let kind = if size == body_len { RepairKind::Header } else { RepairKind::Overdump };
            out.push(Repaired { kind, header_size: start, trailer_size: body_len - size, digests: hasher.clone().finish() });
        }
    }

    if let Some(mirrored) = unmirror_nes(bytes) {
        out.push(mirrored);
    }
    out
}

/// Halves `data` for as long as its two halves are identical.
fn unmirror(mut data: &[u8]) -> &[u8] {
    while data.len() >= 2 && data.len() % 2 == 0 && data[..data.len() / 2] == data[data.len() / 2..] {
        data = &data[..data.len() / 2];
    }
    data
}

fn unmirror_nes(bytes: &[u8]) -> Option<Repaired> {
    if !is_ines(bytes) || bytes[6] & INES_TRAINER_FLAG != 0 {
        return None;
    }
    let prg = bytes[4] as usize * 16384;
    let chr = bytes[5] as usize * 8192;
    if bytes.len() < 16 + prg + chr {
        return None;
    }
    let (prg_data, chr_data) = (&bytes[16..16 + prg], &bytes[16 + prg..16 + prg + chr]);
    let (prg_half, chr_half) = (unmirror(prg_data), unmirror(chr_data));
    if prg_half.len() == prg && chr_half.len() == chr {
        return None;
    }
    let mut hasher = Hasher::new();
    hasher.update(prg_half);
    hasher.update(chr_half);
    let kept = (prg_half.len() + chr_half.len()) as u64;
    Some(Repaired {
        kind: RepairKind::Mirrored,
        header_size: 16,
        trailer_size: bytes.len() as u64 - 16 - kept,
        digests: hasher.finish(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digests_of(parts: &[&[u8]]) -> Digests {
        let mut h = Hasher::new();
        for p in parts {
            h.update(p);
        }
        h.finish()
    }

    fn pattern(len: usize, seed: u32) -> Vec<u8> {
        (0..len as u32).map(|i| (i.wrapping_mul(2654435761).wrapping_add(seed) >> 13) as u8).collect()
    }

    #[test]
    fn overdump_prefix_is_offered_at_dat_sizes_only() {
        let game = pattern(4 << 20, 1);
        let mut file = game.clone();
        file.extend(std::iter::repeat_n(0xFF, 4 << 20));

        let found = candidates(&file, &[1 << 20, 4 << 20, 12 << 20]);
        // 1 MiB is under half the file, 12 MiB is past its end.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, RepairKind::Overdump);
        assert_eq!((found[0].header_size, found[0].trailer_size), (0, 4 << 20));
        assert_eq!(found[0].digests, digests_of(&[&game]));
    }

    #[test]
    fn stray_512_byte_header_is_stripped_on_any_system() {
        let game = pattern(524288, 2);
        let mut file = vec![0u8; 512];
        file.extend_from_slice(&game);

        let found = candidates(&file, &[524288]);
        let header = found.iter().find(|c| c.kind == RepairKind::Header).unwrap();
        assert_eq!((header.header_size, header.trailer_size), (512, 0));
        assert_eq!(header.digests, digests_of(&[&game]));
    }

    #[test]
    fn nes_overdump_skips_the_ines_header() {
        let game = pattern(196608, 3);
        let mut file = b"NES\x1a\x10\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        file.extend_from_slice(&game);
        file.extend(vec![0u8; 65536]);

        let found = candidates(&file, &[196608, 262144]);
        assert_eq!(found.len(), 1, "the whole body is the normal pass's job");
        assert_eq!((found[0].header_size, found[0].trailer_size), (16, 65536));
        assert_eq!(found[0].digests, digests_of(&[&game]));
    }

    #[test]
    fn mirrored_nes_chips_are_halved() {
        let (prg, chr) = (pattern(16384, 4), pattern(8192, 5));
        // Header claims 32 KiB PRG and 16 KiB CHR, each stored twice.
        let mut file = b"NES\x1a\x02\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        file.extend_from_slice(&prg);
        file.extend_from_slice(&prg);
        file.extend_from_slice(&chr);
        file.extend_from_slice(&chr);

        let mirrored = candidates(&file, &[]).into_iter().find(|c| c.kind == RepairKind::Mirrored).unwrap();
        assert_eq!(mirrored.digests, digests_of(&[&prg, &chr]));
        assert_eq!((mirrored.header_size, mirrored.trailer_size), (16, 24576));
    }

    #[test]
    fn distinct_chips_are_not_halved() {
        let mut file = b"NES\x1a\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        file.extend_from_slice(&pattern(32768 + 8192, 6));
        assert!(unmirror_nes(&file).is_none());
    }
}
