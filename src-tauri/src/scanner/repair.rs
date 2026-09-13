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
//! - Trimmed dumps: the padding at the end of the cartridge (0xFF or 0x00
//!   bytes) was cut off to save space, e.g. a 28 MiB Nintendo 64 file for a
//!   32 MiB cartridge.
//! - Interleaved SNES dumps: some copiers stored a cartridge's two halves
//!   woven together, 32 KiB block by block.
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
    /// End padding cut off.
    Trimmed,
    /// SNES halves woven together.
    Interleaved,
}

impl RepairKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RepairKind::Overdump => "overdump",
            RepairKind::Header => "header",
            RepairKind::Mirrored => "mirrored",
            RepairKind::Trimmed => "trimmed",
            RepairKind::Interleaved => "interleaved",
        }
    }
}

/// Interleaving works in 32 KiB blocks; SNES cartridges are at most 6 MiB.
const INTERLEAVE_BLOCK: usize = 0x8000;
const MAX_INTERLEAVED_BYTES: usize = 8 << 20;

#[derive(Debug, Clone)]
pub struct Repaired {
    pub kind: RepairKind,
    /// Bytes skipped at the start.
    pub header_size: u64,
    /// Bytes of the file left out after the data that matched; for a trimmed
    /// dump, the padding bytes added back instead.
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

        // Trimmed: the body, then padding up to each larger DAT size (at most
        // double, as with overdumps), trying both usual padding bytes.
        let padded_sizes: Vec<u64> =
            sizes.iter().copied().filter(|&s| s > body_len && s <= body_len * MIN_PREFIX_FRACTION).collect();
        if !padded_sizes.is_empty() {
            let mut base = Hasher::new();
            base.update(body);
            for fill in [0xFFu8, 0x00] {
                let mut hasher = base.clone();
                let mut position = body_len;
                let block = [fill; 65536];
                for &size in &padded_sizes {
                    while position < size {
                        let n = (size - position).min(block.len() as u64) as usize;
                        hasher.update(&block[..n]);
                        position += n as u64;
                    }
                    out.push(Repaired {
                        kind: RepairKind::Trimmed,
                        header_size: start,
                        trailer_size: size - body_len,
                        digests: hasher.clone().finish(),
                    });
                }
            }
        }

        if let Some(digests) = uninterleave(body) {
            out.push(Repaired { kind: RepairKind::Interleaved, header_size: start, trailer_size: 0, digests });
        }

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

/// Digests of an interleaved dump put back in order: the stored file's second
/// half holds the even blocks and its first half the odd ones.
fn uninterleave(body: &[u8]) -> Option<Digests> {
    let blocks = body.len() / INTERLEAVE_BLOCK;
    if body.len() > MAX_INTERLEAVED_BYTES || body.len() % (2 * INTERLEAVE_BLOCK) != 0 || blocks < 2 {
        return None;
    }
    let half = blocks / 2;
    let block = |i: usize| &body[i * INTERLEAVE_BLOCK..(i + 1) * INTERLEAVE_BLOCK];
    let mut hasher = Hasher::new();
    for i in 0..half {
        hasher.update(block(half + i));
        hasher.update(block(i));
    }
    Some(hasher.finish())
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

        let found: Vec<_> = candidates(&file, &[1 << 20, 4 << 20, 32 << 20])
            .into_iter()
            .filter(|c| c.kind == RepairKind::Overdump)
            .collect();
        // 1 MiB is under half the file, 32 MiB is past its end.
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

        let found: Vec<_> =
            candidates(&file, &[196608, 262144]).into_iter().filter(|c| c.kind == RepairKind::Overdump).collect();
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
    fn trimmed_padding_is_restored_with_either_fill_byte() {
        let game = pattern(6 << 20, 7);
        let mut cartridge = game.clone();
        cartridge.extend(std::iter::repeat_n(0xFF, 2 << 20));

        let found = candidates(&game, &[8 << 20, 32 << 20]);
        let trimmed: Vec<_> = found.iter().filter(|c| c.kind == RepairKind::Trimmed).collect();
        // One per fill byte; 32 MiB is more than double the file.
        assert_eq!(trimmed.len(), 2);
        assert!(trimmed.iter().any(|c| c.digests == digests_of(&[&cartridge])));
        assert!(trimmed.iter().all(|c| (c.header_size, c.trailer_size) == (0, 2 << 20)));
    }

    #[test]
    fn interleaved_snes_halves_are_put_back_in_order() {
        let block = |i: u32| pattern(0x8000, 100 + i);
        let cartridge: Vec<u8> = (0..8).flat_map(block).collect();
        // Stored as: odd blocks, then even blocks.
        let stored: Vec<u8> = [1, 3, 5, 7, 0, 2, 4, 6].into_iter().flat_map(block).collect();

        let found = candidates(&stored, &[]);
        let fixed = found.iter().find(|c| c.kind == RepairKind::Interleaved).unwrap();
        assert_eq!(fixed.digests, digests_of(&[&cartridge]));
    }

    #[test]
    fn distinct_chips_are_not_halved() {
        let mut file = b"NES\x1a\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        file.extend_from_slice(&pattern(32768 + 8192, 6));
        assert!(unmirror_nes(&file).is_none());
    }
}
