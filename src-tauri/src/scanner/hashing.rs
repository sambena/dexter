use md5::{Digest, Md5};
use sha1::Sha1;
use std::io::Read;
use std::path::Path;

pub struct FileHashes {
    pub size: u64,
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

pub fn hash_reader<R: Read>(mut reader: R) -> std::io::Result<FileHashes> {
    let mut crc = crc32fast::Hasher::new();
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut buf = [0u8; 65536];
    let mut size = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        md5.update(&buf[..n]);
        sha1.update(&buf[..n]);
        size += n as u64;
    }
    Ok(FileHashes {
        size,
        crc32: format!("{:08x}", crc.finalize()),
        md5: format!("{:x}", md5.finalize()),
        sha1: format!("{:x}", sha1.finalize()),
    })
}

pub fn hash_file(path: &Path) -> std::io::Result<FileHashes> {
    let file = std::fs::File::open(path)?;
    hash_reader(file)
}
