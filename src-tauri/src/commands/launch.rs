use crate::commands::maintenance::on_disk_path;
use crate::db::repo;
use crate::emulators::retroarch::RetroArch;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

const ROM_PLACEHOLDER: &str = "%ROM%";

/// Splits an argument string the way a Windows command line reads it:
/// whitespace separates arguments and double quotes group them. The quotes
/// themselves are dropped because Command re-quotes each argument when it
/// builds the real command line, so `"%ROM%"` and `%ROM%` both reach the
/// emulator as a single argument even when the path contains spaces.
fn split_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in args.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(current);
    }
    out
}

/// The emulator's argument list for one ROM. When the configured arguments
/// don't mention %ROM% (including when none are set), the ROM path goes last,
/// which is where nearly every emulator expects it.
fn build_args(template: Option<&str>, rom_path: &str) -> Vec<String> {
    let mut args = split_args(template.unwrap_or(""));
    if args.iter().any(|a| a.contains(ROM_PLACEHOLDER)) {
        for a in &mut args {
            *a = a.replace(ROM_PLACEHOLDER, rom_path);
        }
    } else {
        args.push(rom_path.to_string());
    }
    args
}

/// The path to hand an emulator. An extracted Wii U title is a folder, and
/// Cemu launches it from the single executable in its code folder.
fn launchable_path(rom: &Path) -> String {
    if rom.is_dir() {
        let rpx: Vec<PathBuf> = std::fs::read_dir(rom.join("code"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rpx")))
            .collect();
        if let [only] = rpx.as_slice() {
            return only.to_string_lossy().to_string();
        }
    }
    rom.to_string_lossy().to_string()
}

/// Starts the system's configured emulator on a ROM and returns once the
/// process is running, without waiting for it to exit.
#[tauri::command]
pub async fn launch_rom(rom_id: i64, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let details = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            repo::get_rom_details(&conn, rom_id).map_err(|e| e.to_string())?
        }
        .ok_or_else(|| "This ROM is no longer in the library.".to_string())?;

        if let Some(kind @ ("update" | "dlc")) = details.title_kind.as_deref() {
            return Err(format!(
                "This is {} for a game, not a game, so it can't be started on its own. Install it into the emulator (in Cemu: File → Install game title, update or DLC), then play the game.",
                if kind == "update" { "an update" } else { "DLC" }
            ));
        }

        // A zipped ROM is launched by handing over the archive itself, which
        // RetroArch and most standalone emulators open directly.
        let rom_path = on_disk_path(&details.file_path, details.archive_member.as_deref());
        if !Path::new(&rom_path).exists() {
            return Err(format!(
                "The ROM wasn't found at {}. If it was moved, run Scan Files.",
                rom_path
            ));
        }
        let rom_path = launchable_path(Path::new(&rom_path));

        let (emulator, args) = match details.emulator_core.as_deref().filter(|c| !c.is_empty()) {
            Some(core) => {
                let saved = {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    repo::get_setting(&conn, "retroarch_path").map_err(|e| e.to_string())?
                };
                let exe = saved
                    .filter(|p| Path::new(p).is_file())
                    .map(PathBuf::from)
                    .or_else(|| crate::emulators::detect::detect().retroarch)
                    .ok_or("RetroArch wasn't found. Set where it's installed in Settings → Emulators.")?;
                let retroarch = RetroArch::at(&exe);
                let core_path = retroarch.core_path(core);
                if !core_path.is_file() {
                    return Err(format!(
                        "The {} core isn't installed in RetroArch yet. In RetroArch: Main Menu → Online Updater → Core Downloader.",
                        core
                    ));
                }
                (exe.to_string_lossy().to_string(), vec!["-L".to_string(), core_path.to_string_lossy().to_string(), rom_path])
            }
            None => {
                let emulator = details.emulator_path.filter(|p| !p.trim().is_empty()).ok_or_else(|| {
                    format!(
                        "No emulator is set for {}. Choose one in Settings → Emulators.",
                        details.system_name.as_deref().unwrap_or("this ROM's system")
                    )
                })?;
                let args = build_args(details.emulator_args.as_deref(), &rom_path);
                (emulator, args)
            }
        };
        let emulator_path = Path::new(&emulator);
        if !emulator_path.is_file() {
            return Err(format!("The emulator wasn't found at {}.", emulator));
        }

        let mut cmd = Command::new(emulator_path);
        cmd.args(args);
        // Arguments like RetroArch's `-L cores\snes9x_libretro.dll` are
        // relative to the emulator's own folder, not Dexter's.
        if let Some(dir) = emulator_path.parent() {
            cmd.current_dir(dir);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start {}: {}", emulator, e))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROM: &str = r"D:\ROMs\SNES\Super Metroid (Japan, USA).sfc";

    #[test]
    fn no_args_passes_rom_alone() {
        assert_eq!(build_args(None, ROM), vec![ROM]);
        assert_eq!(build_args(Some("   "), ROM), vec![ROM]);
    }

    #[test]
    fn args_without_placeholder_put_rom_last() {
        assert_eq!(build_args(Some("--fullscreen"), ROM), vec!["--fullscreen", ROM]);
    }

    #[test]
    fn placeholder_is_replaced_in_place() {
        assert_eq!(
            build_args(Some(r"-L cores\snes9x_libretro.dll %ROM% -f"), ROM),
            vec!["-L", r"cores\snes9x_libretro.dll", ROM, "-f"]
        );
    }

    #[test]
    fn quoted_placeholder_stays_one_argument() {
        assert_eq!(build_args(Some(r#""%ROM%""#), ROM), vec![ROM]);
    }

    #[test]
    fn quotes_group_arguments_with_spaces() {
        assert_eq!(
            split_args(r#"-L "C:\Program Files\cores\a.dll" -f"#),
            vec!["-L", r"C:\Program Files\cores\a.dll", "-f"]
        );
    }

    #[test]
    fn placeholder_inside_a_flag() {
        assert_eq!(build_args(Some("--rom=%ROM%"), ROM), vec![format!("--rom={}", ROM)]);
    }

    #[test]
    fn empty_quotes_are_an_empty_argument() {
        assert_eq!(split_args(r#"a "" b"#), vec!["a", "", "b"]);
    }
}
