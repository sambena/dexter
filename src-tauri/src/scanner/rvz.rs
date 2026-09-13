//! Decodes GameCube RVZ images (Dolphin's compressed disc format) back into
//! the raw disc, as a stream, so they can be hashed against Redump.
//!
//! RVZ splits the disc into fixed-size groups, each compressed on its own.
//! Before compressing, runs of the pseudo-random "junk" padding that fills
//! unused space on Nintendo discs are replaced by the seed that generates
//! them, which is where most of RVZ's savings come from. Decoding reverses
//! both, so the output is the exact disc Redump dumped.
//!
//! Only GameCube discs with no or Zstandard compression are handled. Wii
//! discs store their partitions decrypted with hash exceptions, which would
//! need re-encrypting to rebuild; those report `ErrorKind::Unsupported`.
//! The format is documented in Dolphin's docs/WiaAndRvz.md.

use sha1::{Digest, Sha1};
use std::io::{self, Read, Seek, SeekFrom};

const HEADER_1_SIZE: usize = 0x48;
const DISC_HEADER_SIZE: usize = 0x80;
const RAW_DATA_ENTRY_SIZE: usize = 0x18;
const GROUP_ENTRY_SIZE: usize = 0x0C;
/// Junk data restarts from its seed at every 0x8000-byte block.
const JUNK_BLOCK_SIZE: u64 = 0x8000;

const DISC_TYPE_GAMECUBE: u32 = 1;
const COMPRESSION_NONE: u32 = 0;
const COMPRESSION_ZSTD: u32 = 5;

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}

fn be64(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().unwrap())
}

fn invalid(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("corrupt RVZ file: {}", what.into()))
}

fn unsupported(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, what.into())
}

/// Dolphin's lagged Fibonacci generator for Nintendo disc junk data.
struct JunkGenerator {
    buffer: [u32; Self::K],
    /// The buffer as output bytes (each word big-endian).
    bytes: [u8; Self::K * 4],
    position: usize,
}

impl JunkGenerator {
    const K: usize = 521;
    const J: usize = 32;
    const SEED_WORDS: usize = 17;

    fn new(seed: &[u8]) -> Self {
        let mut g = JunkGenerator { buffer: [0; Self::K], bytes: [0; Self::K * 4], position: 0 };
        for i in 0..Self::SEED_WORDS {
            g.buffer[i] = be32(seed, i * 4);
        }
        for i in Self::SEED_WORDS..Self::K {
            g.buffer[i] = (g.buffer[i - 17] << 23) ^ (g.buffer[i - 16] >> 9) ^ g.buffer[i - 1];
        }
        // The generator's output skips two bits in its third byte; Dolphin
        // folds that into the buffer once so output is a plain byte copy.
        for x in g.buffer.iter_mut() {
            *x = (*x & 0xFF00_FFFF) | ((*x >> 2) & 0x00FF_0000);
        }
        for _ in 0..4 {
            g.forward();
        }
        g
    }

    fn forward(&mut self) {
        for i in 0..Self::J {
            self.buffer[i] ^= self.buffer[i + Self::K - Self::J];
        }
        for i in Self::J..Self::K {
            self.buffer[i] ^= self.buffer[i - Self::J];
        }
        for (i, word) in self.buffer.iter().enumerate() {
            self.bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
    }

    fn skip(&mut self, count: usize) {
        self.position += count;
        while self.position >= Self::K * 4 {
            self.forward();
            self.position -= Self::K * 4;
        }
    }

    fn fill(&mut self, out: &mut Vec<u8>, mut count: usize) {
        while count > 0 {
            let n = count.min(Self::K * 4 - self.position);
            out.extend_from_slice(&self.bytes[self.position..self.position + n]);
            self.position += n;
            count -= n;
            if self.position == Self::K * 4 {
                self.forward();
                self.position = 0;
            }
        }
    }
}

/// Expands RVZ-packed group data: alternating literal runs and junk runs
/// given by seed. `disc_offset` is where the group starts on the disc.
fn unpack(packed: &[u8], disc_offset: u64, out_len: usize) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(out_len);
    let mut at = 0;
    while out.len() < out_len {
        if at + 4 > packed.len() {
            return Err(invalid("packed data ends early"));
        }
        let header = be32(packed, at);
        at += 4;
        let size = (header & 0x7FFF_FFFF) as usize;
        let size = size.min(out_len - out.len());
        if header & 0x8000_0000 != 0 {
            let seed_len = JunkGenerator::SEED_WORDS * 4;
            let seed = packed.get(at..at + seed_len).ok_or_else(|| invalid("junk seed ends early"))?;
            at += seed_len;
            let mut junk = JunkGenerator::new(seed);
            junk.skip(((disc_offset + out.len() as u64) % JUNK_BLOCK_SIZE) as usize);
            junk.fill(&mut out, size);
        } else {
            let literal = packed.get(at..at + size).ok_or_else(|| invalid("literal data ends early"))?;
            out.extend_from_slice(literal);
            at += size;
        }
    }
    Ok(out)
}

struct Group {
    file_offset: u64,
    stored_size: u32,
    compressed: bool,
    packed_size: u32,
}

pub struct RvzReader<R: Read + Seek> {
    inner: R,
    compression: u32,
    chunk_size: u64,
    iso_size: u64,
    disc_header: [u8; DISC_HEADER_SIZE],
    /// (disc offset, groups) per raw data region, in disc order.
    regions: Vec<(u64, u64, Vec<Group>)>,
    position: u64,
    buffer: Vec<u8>,
    buffer_start: u64,
}

impl<R: Read + Seek> RvzReader<R> {
    pub fn new(mut inner: R) -> io::Result<Self> {
        let mut header_1 = [0u8; HEADER_1_SIZE];
        inner.read_exact(&mut header_1)?;
        if &header_1[..4] != b"RVZ\x01" {
            return Err(invalid("not an RVZ file"));
        }
        let header_2_size = be32(&header_1, 0x0C) as usize;
        if !(0xDC..=0x1000).contains(&header_2_size) {
            return Err(invalid("unexpected header size"));
        }
        let iso_size = be64(&header_1, 0x24);
        let mut header_2 = vec![0u8; header_2_size];
        inner.read_exact(&mut header_2)?;
        if Sha1::digest(&header_2).as_slice() != &header_1[0x10..0x24] {
            return Err(invalid("header checksum doesn't match"));
        }

        let disc_type = be32(&header_2, 0x00);
        let compression = be32(&header_2, 0x04);
        let chunk_size = be32(&header_2, 0x0C) as u64;
        if disc_type != DISC_TYPE_GAMECUBE || be32(&header_2, 0x90) != 0 {
            return Err(unsupported("only GameCube RVZ images can be verified; Wii images store their data decrypted"));
        }
        if compression != COMPRESSION_NONE && compression != COMPRESSION_ZSTD {
            return Err(unsupported(format!("RVZ compression method {} isn't supported, only Zstandard", compression)));
        }
        if chunk_size == 0 || chunk_size > 64 << 20 {
            return Err(invalid("unexpected chunk size"));
        }
        let mut disc_header = [0u8; DISC_HEADER_SIZE];
        disc_header.copy_from_slice(&header_2[0x10..0x90]);

        let raw_count = be32(&header_2, 0xB4) as usize;
        let raw_entries = read_table(
            &mut inner,
            compression,
            be64(&header_2, 0xB8),
            be32(&header_2, 0xC0) as usize,
            raw_count * RAW_DATA_ENTRY_SIZE,
        )?;
        let group_count = be32(&header_2, 0xC4) as usize;
        let group_entries = read_table(
            &mut inner,
            compression,
            be64(&header_2, 0xC8),
            be32(&header_2, 0xD0) as usize,
            group_count * GROUP_ENTRY_SIZE,
        )?;

        let mut regions = Vec::with_capacity(raw_count);
        for i in 0..raw_count {
            let e = &raw_entries[i * RAW_DATA_ENTRY_SIZE..];
            let (offset, size) = (be64(e, 0), be64(e, 8));
            let (first_group, groups) = (be32(e, 16) as usize, be32(e, 20) as usize);
            // Regions are stored from the start of the chunk they begin in.
            let skipped = offset % chunk_size;
            let (offset, size) = (offset - skipped, size + skipped);
            if first_group + groups > group_count || groups as u64 * chunk_size < size {
                return Err(invalid("group table doesn't cover the disc"));
            }
            let groups = (first_group..first_group + groups)
                .map(|g| {
                    let ge = &group_entries[g * GROUP_ENTRY_SIZE..];
                    let data_size = be32(ge, 4);
                    Group {
                        file_offset: (be32(ge, 0) as u64) << 2,
                        stored_size: data_size & 0x7FFF_FFFF,
                        compressed: data_size & 0x8000_0000 != 0,
                        packed_size: be32(ge, 8),
                    }
                })
                .collect();
            regions.push((offset, size, groups));
        }
        regions.sort_by_key(|(offset, _, _)| *offset);

        Ok(RvzReader {
            inner,
            compression,
            chunk_size,
            iso_size,
            disc_header,
            regions,
            position: 0,
            buffer: Vec::new(),
            buffer_start: 0,
        })
    }

    /// Decodes whatever covers `self.position` into `self.buffer`.
    fn load(&mut self) -> io::Result<()> {
        let position = self.position;
        let found = self.regions.iter().enumerate().find(|(_, (offset, size, _))| position >= *offset && position < offset + size);
        let Some((region_index, &(region_offset, region_size, _))) = found else {
            // Not covered by any region: zeros up to the next region or the end.
            let next = self.regions.iter().map(|(o, _, _)| *o).filter(|&o| o > position).min().unwrap_or(self.iso_size);
            self.buffer = vec![0; (next - position).min(self.chunk_size) as usize];
            self.buffer_start = position;
            return Ok(());
        };
        let group_index = ((position - region_offset) / self.chunk_size) as usize;
        let group_start = region_offset + group_index as u64 * self.chunk_size;
        let group_len = self.chunk_size.min(region_offset + region_size - group_start) as usize;
        let group = &self.regions[region_index].2[group_index];

        let data = if group.stored_size == 0 {
            vec![0; group_len]
        } else {
            self.inner.seek(SeekFrom::Start(group.file_offset))?;
            let mut stored = vec![0u8; group.stored_size as usize];
            self.inner.read_exact(&mut stored)?;
            let expanded_len = if group.packed_size != 0 { group.packed_size as usize } else { group_len };
            let expanded = if group.compressed && self.compression == COMPRESSION_ZSTD {
                zstd::bulk::decompress(&stored, expanded_len).map_err(|e| invalid(format!("group {}: {}", group_index, e)))?
            } else {
                stored
            };
            if group.packed_size != 0 {
                unpack(&expanded, group_start, group_len)?
            } else {
                expanded
            }
        };
        if data.len() < group_len {
            return Err(invalid(format!("group {} is short", group_index)));
        }
        self.buffer = data;
        self.buffer.truncate(group_len);
        self.buffer_start = group_start;
        Ok(())
    }
}

/// Reads one of the header's entry tables, which are compressed like the data.
fn read_table<R: Read + Seek>(inner: &mut R, compression: u32, offset: u64, stored_size: usize, size: usize) -> io::Result<Vec<u8>> {
    inner.seek(SeekFrom::Start(offset))?;
    let mut stored = vec![0u8; stored_size];
    inner.read_exact(&mut stored)?;
    let table = if compression == COMPRESSION_ZSTD {
        zstd::bulk::decompress(&stored, size).map_err(|e| invalid(format!("entry table: {}", e)))?
    } else {
        stored
    };
    if table.len() < size {
        return Err(invalid("entry table is short"));
    }
    Ok(table)
}

impl<R: Read + Seek> Read for RvzReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.position >= self.iso_size {
            return Ok(0);
        }
        if self.position < DISC_HEADER_SIZE as u64 {
            let from = self.position as usize;
            let n = out.len().min(DISC_HEADER_SIZE - from);
            out[..n].copy_from_slice(&self.disc_header[from..from + n]);
            self.position += n as u64;
            return Ok(n);
        }
        let buffer_end = self.buffer_start + self.buffer.len() as u64;
        if self.position < self.buffer_start || self.position >= buffer_end {
            self.load()?;
        }
        let from = (self.position - self.buffer_start) as usize;
        let n = out.len().min(self.buffer.len() - from).min((self.iso_size - self.position) as usize);
        out[..n].copy_from_slice(&self.buffer[from..from + n]);
        self.position += n as u64;
        Ok(n)
    }
}

pub fn is_rvz(file_name: &str) -> bool {
    std::path::Path::new(file_name).extension().is_some_and(|e| e.eq_ignore_ascii_case("rvz"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_generator_matches_a_gamecube_disc() {
        // Every GameCube disc's junk starts from a seed derived from its game
        // ID; this checks the generator against itself across a block wrap.
        let seed: Vec<u8> = (0..68u8).collect();
        let mut straight = JunkGenerator::new(&seed);
        let mut all = Vec::new();
        straight.fill(&mut all, 6000);
        let mut skipped = JunkGenerator::new(&seed);
        skipped.skip(4000);
        let mut tail = Vec::new();
        skipped.fill(&mut tail, 2000);
        assert_eq!(&all[4000..], &tail[..]);
    }

    #[test]
    fn packed_literals_and_junk_expand_in_order() {
        let seed = [7u8; 68];
        let mut packed = Vec::new();
        packed.extend_from_slice(&3u32.to_be_bytes());
        packed.extend_from_slice(b"abc");
        packed.extend_from_slice(&(0x8000_0000u32 | 5).to_be_bytes());
        packed.extend_from_slice(&seed);

        let out = unpack(&packed, 0x8000 - 3, 8).unwrap();
        assert_eq!(&out[..3], b"abc");
        // The junk starts on a block boundary, so it's the seed's first bytes.
        let mut expected = Vec::new();
        JunkGenerator::new(&seed).fill(&mut expected, 5);
        assert_eq!(&out[3..], &expected[..]);
    }

    /// Decodes a real RVZ file named by DEXTER_RVZ_FILE and prints its hashes.
    #[test]
    #[ignore]
    fn real_rvz_file() {
        let path = std::env::var("DEXTER_RVZ_FILE").expect("set DEXTER_RVZ_FILE");
        let h = crate::scanner::hashing::hash_file(std::path::Path::new(&path)).unwrap();
        println!("size {} crc32 {} sha1 {}", h.size, h.full.crc32, h.full.sha1);
    }
}
