//! Finds installed emulators by looking for their executables in the usual
//! install locations. Bounded in depth and size, since a drive root can hold
//! a large game library.

use super::{executable_matches, STANDALONE_EMULATORS};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct DetectedEmulator {
    pub id: &'static str,
    pub name: &'static str,
    pub path: PathBuf,
}

/// Deep enough for layouts like D:\Switch\ryujinx\publish\Ryujinx.exe.
const MAX_DEPTH: usize = 4;
/// Per search root, so one huge folder can't stall detection.
const MAX_ENTRIES_PER_ROOT: usize = 50_000;

/// Folders that never contain emulators but can be enormous.
const SKIP_DIRS: &[&str] = &[
    "windows", "$recycle.bin", "system volume information", "programdata", "users", "steamapps",
    "node_modules", ".git", "roms", "windowsapps", "winsxs",
];

fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA", "APPDATA"] {
        if let Ok(dir) = std::env::var(var) {
            roots.push(PathBuf::from(dir));
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        for sub in ["Desktop", "Downloads", "Documents", "Emulators"] {
            roots.push(PathBuf::from(&profile).join(sub));
        }
    }
    roots.extend(fixed_drive_roots());
    roots.retain(|r| r.is_dir());
    roots.dedup();
    roots
}

/// Local fixed drives only: a disconnected network drive can hang for
/// seconds, and removable media rarely holds installed emulators.
#[cfg(windows)]
fn fixed_drive_roots() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_FIXED: u32 = 3;
    (b'C'..=b'Z')
        .filter_map(|letter| {
            let root: Vec<u16> = format!("{}:\\", letter as char).encode_utf16().chain([0]).collect();
            // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
            let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
            (kind == DRIVE_FIXED).then(|| PathBuf::from(format!("{}:\\", letter as char)))
        })
        .collect()
}

#[cfg(not(windows))]
fn fixed_drive_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn walk_for<F: FnMut(&Path, &str)>(root: &Path, mut on_file: F) {
    let walker = WalkDir::new(root).max_depth(MAX_DEPTH).into_iter().filter_entry(|e| {
        e.depth() == 0
            || !e.file_type().is_dir()
            || !SKIP_DIRS.contains(&e.file_name().to_string_lossy().to_lowercase().as_str())
    });
    for entry in walker.flatten().take(MAX_ENTRIES_PER_ROOT) {
        if entry.file_type().is_file() {
            if let Some(name) = entry.file_name().to_str() {
                on_file(entry.path(), name);
            }
        }
    }
}

pub struct Detection {
    pub retroarch: Option<PathBuf>,
    pub standalone: Vec<DetectedEmulator>,
}

/// Looks for RetroArch and every known standalone emulator. The first copy
/// found of each wins.
pub fn detect() -> Detection {
    let mut retroarch = None;
    let mut standalone: Vec<DetectedEmulator> = Vec::new();
    let mut roots = search_roots();
    // Steam installs RetroArch under steamapps, which the walk skips.
    for drive in fixed_drive_roots() {
        for steam in ["SteamLibrary", "Program Files (x86)\\Steam"] {
            roots.insert(0, drive.join(steam).join("steamapps").join("common").join("RetroArch"));
        }
    }
    roots.retain(|r| r.is_dir());

    for root in roots {
        walk_for(&root, |path, name| {
            if retroarch.is_none() && name.eq_ignore_ascii_case("retroarch.exe") {
                retroarch = Some(path.to_path_buf());
            }
            for known in STANDALONE_EMULATORS {
                if standalone.iter().any(|d| d.id == known.id) {
                    continue;
                }
                if known.executables.iter().any(|pattern| executable_matches(pattern, name)) {
                    standalone.push(DetectedEmulator { id: known.id, name: known.name, path: path.to_path_buf() });
                }
            }
        });
    }
    Detection { retroarch, standalone }
}
