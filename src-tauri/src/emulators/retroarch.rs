use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct CoreInfo {
    /// File stem without "_libretro", e.g. "mgba". Used in the -L path.
    pub id: String,
    /// Short name for display, e.g. "mGBA".
    pub name: String,
    /// Platforms the core runs, in libretro database spelling.
    pub platforms: Vec<String>,
}

pub struct RetroArch {
    pub exe: PathBuf,
    pub cores_dir: PathBuf,
    pub info_dir: PathBuf,
}

impl RetroArch {
    /// Resolves RetroArch's folders from its executable, honouring
    /// retroarch.cfg overrides. In that file ":" stands for the executable's
    /// own folder, e.g. `libretro_directory = ":\cores"`.
    pub fn at(exe: &Path) -> RetroArch {
        let base = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        let config = std::fs::read_to_string(base.join("retroarch.cfg")).unwrap_or_default();
        let dir_setting = |key: &str, default: &str| -> PathBuf {
            config_value(&config, key)
                .filter(|v| !v.is_empty() && v != "default")
                .map(|v| match v.strip_prefix(':') {
                    Some(rest) => base.join(rest.trim_start_matches(['\\', '/'])),
                    None => PathBuf::from(v),
                })
                .unwrap_or_else(|| base.join(default))
        };
        RetroArch {
            exe: exe.to_path_buf(),
            cores_dir: dir_setting("libretro_directory", "cores"),
            info_dir: dir_setting("libretro_info_path", "info"),
        }
    }

    pub fn core_path(&self, core_id: &str) -> PathBuf {
        self.cores_dir.join(format!("{}_libretro.dll", core_id))
    }

    pub fn installed_cores(&self) -> HashSet<String> {
        std::fs::read_dir(&self.cores_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.strip_suffix("_libretro.dll").map(str::to_string))
            .collect()
    }

    /// Every core RetroArch knows about, installed or not, from its info files.
    pub fn core_infos(&self) -> Vec<CoreInfo> {
        let mut cores: Vec<CoreInfo> = std::fs::read_dir(&self.info_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let id = file_name.to_str()?.strip_suffix("_libretro.info")?.to_string();
                let text = std::fs::read_to_string(entry.path()).ok()?;
                Some(parse_core_info(id, &text))
            })
            .collect();
        cores.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        cores
    }
}

fn config_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

pub fn parse_core_info(id: String, text: &str) -> CoreInfo {
    let name = config_value(text, "corename")
        .filter(|n| !n.is_empty())
        .or_else(|| {
            // display_name looks like "Nintendo - Game Boy Advance (mGBA)".
            let display = config_value(text, "display_name")?;
            let inner = display.rsplit_once('(')?.1.strip_suffix(')')?.to_string();
            Some(inner)
        })
        .unwrap_or_else(|| id.clone());
    let platforms = config_value(text, "database")
        .map(|d| d.split('|').map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    CoreInfo { id, name, platforms }
}

/// Whether an emulator configuration just points at RetroArch without a
/// core, which opens RetroArch's menu instead of the game.
pub fn is_bare_retroarch(path: &str, args: Option<&str>) -> bool {
    let is_retroarch = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("retroarch.exe"));
    let names_core = args.is_some_and(|a| a.split_whitespace().any(|t| t == "-L" || t.starts_with("--libretro")));
    is_retroarch && !names_core
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_info_from_real_file_format() {
        let info = parse_core_info(
            "mgba".into(),
            "display_name = \"Nintendo - Game Boy Advance (mGBA)\"\nsupported_extensions = \"gb|gbc|gba\"\ncorename = \"mGBA\"\n\
             database = \"Nintendo - Game Boy|Nintendo - Game Boy Color|Nintendo - Game Boy Advance\"\n",
        );
        assert_eq!(info.name, "mGBA");
        assert_eq!(info.platforms, vec!["Nintendo - Game Boy", "Nintendo - Game Boy Color", "Nintendo - Game Boy Advance"]);
    }

    #[test]
    fn name_falls_back_to_display_name() {
        let info = parse_core_info("x".into(), "display_name = \"Sony - PlayStation (SwanStation)\"\n");
        assert_eq!(info.name, "SwanStation");
    }

    #[test]
    fn config_overrides_and_colon_prefix() {
        let dir = std::env::temp_dir().join(format!("dexter-ra-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("retroarch.cfg"), "libretro_directory = \":\\cores64\"\nlibretro_info_path = \"default\"\n").unwrap();
        let ra = RetroArch::at(&dir.join("retroarch.exe"));
        assert_eq!(ra.cores_dir, dir.join("cores64"));
        assert_eq!(ra.info_dir, dir.join("info"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bare_retroarch_detection() {
        assert!(is_bare_retroarch(r"D:\RetroArch\retroarch.exe", None));
        assert!(is_bare_retroarch(r"D:\RetroArch\retroarch.exe", Some("%ROM%")));
        assert!(!is_bare_retroarch(r"D:\RetroArch\retroarch.exe", Some(r"-L cores\mgba_libretro.dll %ROM%")));
        assert!(!is_bare_retroarch(r"D:\Cemu\Cemu.exe", None));
    }
}
