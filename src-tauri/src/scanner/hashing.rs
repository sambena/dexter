use md5::{Digest, Md5};
use sha1::Sha1;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Digests {
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

pub struct FileHashes {
    pub size: u64,
    /// The whole file, byte for byte.
    pub full: Digests,
    /// Set when the file starts with a header that DATs usually leave out of
    /// their hashes, so both forms can be tried when matching.
    pub headerless: Option<Headerless>,
}

pub struct Headerless {
    pub header_size: u64,
    /// Bytes also left off the end, e.g. a title tag appended by a ROM site.
    pub trailer_size: u64,
    pub digests: Digests,
}

/// Bytes at either end of a file that aren't part of the ROM data DATs hash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    pub header: u64,
    pub trailer: u64,
}

/// Extensions of SNES dumps that may carry a 512-byte copier header.
const SNES_EXTENSIONS: &[&str] = &["smc", "sfc", "swc", "fig"];

/// Bytes read before deciding whether a file has a header. Large enough for
/// every header detected here.
const PEEK_BYTES: usize = 512;

/// NES PRG and CHR data come in whole 8 KiB blocks.
const NES_DATA_UNIT: u64 = 8192;
/// iNES flags 6, bit 2: a 512-byte trainer sits between header and data.
const INES_TRAINER_FLAG: u8 = 0x04;

/// Bytes at the start and end of a ROM that DATs leave out of their hashes.
///
/// - NES (iNES) and Famicom Disk System (fwNES) dumps start with a 16-byte
///   header marked by a magic number. No-Intro's headerless DATs hash the
///   data after it, and its headered DAT expects a clean header that old
///   dumps often don't have (e.g. "DiskDude!" written into the padding).
/// - NES dumps can also end with bytes past the last whole 8 KiB block: some
///   ROM sites append a ~128-byte title tag (e.g. "10 Yard Fight  (Vimm's
///   Lair - http://vimm.net)"). Dumps with a trainer are left untrimmed,
///   since their layout isn't plain header + data.
/// - SNES dumps from copier devices carry a 512-byte header with no magic
///   number; it shows as a file size 512 bytes over a multiple of 1024,
///   which real cartridge data never is.
pub fn detect_framing(file_name: &str, size: Option<u64>, start: &[u8]) -> Option<Framing> {
    if start.len() >= 16 && start.starts_with(b"NES\x1a") {
        let has_trainer = start[6] & INES_TRAINER_FLAG != 0;
        let trailer = match size {
            Some(size) if !has_trainer && size > 16 => (size - 16) % NES_DATA_UNIT,
            _ => 0,
        };
        return Some(Framing { header: 16, trailer });
    }
    if start.len() >= 16 && start.starts_with(b"FDS\x1a") {
        return Some(Framing { header: 16, trailer: 0 });
    }
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let is_snes = ext.as_deref().is_some_and(|e| SNES_EXTENSIONS.contains(&e));
    match size {
        Some(size) if is_snes && size > 512 && size % 1024 == 512 => Some(Framing { header: 512, trailer: 0 }),
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct Hasher {
    crc: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
}

impl Hasher {
    pub(crate) fn new() -> Self {
        Hasher { crc: crc32fast::Hasher::new(), md5: Md5::new(), sha1: Sha1::new() }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        self.crc.update(bytes);
        self.md5.update(bytes);
        self.sha1.update(bytes);
    }

    pub(crate) fn finish(self) -> Digests {
        Digests {
            crc32: format!("{:08x}", self.crc.finalize()),
            md5: format!("{:x}", self.md5.finalize()),
            sha1: format!("{:x}", self.sha1.finalize()),
        }
    }
}

/// Fills as much of `buf` as the reader can supply, stopping only at EOF.
fn read_up_to<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

/// Hashes a ROM in one pass, computing headerless digests alongside the full
/// ones when a header is detected. `size` is the expected length where it's
/// known up front (from file metadata or the zip directory).
pub fn hash_reader<R: Read>(mut reader: R, file_name: &str, size: Option<u64>) -> std::io::Result<FileHashes> {
    let mut buf = vec![0u8; 65536];
    let peeked = read_up_to(&mut reader, &mut buf[..PEEK_BYTES])?;
    let framing = detect_framing(file_name, size, &buf[..peeked]);
    // The body is the byte range [header, size - trailer). A trailer is only
    // ever detected when the size is known.
    let body_end = match (framing, size) {
        (Some(f), Some(size)) => size.saturating_sub(f.trailer),
        _ => u64::MAX,
    };

    let mut full = Hasher::new();
    let mut body = framing.map(|_| Hasher::new());
    let mut position = 0u64;
    let mut chunk_len = peeked;
    while chunk_len > 0 {
        let chunk = &buf[..chunk_len];
        full.update(chunk);
        if let (Some(body), Some(f)) = (body.as_mut(), framing) {
            let end_of_chunk = position + chunk_len as u64;
            let from = f.header.clamp(position, end_of_chunk);
            let to = body_end.clamp(from, end_of_chunk);
            body.update(&chunk[(from - position) as usize..(to - position) as usize]);
        }
        position += chunk_len as u64;
        chunk_len = reader.read(&mut buf)?;
    }

    Ok(FileHashes {
        size: position,
        full: full.finish(),
        headerless: framing
            .zip(body)
            // A file no bigger than its framing has no data to match.
            .filter(|(f, _)| position > f.header + f.trailer)
            .map(|(f, body)| Headerless { header_size: f.header, trailer_size: f.trailer, digests: body.finish() }),
    })
}

pub fn hash_file(path: &Path) -> std::io::Result<FileHashes> {
    let file = std::fs::File::open(path)?;
    let size = file.metadata().ok().map(|m| m.len());
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // An ECM file is hashed as the image it decodes to, whose size isn't
    // known until it's decoded.
    if let Some(decoded) = crate::scanner::ecm::decoded_name(name) {
        let reader = crate::scanner::ecm::EcmReader::new(std::io::BufReader::with_capacity(1 << 20, file));
        return hash_reader(reader, decoded, None);
    }
    hash_reader(file, name, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digests_of(bytes: &[u8]) -> Digests {
        let mut h = Hasher::new();
        h.update(bytes);
        h.finish()
    }

    fn ines(prg: &[u8]) -> Vec<u8> {
        let mut rom = b"NES\x1a\x02\x01\x00DiskDude!".to_vec();
        rom.extend_from_slice(prg);
        rom
    }

    #[test]
    fn headerless_file_has_no_second_digest() {
        let rom = vec![7u8; 4096];
        let h = hash_reader(&rom[..], "Game.sfc", Some(4096)).unwrap();
        assert_eq!(h.full, digests_of(&rom));
        assert!(h.headerless.is_none());
    }

    #[test]
    fn ines_header_is_skipped_regardless_of_extension() {
        let prg: Vec<u8> = (0..40960u32).map(|i| (i % 251) as u8).collect();
        let rom = ines(&prg);
        let h = hash_reader(&rom[..], "1942.bin", Some(rom.len() as u64)).unwrap();
        assert_eq!(h.size, rom.len() as u64);
        assert_eq!(h.full, digests_of(&rom));
        let headerless = h.headerless.unwrap();
        assert_eq!(headerless.header_size, 16);
        assert_eq!(headerless.digests, digests_of(&prg));
    }

    #[test]
    fn snes_copier_header_is_skipped() {
        let mut rom = vec![0xAAu8; 512];
        let data: Vec<u8> = (0..1_048_576u32).map(|i| (i % 13) as u8).collect();
        rom.extend_from_slice(&data);
        let h = hash_reader(&rom[..], "Aerobiz.SMC", Some(rom.len() as u64)).unwrap();
        let headerless = h.headerless.unwrap();
        assert_eq!(headerless.header_size, 512);
        assert_eq!(headerless.digests, digests_of(&data));
    }

    #[test]
    fn copier_header_rule_only_applies_to_snes_extensions() {
        assert_eq!(detect_framing("track.bin", Some(1_049_088), &[0; 16]), None);
        assert_eq!(
            detect_framing("game.fig", Some(1_049_088), &[0; 16]),
            Some(Framing { header: 512, trailer: 0 })
        );
    }

    #[test]
    fn copier_header_needs_a_known_size() {
        assert_eq!(detect_framing("game.smc", None, &[0; 16]), None);
    }

    #[test]
    fn header_spanning_reads_is_skipped_exactly() {
        // A reader that returns one byte at a time exercises the peek loop
        // and a header boundary that falls mid-chunk.
        struct Trickle<'a>(&'a [u8]);
        impl Read for Trickle<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.0.is_empty() || buf.is_empty() {
                    return Ok(0);
                }
                buf[0] = self.0[0];
                self.0 = &self.0[1..];
                Ok(1)
            }
        }
        let prg = vec![3u8; 2000];
        let rom = ines(&prg);
        let h = hash_reader(Trickle(&rom), "x.nes", None).unwrap();
        assert_eq!(h.full, digests_of(&rom));
        assert_eq!(h.headerless.unwrap().digests, digests_of(&prg));
    }

    #[test]
    fn nes_title_trailer_is_left_out() {
        let prg: Vec<u8> = (0..40960u32).map(|i| (i % 7) as u8).collect();
        let mut rom = ines(&prg);
        let mut tag = b"10 Yard Fight  (Vimm's Lair - http://vimm.net)".to_vec();
        tag.resize(128, 0);
        rom.extend_from_slice(&tag);
        let h = hash_reader(&rom[..], "10 Yard Fight.nes", Some(rom.len() as u64)).unwrap();
        assert_eq!(h.full, digests_of(&rom));
        let headerless = h.headerless.unwrap();
        assert_eq!((headerless.header_size, headerless.trailer_size), (16, 128));
        assert_eq!(headerless.digests, digests_of(&prg));
    }

    #[test]
    fn trailer_boundary_mid_chunk_with_trickling_reader() {
        struct Trickle<'a>(&'a [u8]);
        impl Read for Trickle<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.len().min(buf.len()).min(3000);
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0 = &self.0[n..];
                Ok(n)
            }
        }
        let prg = vec![9u8; 16384];
        let mut rom = ines(&prg);
        rom.extend_from_slice(&[b'x'; 127]);
        let h = hash_reader(Trickle(&rom), "a.nes", Some(rom.len() as u64)).unwrap();
        assert_eq!(h.headerless.unwrap().digests, digests_of(&prg));
    }

    #[test]
    fn nes_trainer_or_unknown_size_is_not_trimmed() {
        let mut header = *b"NES\x1a\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(detect_framing("a.nes", None, &header), Some(Framing { header: 16, trailer: 0 }));
        header[6] = INES_TRAINER_FLAG;
        assert_eq!(detect_framing("a.nes", Some(16 + 512 + 40960), &header), Some(Framing { header: 16, trailer: 0 }));
    }

    #[test]
    fn fds_is_never_trimmed() {
        // Disk sides are 65500 bytes, not whole 8 KiB blocks.
        let header = *b"FDS\x1a\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(detect_framing("a.fds", Some(16 + 65500), &header), Some(Framing { header: 16, trailer: 0 }));
    }

    #[test]
    fn file_that_is_only_a_header_has_no_headerless_digest() {
        let rom = b"NES\x1a\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        let h = hash_reader(&rom[..], "empty.nes", Some(16)).unwrap();
        assert!(h.headerless.is_none());
    }
}
