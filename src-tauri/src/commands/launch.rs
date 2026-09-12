use crate::commands::maintenance::on_disk_path;
use crate::db::repo;
use crate::state::AppState;
use std::path::Path;
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

        let emulator = details
            .emulator_path
            .filter(|p| !p.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "No emulator is set for {}. Choose one in Settings → Emulators.",
                    details.system_name.as_deref().unwrap_or("this ROM's system")
                )
            })?;
        let emulator_path = Path::new(&emulator);
        if !emulator_path.is_file() {
            return Err(format!("The emulator wasn't found at {}.", emulator));
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

        let mut cmd = Command::new(emulator_path);
        cmd.args(build_args(details.emulator_args.as_deref(), &rom_path));
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
