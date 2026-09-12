use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct SystemFolder {
    pub folder_name: String,
    pub path: PathBuf,
}

/// Lists the immediate subdirectories of the ROM root — each one is treated as a system.
pub fn list_system_folders(root: &Path) -> std::io::Result<Vec<SystemFolder>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.push(SystemFolder {
                    folder_name: name.to_string(),
                    path: path.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// Subfolder names commonly created by emulator frontends (RetroArch, LaunchBox, etc.)
/// alongside the actual ROMs — never worth descending into during a scan.
const SKIP_DIR_NAMES: &[&str] = &[
    "media", "thumbnails", "thumbs", "screenshots", "snaps", "titles", "boxart", "boxarts",
    "covers", "manuals", "saves", "save", "states", "savestates", "cheats", "configs", "config",
    "playlists", "overlays", "shaders", "logs", "downloaded_media", "named_boxarts",
    "named_snaps", "named_titles", "system",
];

/// File extensions that are never ROM data — save states, art, and frontend metadata that
/// tend to live right next to the ROMs themselves rather than in their own subfolder.
const SKIP_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "gif", "webp", "ico", "tif", "tiff", "svg",
    "txt", "nfo", "md", "pdf", "doc", "docx", "url", "lnk", "ini", "cfg", "conf",
    "json", "xml", "yml", "yaml", "log", "db", "sqlite", "sqlite3",
    "srm", "state", "rtc", "bak", "tmp",
    // Battery saves and memory cards written by emulators.
    "sav", "eep", "sra", "fla", "mpk", "dsv", "mcr",
    // Video snaps and music that frontends download next to ROMs.
    "mp4", "mkv", "avi", "webm", "mov", "mp3", "ogg", "wav", "flac",
    // Tools and checksum files that come along with ROM sets.
    "exe", "dll", "bat", "cmd", "sh", "sfv", "md5", "sha1",
];

fn has_skipped_extension(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let lower = ext.to_lowercase();
            SKIP_EXTENSIONS.contains(&lower.as_str()) || lower.starts_with("state")
        }
        None => false,
    }
}

/// Detects an extracted/decrypted Wii U title dump: a folder with a `meta/meta.xml`
/// (the standard layout produced by CDecrypt and similar tools, alongside `code/`
/// and `content/`). Such a "ROM" is really thousands of loose asset files, so the
/// whole folder must be treated as a single scan unit rather than walked into.
fn is_wiiu_title_root(dir: &Path) -> bool {
    dir.join("meta").join("meta.xml").is_file()
}

pub enum ScanTarget {
    /// A single ROM file to hash directly (or, if a .zip, to hash entry-by-entry).
    File(PathBuf),
    /// A whole directory that represents one "ROM" (e.g. a Wii U title dump) —
    /// too many internal files to hash meaningfully, so it's listed unmatched.
    FolderRom(PathBuf),
}

/// Recursively lists ROM candidates under a system's folder, pruning known non-ROM
/// subfolders (box art, save states, thumbnails, ...) and file types, and treating
/// whole-folder ROM dumps (e.g. Wii U titles) as a single unit instead of descending.
pub fn list_scan_targets(system_dir: &Path) -> Vec<ScanTarget> {
    let mut results = Vec::new();
    let mut it = WalkDir::new(system_dir).into_iter();
    while let Some(entry) = it.next() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().is_dir() {
            if entry.depth() == 0 {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if SKIP_DIR_NAMES.contains(&name.as_str()) {
                it.skip_current_dir();
                continue;
            }
            if is_wiiu_title_root(entry.path()) {
                results.push(ScanTarget::FolderRom(entry.path().to_path_buf()));
                it.skip_current_dir();
            }
            continue;
        }
        if !has_skipped_extension(entry.path()) {
            results.push(ScanTarget::File(entry.into_path()));
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_rom_files_found_in_real_libraries_are_skipped() {
        for name in ["Pokemon - Red Version.sav", "Combat-video.mp4", "scraper.exe", "Game.STATE1", "cover.PNG"] {
            assert!(has_skipped_extension(Path::new(name)), "{} should be skipped", name);
        }
    }

    #[test]
    fn rom_files_are_kept() {
        for name in ["Super Mario Bros..nes", "Aerobiz.smc", "F-Zero GX (USA).rvz", "game.zip", "Wii U Title"] {
            assert!(!has_skipped_extension(Path::new(name)), "{} should be kept", name);
        }
    }
}
