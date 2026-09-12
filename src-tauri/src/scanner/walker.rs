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

/// Recursively lists every file under a system's folder.
pub fn list_files_in(system_dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(system_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}
