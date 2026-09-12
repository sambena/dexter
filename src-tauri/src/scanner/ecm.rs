//! Decodes ECM files (Neill Corlett's "Error Code Modeler") back into the raw
//! CD image they were made from, as a stream, so a `.bin.ecm` can be hashed
//! against Redump without writing the 700 MB image anywhere.
//!
//! ECM removes the sync pattern, header and error-correction bytes of each
//! sector, since they can be recomputed from the data. Decoding recomputes
//! them. The file ends with an EDC checksum of the whole decoded image, which
//! is checked, so a corrupt ECM file is an error rather than a wrong hash.

use std::io::{self, Read};

const SECTOR: usize = 2352;

struct Tables {
    ecc_f: [u8; 256],
    ecc_b: [u8; 256],
    edc: [u32; 256],
}

fn tables() -> &'static Tables {
    static TABLES: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        let mut t = Tables { ecc_f: [0; 256], ecc_b: [0; 256], edc: [0; 256] };
        for i in 0..256u32 {
            let j = (i << 1) ^ if i & 0x80 != 0 { 0x11D } else { 0 };
            t.ecc_f[i as usize] = j as u8;
            t.ecc_b[(i ^ j) as usize & 0xFF] = i as u8;
            let mut edc = i;
            for _ in 0..8 {
                edc = (edc >> 1) ^ if edc & 1 != 0 { 0xD801_8001 } else { 0 };
            }
            t.edc[i as usize] = edc;
        }
        t
    })
}

fn edc_update(mut edc: u32, bytes: &[u8]) -> u32 {
    let t = tables();
    for &b in bytes {
        edc = (edc >> 8) ^ t.edc[((edc ^ b as u32) & 0xFF) as usize];
    }
    edc
}

fn write_edc(sector: &mut [u8], from: usize, len: usize, at: usize) {
    let edc = edc_update(0, &sector[from..from + len]);
    sector[at..at + 4].copy_from_slice(&edc.to_le_bytes());
}

/// One Reed-Solomon product code parity block (P or Q) over sector[0xC..].
fn ecc_block(sector: &mut [u8], major_count: usize, minor_count: usize, major_mult: usize, minor_inc: usize, dest: usize) {
    let t = tables();
    let size = major_count * minor_count;
    for major in 0..major_count {
        let mut index = (major >> 1) * major_mult + (major & 1);
        let (mut ecc_a, mut ecc_b) = (0u8, 0u8);
        for _ in 0..minor_count {
            let temp = sector[0xC + index];
            index += minor_inc;
            if index >= size {
                index -= size;
            }
            ecc_a ^= temp;
            ecc_b ^= temp;
            ecc_a = t.ecc_f[ecc_a as usize];
        }
        ecc_a = t.ecc_b[(t.ecc_f[ecc_a as usize] ^ ecc_b) as usize];
        sector[dest + major] = ecc_a;
        sector[dest + major + major_count] = ecc_a ^ ecc_b;
    }
}

fn write_ecc(sector: &mut [u8], zero_address: bool) {
    let address: [u8; 4] = sector[12..16].try_into().unwrap();
    if zero_address {
        sector[12..16].fill(0);
    }
    ecc_block(sector, 86, 24, 2, 86, 0x81C);
    ecc_block(sector, 52, 43, 86, 88, 0x8C8);
    sector[12..16].copy_from_slice(&address);
}

enum Block {
    /// Bytes copied through as-is.
    Raw(u64),
    /// Sectors of type 1 (Mode 1), 2 (Mode 2 Form 1) or 3 (Mode 2 Form 2).
    Sectors(u8, u64),
    End,
}

pub struct EcmReader<R: Read> {
    inner: R,
    started: bool,
    block: Option<Block>,
    out: Vec<u8>,
    out_pos: usize,
    edc: u32,
    done: bool,
}

impl<R: Read> EcmReader<R> {
    pub fn new(inner: R) -> Self {
        EcmReader { inner, started: false, block: None, out: Vec::with_capacity(SECTOR), out_pos: 0, edc: 0, done: false }
    }

    fn byte(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        self.inner.read_exact(&mut b).map_err(truncated)?;
        Ok(b[0])
    }

    fn next_block(&mut self) -> io::Result<Block> {
        let mut c = self.byte()?;
        let kind = c & 3;
        let mut count = ((c >> 2) & 0x1F) as u64;
        let mut bits = 5;
        while c & 0x80 != 0 {
            c = self.byte()?;
            count |= ((c & 0x7F) as u64) << bits;
            bits += 7;
            if bits > 40 {
                return Err(corrupt("block count is too long"));
            }
        }
        if count as u32 == u32::MAX {
            return Ok(Block::End);
        }
        let count = (count as u32 as u64) + 1;
        if count >= 0x8000_0000 {
            return Err(corrupt("block count is out of range"));
        }
        Ok(if kind == 0 { Block::Raw(count) } else { Block::Sectors(kind, count) })
    }

    /// Decodes the next chunk of output into `self.out`. Returns false at the end.
    fn fill(&mut self) -> io::Result<bool> {
        if !self.started {
            let mut magic = [0u8; 4];
            self.inner.read_exact(&mut magic).map_err(truncated)?;
            if &magic != b"ECM\0" {
                return Err(corrupt("not an ECM file"));
            }
            self.started = true;
        }
        loop {
            let block = match self.block.take() {
                Some(block) => block,
                None => self.next_block()?,
            };
            self.out.clear();
            self.out_pos = 0;
            match block {
                Block::End => {
                    let mut stored = [0u8; 4];
                    self.inner.read_exact(&mut stored).map_err(truncated)?;
                    if u32::from_le_bytes(stored) != self.edc {
                        return Err(corrupt("checksum of the decoded image doesn't match"));
                    }
                    return Ok(false);
                }
                Block::Raw(0) | Block::Sectors(_, 0) => continue,
                Block::Raw(remaining) => {
                    let n = remaining.min(SECTOR as u64) as usize;
                    self.out.resize(n, 0);
                    self.inner.read_exact(&mut self.out).map_err(truncated)?;
                    self.block = Some(Block::Raw(remaining - n as u64));
                }
                Block::Sectors(kind, remaining) => {
                    self.decode_sector(kind)?;
                    self.block = Some(Block::Sectors(kind, remaining - 1));
                }
            }
            self.edc = edc_update(self.edc, &self.out);
            return Ok(true);
        }
    }

    fn decode_sector(&mut self, kind: u8) -> io::Result<()> {
        let mut s = [0u8; SECTOR];
        s[1..11].fill(0xFF);
        match kind {
            1 => {
                s[0x0F] = 0x01;
                self.inner.read_exact(&mut s[0x0C..0x0F]).map_err(truncated)?;
                self.inner.read_exact(&mut s[0x10..0x810]).map_err(truncated)?;
                write_edc(&mut s, 0, 0x810, 0x810);
                write_ecc(&mut s, false);
                self.out.extend_from_slice(&s);
            }
            2 | 3 => {
                s[0x0F] = 0x02;
                // Stored: the subheader's second copy, then the user data up
                // to where the EDC goes (Form 1 also has ECC after it).
                let end = if kind == 2 { 0x818 } else { 0x92C };
                self.inner.read_exact(&mut s[0x14..end]).map_err(truncated)?;
                s.copy_within(0x14..0x18, 0x10);
                if kind == 2 {
                    write_edc(&mut s, 0x10, 0x808, 0x818);
                    write_ecc(&mut s, true);
                } else {
                    write_edc(&mut s, 0x10, 0x91C, 0x92C);
                }
                // Mode 2 sectors are stored without their sync and header.
                self.out.extend_from_slice(&s[0x10..]);
            }
            _ => unreachable!("block kind is two bits and 0 is raw"),
        }
        Ok(())
    }
}

impl<R: Read> Read for EcmReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.out_pos == self.out.len() {
            if self.done || !self.fill()? {
                self.done = true;
                return Ok(0);
            }
        }
        let n = buf.len().min(self.out.len() - self.out_pos);
        buf[..n].copy_from_slice(&self.out[self.out_pos..self.out_pos + n]);
        self.out_pos += n;
        Ok(n)
    }
}

fn corrupt(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("corrupt ECM file: {}", what))
}

fn truncated(e: io::Error) -> io::Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        corrupt("it ends early")
    } else {
        e
    }
}

/// The name of the image an ECM file decodes to ("Game.bin.ecm" -> "Game.bin").
pub fn decoded_name(file_name: &str) -> Option<&str> {
    let dot = file_name.rfind('.')?;
    file_name[dot + 1..].eq_ignore_ascii_case("ecm").then(|| &file_name[..dot])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a type/count pair the way the ECM encoder does.
    fn type_count(out: &mut Vec<u8>, kind: u8, count: u32) {
        let mut n = count.wrapping_sub(1) as u64;
        let mut c = ((n & 0x1F) << 2) as u8 | kind;
        n >>= 5;
        while n != 0 {
            out.push(c | 0x80);
            c = (n & 0x7F) as u8;
            n >>= 7;
        }
        out.push(c);
    }

    /// A real Mode 1 sector: sync, header, data, then EDC and ECC.
    fn mode1_sector(minute: u8, data_seed: u8) -> Vec<u8> {
        let mut s = [0u8; SECTOR];
        s[1..11].fill(0xFF);
        s[12..16].copy_from_slice(&[minute, 0x02, 0x00, 0x01]);
        for (i, b) in s[0x10..0x810].iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(31).wrapping_add(data_seed);
        }
        write_edc(&mut s, 0, 0x810, 0x810);
        write_ecc(&mut s, false);
        s.to_vec()
    }

    fn decode(ecm: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        EcmReader::new(ecm).read_to_end(&mut out)?;
        Ok(out)
    }

    fn encode(raw_prefix: &[u8], sectors: &[Vec<u8>]) -> Vec<u8> {
        let mut ecm = b"ECM\0".to_vec();
        type_count(&mut ecm, 0, raw_prefix.len() as u32);
        ecm.extend_from_slice(raw_prefix);
        type_count(&mut ecm, 1, sectors.len() as u32);
        for s in sectors {
            ecm.extend_from_slice(&s[0x0C..0x0F]);
            ecm.extend_from_slice(&s[0x10..0x810]);
        }
        type_count(&mut ecm, 0, 0);
        let mut image = raw_prefix.to_vec();
        sectors.iter().for_each(|s| image.extend_from_slice(s));
        ecm.extend_from_slice(&edc_update(0, &image).to_le_bytes());
        ecm
    }

    #[test]
    fn mode1_sectors_are_rebuilt_exactly() {
        let raw = b"raw bytes the encoder couldn't model".to_vec();
        let sectors = vec![mode1_sector(0, 1), mode1_sector(1, 2), mode1_sector(2, 3)];
        let mut image = raw.clone();
        sectors.iter().for_each(|s| image.extend_from_slice(s));

        assert_eq!(decode(&encode(&raw, &sectors)).unwrap(), image);
    }

    #[test]
    fn mode2_sectors_are_rebuilt_without_sync_and_header() {
        // A Mode 2 Form 2 sector from subheader on (as ECM sees it), and a
        // Form 1 one, each preceded by its sync+header as raw bytes.
        let mut form2 = [0u8; SECTOR];
        form2[1..11].fill(0xFF);
        form2[0x0F] = 0x02;
        form2[0x10..0x14].copy_from_slice(&[0, 0, 0x20, 0]);
        form2[0x14..0x18].copy_from_slice(&[0, 0, 0x20, 0]);
        for (i, b) in form2[0x18..0x92C].iter_mut().enumerate() {
            *b = (i % 253) as u8;
        }
        write_edc(&mut form2, 0x10, 0x91C, 0x92C);

        let mut form1 = [0u8; SECTOR];
        form1[1..11].fill(0xFF);
        form1[12..16].copy_from_slice(&[0, 2, 1, 2]);
        form1[0x10..0x14].copy_from_slice(&[0, 0, 8, 0]);
        form1[0x14..0x18].copy_from_slice(&[0, 0, 8, 0]);
        for (i, b) in form1[0x18..0x818].iter_mut().enumerate() {
            *b = (i % 199) as u8;
        }
        write_edc(&mut form1, 0x10, 0x808, 0x818);
        write_ecc(&mut form1, true);

        let mut ecm = b"ECM\0".to_vec();
        type_count(&mut ecm, 0, 16);
        ecm.extend_from_slice(&form2[..16]);
        type_count(&mut ecm, 3, 1);
        ecm.extend_from_slice(&form2[0x14..0x92C]);
        type_count(&mut ecm, 0, 16);
        ecm.extend_from_slice(&form1[..16]);
        type_count(&mut ecm, 2, 1);
        ecm.extend_from_slice(&form1[0x14..0x818]);
        type_count(&mut ecm, 0, 0);
        let image = [&form2[..], &form1[..]].concat();
        ecm.extend_from_slice(&edc_update(0, &image).to_le_bytes());

        assert_eq!(decode(&ecm).unwrap(), image);
    }

    #[test]
    fn corrupt_data_fails_the_checksum() {
        let mut ecm = encode(b"", &[mode1_sector(0, 9)]);
        ecm[20] ^= 0x40;
        let err = decode(&ecm).unwrap_err();
        assert!(err.to_string().contains("checksum"), "{}", err);
    }

    #[test]
    fn truncated_file_is_an_error() {
        let ecm = encode(b"abc", &[mode1_sector(0, 5)]);
        assert!(decode(&ecm[..ecm.len() - 10]).is_err());
    }

    /// Decodes a real ECM file named by DEXTER_ECM_FILE; its stored checksum
    /// was computed by the original encoder, so passing is an independent check.
    #[test]
    #[ignore]
    fn real_ecm_file() {
        let path = std::env::var("DEXTER_ECM_FILE").expect("set DEXTER_ECM_FILE");
        let h = crate::scanner::hashing::hash_file(std::path::Path::new(&path)).unwrap();
        println!("size {} crc32 {} sha1 {}", h.size, h.full.crc32, h.full.sha1);
    }

    #[test]
    fn decoded_names() {
        assert_eq!(decoded_name("Final Fantasy VII (Europe) (Disc 1).bin.ecm"), Some("Final Fantasy VII (Europe) (Disc 1).bin"));
        assert_eq!(decoded_name("Game.bin"), None);
    }
}
