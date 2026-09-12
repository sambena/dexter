//! The command set exposed outside the window: over the local HTTP API while
//! the app is open, and through dexter-cli. Both call the same functions the
//! window invokes, so behaviour can't drift between the three.

pub mod server;

use crate::commands;
use crate::models::{RomFilter, Settings};
use crate::state::AppState;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};

pub struct CommandInfo {
    pub name: &'static str,
    pub args: &'static str,
    pub summary: &'static str,
    /// Tells the window to reload its lists once the command finishes.
    pub changes_library: bool,
    /// Shown in the window's progress strip while the command runs.
    pub job: Option<Job>,
}

#[derive(Clone, Copy)]
pub struct Job {
    pub label: &'static str,
    pub cancellable: bool,
}

const fn read(name: &'static str, args: &'static str, summary: &'static str) -> CommandInfo {
    CommandInfo { name, args, summary, changes_library: false, job: None }
}

const fn write(name: &'static str, args: &'static str, summary: &'static str, job: Option<Job>) -> CommandInfo {
    CommandInfo { name, args, summary, changes_library: true, job }
}

const fn job(label: &'static str, cancellable: bool) -> Option<Job> {
    Some(Job { label, cancellable })
}

pub const COMMANDS: &[CommandInfo] = &[
    read("get_settings", "", "Current settings (ROM root folder)."),
    write("save_settings", "settings.rom_root_path?", "Replace settings.", None),
    read("list_systems", "", "Systems with their emulator, DAT URL and whether a DAT is imported."),
    write(
        "set_system_emulator",
        "system_id, emulator_path?, emulator_args?",
        "Set a system's emulator. An omitted value is cleared, as in the window.",
        None,
    ),
    read("get_emulator_setup", "", "Detected RetroArch/emulators and each system's emulator options."),
    write("set_retroarch_path", "path?", "Set (or clear) where RetroArch is installed.", None),
    write(
        "set_system_emulator_choice",
        "system_id, choice.kind (none|retroarch|standalone|custom), choice.core?, choice.emulator_id?, choice.path?, choice.args?",
        "Choose how a system launches.",
        None,
    ),
    write("auto_configure_emulators", "", "Set up every system without a working emulator.", Some(Job { label: "Setting up emulators", cancellable: false })),
    write("set_system_dat_url", "system_id, dat_url?", "Set or clear a system's direct DAT download URL.", None),
    read("list_dat_sources", "", "Imported DATs."),
    read("has_known_dat_source", "folder_name", "Whether a DAT can be fetched automatically for a system folder."),
    write("import_dat_file", "system_id, file_path", "Import a DAT (.dat/.xml/.zip) for a system and re-match.", job("Importing DAT", false)),
    write("import_dat_folder", "folder_path", "Import every DAT in a folder, matching systems by name.", job("Importing DAT folder", false)),
    write("fetch_dat_file", "system_id", "Download and import the system's DAT.", job("Fetching DAT", false)),
    write("remove_dat_source", "dat_source_id", "Remove an imported DAT and re-match.", job("Removing DAT and re-matching", false)),
    write("scan_library", "", "Discover files under the ROM root.", job("Scanning library", true)),
    write("hash_pending_roms", "", "Hash pending ROMs and match them against DATs.", job("Hashing & matching", true)),
    read("cancel_scan", "", "Stop the running scan, hash or box art job."),
    read("list_roms", "filter.search_text?, filter.system_id?, filter.match_status?, filter.sort_by?, filter.sort_dir?", "List ROMs."),
    read("get_rom_details", "rom_id", "Full details for one ROM."),
    read("get_box_art", "rom_id", "A ROM's box art as a data URL, or null."),
    read("has_known_box_art_source", "folder_name", "Whether box art can be downloaded for a system folder."),
    write("fetch_box_art", "rom_id", "Download box art for one matched ROM.", job("Downloading box art", false)),
    write("fetch_all_box_art", "", "Download missing box art for every matched game.", job("Downloading box art", true)),
    write("set_box_art_file", "rom_id, file_path", "Use a local image as a ROM's box art.", None),
    read("list_duplicates", "", "Groups of byte-identical files."),
    read("preview_renames", "", "Planned DAT-name renames, without touching disk."),
    write("apply_renames", "rom_ids", "Rename files to their DAT names (plan is recomputed first).", job("Renaming files", false)),
    write("delete_roms", "rom_ids", "Send the files behind these ROMs to the Recycle Bin.", job("Deleting ROMs", false)),
    read("launch_rom", "rom_id", "Start the system's emulator on a ROM."),
];

pub fn find(name: &str) -> Option<&'static CommandInfo> {
    let name = name.replace('-', "_");
    COMMANDS.iter().find(|c| c.name == name)
}

fn arg<T: DeserializeOwned>(args: &Map<String, Value>, name: &str) -> Result<T, String> {
    let value = args.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value.clone())
        .or_else(|e| match &value {
            // dexter-cli reads `--folder-name 1942` as a number; accept it
            // where text was expected.
            Value::Number(_) | Value::Bool(_) => {
                serde_json::from_value(Value::String(value.to_string())).map_err(|_| e)
            }
            _ => Err(e),
        })
        .map_err(|e| match args.get(name) {
            None => format!("missing argument `{}`", name),
            Some(_) => format!("invalid argument `{}`: {}", name, e),
        })
}

fn to_value<T: Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}

/// Runs one command. The window is told when a job starts and ends and when
/// the library changed, so it stays in sync with work it didn't start.
pub async fn dispatch(app: &AppHandle, name: &str, args: Value) -> Result<Value, String> {
    let info = find(name).ok_or_else(|| format!("unknown command `{}`", name))?;
    let args = match args {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        _ => return Err("arguments must be a JSON object".to_string()),
    };

    if let Some(job) = info.job {
        let _ = app.emit("api://job-started", json!({ "label": job.label, "cancellable": job.cancellable }));
    }
    let result = run(app, info.name, &args).await;
    if info.job.is_some() {
        let _ = app.emit("api://job-finished", ());
    }
    if info.changes_library && result.is_ok() {
        let _ = app.emit("api://library-changed", ());
    }
    result
}

async fn run(app: &AppHandle, name: &str, a: &Map<String, Value>) -> Result<Value, String> {
    use commands::*;
    let handle = app.clone();
    let state = app.state::<AppState>();

    match name {
        "get_settings" => to_value(settings::get_settings(state)),
        "save_settings" => to_value(settings::save_settings(arg::<Option<Settings>>(a, "settings")?.unwrap_or_default(), state)),
        "list_systems" => to_value(settings::list_systems(state)),
        "set_system_emulator" => to_value(settings::set_system_emulator(
            arg(a, "system_id")?,
            arg(a, "emulator_path")?,
            arg(a, "emulator_args")?,
            state,
        )),
        "get_emulator_setup" => to_value(emulators::get_emulator_setup(handle).await),
        "set_retroarch_path" => to_value(emulators::set_retroarch_path(arg(a, "path")?, handle)),
        "set_system_emulator_choice" => {
            to_value(emulators::set_system_emulator_choice(arg(a, "system_id")?, arg(a, "choice")?, handle).await)
        }
        "auto_configure_emulators" => to_value(emulators::auto_configure_emulators(handle).await),
        "set_system_dat_url" => to_value(dat::set_system_dat_url(arg(a, "system_id")?, arg(a, "dat_url")?, state)),
        "list_dat_sources" => to_value(dat::list_dat_sources(state)),
        "has_known_dat_source" => Ok(json!(dat::has_known_dat_source(arg(a, "folder_name")?))),
        "import_dat_file" => to_value(dat::import_dat_file(arg(a, "system_id")?, arg(a, "file_path")?, handle).await),
        "import_dat_folder" => to_value(dat::import_dat_folder(arg(a, "folder_path")?, handle).await),
        "fetch_dat_file" => to_value(dat::fetch_dat_file(arg(a, "system_id")?, handle).await),
        "remove_dat_source" => to_value(dat::remove_dat_source(arg(a, "dat_source_id")?, handle).await),
        "scan_library" => to_value(scan::scan_library(handle, state).await),
        "hash_pending_roms" => to_value(hash::hash_pending_roms(handle, state).await),
        "cancel_scan" => to_value(control::cancel_scan(state)),
        "list_roms" => to_value(roms::list_roms(arg::<Option<RomFilter>>(a, "filter")?.unwrap_or_default(), state)),
        "get_rom_details" => to_value(roms::get_rom_details(arg(a, "rom_id")?, state)),
        "get_box_art" => to_value(art::get_box_art(arg(a, "rom_id")?, state)),
        "has_known_box_art_source" => Ok(json!(art::has_known_box_art_source(arg(a, "folder_name")?))),
        "fetch_box_art" => to_value(art::fetch_box_art(arg(a, "rom_id")?, handle).await),
        "fetch_all_box_art" => to_value(art::fetch_all_box_art(handle).await),
        "set_box_art_file" => {
            let rom_id = arg(a, "rom_id")?;
            let file_path: String = arg(a, "file_path")?;
            to_value(art::set_box_art_from_file(app, &state, rom_id, Path::new(&file_path)))
        }
        "list_duplicates" => to_value(maintenance::list_duplicates(handle).await),
        "preview_renames" => to_value(maintenance::preview_renames(handle).await),
        "apply_renames" => to_value(maintenance::apply_renames(arg(a, "rom_ids")?, handle).await),
        "delete_roms" => to_value(maintenance::delete_roms(arg(a, "rom_ids")?, handle).await),
        "launch_rom" => to_value(launch::launch_rom(arg(a, "rom_id")?, handle).await),
        _ => Err(format!("command `{}` is listed but not wired up", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_unique() {
        for (i, c) in COMMANDS.iter().enumerate() {
            assert!(COMMANDS[i + 1..].iter().all(|o| o.name != c.name), "duplicate {}", c.name);
        }
    }

    #[test]
    fn find_accepts_kebab_case() {
        assert_eq!(find("list-roms").map(|c| c.name), Some("list_roms"));
        assert!(find("nope").is_none());
    }

    #[test]
    fn missing_required_argument_is_named() {
        let err = arg::<i64>(&Map::new(), "rom_id").unwrap_err();
        assert_eq!(err, "missing argument `rom_id`");
    }

    #[test]
    fn optional_argument_may_be_omitted() {
        assert_eq!(arg::<Option<String>>(&Map::new(), "dat_url").unwrap(), None);
    }
}
