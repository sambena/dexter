use crate::scanner::hashing::{hash_reader, FileHashes};
use std::fs::File;
use std::path::Path;

pub struct ArchiveEntry {
    pub inner_name: String,
    pub hashes: FileHashes,
}

pub fn hash_zip_entries(path: &Path) -> anyhow::Result<Vec<ArchiveEntry>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut results = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let inner_name = entry.name().to_string();
        let hashes = hash_reader(entry)?;
        results.push(ArchiveEntry { inner_name, hashes });
    }
    Ok(results)
}
