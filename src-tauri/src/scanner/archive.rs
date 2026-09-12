use crate::scanner::hashing::{hash_reader, FileHashes};
use std::fs::File;
use std::path::Path;

pub struct ArchiveEntryMeta {
    pub inner_name: String,
    pub size: u64,
}

/// Lists the entries inside a .zip without decompressing/hashing them — cheap
/// metadata-only read, used for the quick file-discovery scan.
pub fn list_zip_entries(path: &Path) -> anyhow::Result<Vec<ArchiveEntryMeta>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut results = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        results.push(ArchiveEntryMeta {
            inner_name: entry.name().to_string(),
            size: entry.size(),
        });
    }
    Ok(results)
}

/// Hashes a single named entry inside a .zip — used by the hashing pass.
pub fn hash_zip_member(zip_path: &Path, entry_name: &str) -> anyhow::Result<FileHashes> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let entry = archive.by_name(entry_name)?;
    Ok(hash_reader(entry)?)
}
