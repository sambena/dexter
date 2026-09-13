use crate::db::repo;
use crate::emulators::detect::{self, DetectedEmulator};
use crate::emulators::retroarch::{self, CoreInfo, RetroArch};
use crate::emulators::{self as known, known_emulator, PREFER_STANDALONE, RECOMMENDED_CORES};
use crate::models::{
    AutoConfigureSummary, DetectedEmulatorDto, EmulatorChoice, EmulatorOptionDto, EmulatorSetupDto, SystemDto,
    SystemEmulatorDto,
};
use crate::state::AppState;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::Manager;

const RETROARCH_SETTING: &str = "retroarch_path";

const INSTALL_CORE_HINT: &str = "In RetroArch: Main Menu → Online Updater → Core Downloader.";

/// What's installed on this PC, gathered once per request.
pub struct Environment {
    pub retroarch: Option<RetroArch>,
    pub cores: Vec<CoreInfo>,
    pub installed_cores: HashSet<String>,
    pub standalone: Vec<DetectedEmulator>,
}

impl Environment {
    fn new(retroarch_exe: Option<PathBuf>, standalone: Vec<DetectedEmulator>) -> Self {
        let retroarch = retroarch_exe.filter(|p| p.is_file()).map(|p| RetroArch::at(&p));
        let cores = retroarch.as_ref().map(|r| r.core_infos()).unwrap_or_default();
        let installed_cores = retroarch.as_ref().map(|r| r.installed_cores()).unwrap_or_default();
        Environment { retroarch, cores, installed_cores, standalone }
    }

    pub(crate) fn core_name(&self, id: &str) -> String {
        self.cores.iter().find(|c| c.id == id).map_or_else(|| id.to_string(), |c| c.name.clone())
    }
}

fn saved_retroarch(conn: &rusqlite::Connection) -> Result<Option<PathBuf>, String> {
    Ok(repo::get_setting(conn, RETROARCH_SETTING).map_err(|e| e.to_string())?.filter(|p| !p.is_empty()).map(PathBuf::from))
}

/// Where a core ranks among the recommended ones for any of the platforms;
/// unrecommended cores sort after all of them.
fn core_rank(core_id: &str, platforms: &[String]) -> usize {
    RECOMMENDED_CORES
        .iter()
        .filter(|(platform, _)| platforms.iter().any(|p| p == platform))
        .filter_map(|(_, cores)| cores.iter().position(|c| *c == core_id))
        .min()
        .unwrap_or(usize::MAX)
}

pub fn system_setup(system: &SystemDto, platforms: &[String], env: &Environment) -> SystemEmulatorDto {
    let runs_here = |supported: &[String]| supported.iter().any(|s| platforms.contains(s));

    let mut cores: Vec<&CoreInfo> = env.cores.iter().filter(|c| runs_here(&c.platforms)).collect();
    cores.sort_by_key(|c| (core_rank(&c.id, platforms), c.name.to_lowercase(), c.id.clone()));
    let prefers_standalone = platforms.iter().any(|p| PREFER_STANDALONE.contains(&p.as_str()));

    // Several builds of a core can share a name (Mupen64Plus-Next and its
    // GLES variants), so those get their id to tell them apart.
    let core_option = |core: &CoreInfo| {
        let installed = env.installed_cores.contains(&core.id);
        let shared_name = cores.iter().filter(|c| c.name == core.name).count() > 1;
        let name = if shared_name { format!("{} ({})", core.name, core.id) } else { core.name.clone() };
        EmulatorOptionDto {
            kind: "retroarch".into(),
            core: Some(core.id.clone()),
            emulator_id: None,
            label: format!("RetroArch · {}{}", name, if installed { "" } else { " (not installed)" }),
            installed,
            recommended: false,
        }
    };
    let (installed, not_installed): (Vec<&CoreInfo>, Vec<&CoreInfo>) =
        cores.iter().partition(|c| env.installed_cores.contains(&c.id));

    let mut standalone: Vec<&DetectedEmulator> = env
        .standalone
        .iter()
        .filter(|d| known_emulator(d.id).is_some_and(|k| k.platforms.iter().any(|p| platforms.iter().any(|q| q == p))))
        .collect();
    standalone.sort_by_key(|d| known::STANDALONE_EMULATORS.iter().position(|k| k.id == d.id));
    let standalone_options = standalone.iter().map(|d| EmulatorOptionDto {
        kind: "standalone".into(),
        core: None,
        emulator_id: Some(d.id.into()),
        label: d.name.into(),
        installed: true,
        recommended: false,
    });

    // Best first: what's installed before what isn't, and standalone ahead of
    // RetroArch only for platforms where it runs better.
    let mut options: Vec<EmulatorOptionDto> = Vec::new();
    if prefers_standalone {
        options.extend(standalone_options);
        options.extend(installed.iter().map(|c| core_option(c)));
    } else {
        options.extend(installed.iter().map(|c| core_option(c)));
        options.extend(standalone_options);
    }
    options.extend(not_installed.iter().map(|c| core_option(c)));
    if let Some(best) = options.iter_mut().find(|o| is_viable(o, platforms)) {
        best.recommended = true;
    }

    let mut setup = SystemEmulatorDto {
        system_id: system.id,
        name: system.name.clone(),
        kind: "none".into(),
        core: None,
        emulator_id: None,
        emulator_path: system.emulator_path.clone(),
        emulator_args: system.emulator_args.clone(),
        options,
        problem: None,
    };

    if let Some(core) = system.emulator_core.as_deref().filter(|c| !c.is_empty()) {
        setup.kind = "retroarch".into();
        setup.core = Some(core.to_string());
        setup.problem = if env.retroarch.is_none() {
            Some("RetroArch wasn't found. Set where it's installed above.".into())
        } else if !env.installed_cores.contains(core) {
            Some(format!("The {} core isn't installed yet. {}", env.core_name(core), INSTALL_CORE_HINT))
        } else {
            None
        };
    } else if let Some(path) = system.emulator_path.as_deref().filter(|p| !p.trim().is_empty()) {
        let standalone = env.standalone.iter().find(|d| d.path.to_string_lossy().eq_ignore_ascii_case(path));
        match standalone {
            Some(detected) => {
                setup.kind = "standalone".into();
                setup.emulator_id = Some(detected.id.into());
            }
            None => setup.kind = "custom".into(),
        }
        setup.problem = if !Path::new(path).is_file() {
            Some(format!("Nothing found at {}.", path))
        } else if retroarch::is_bare_retroarch(path, system.emulator_args.as_deref()) {
            Some("RetroArch opens its menu unless it's told which core to use. Pick a RetroArch core from the list.".into())
        } else {
            None
        };
    }
    setup
}

/// Whether an option is worth picking automatically: anything that's
/// installed, or a recommended core that only needs downloading.
fn is_viable(option: &EmulatorOptionDto, platforms: &[String]) -> bool {
    option.installed || core_rank(option.core.as_deref().unwrap_or(""), platforms) != usize::MAX
}

/// The best way to run a system automatically, if there is one: the first
/// viable option, since options are already ordered best first.
/// Returns the choice and a label for the summary.
pub fn auto_choice(setup: &SystemEmulatorDto, env: &Environment, platforms: &[String]) -> Option<(EmulatorChoice, String)> {
    let best = setup.options.iter().find(|o| is_viable(o, platforms))?;
    match best.kind.as_str() {
        "standalone" => {
            let id = best.emulator_id.as_deref()?;
            let detected = env.standalone.iter().find(|d| d.id == id)?;
            Some((
                EmulatorChoice::Standalone { emulator_id: id.to_string(), path: detected.path.to_string_lossy().to_string() },
                best.label.clone(),
            ))
        }
        _ => {
            let core = best.core.clone()?;
            let label = best.label.trim_end_matches(" (not installed)").to_string();
            Some((EmulatorChoice::Retroarch { core }, label))
        }
    }
}

fn apply_choice(conn: &rusqlite::Connection, system_id: i64, choice: &EmulatorChoice) -> Result<(), String> {
    match choice {
        EmulatorChoice::None => repo::set_system_emulator(conn, system_id, None, None),
        EmulatorChoice::Retroarch { core } => repo::set_system_core(conn, system_id, core),
        EmulatorChoice::Standalone { emulator_id, path } => {
            let emulator = known_emulator(emulator_id).ok_or_else(|| format!("Unknown emulator {}", emulator_id))?;
            repo::set_system_emulator(conn, system_id, Some(path), Some(emulator.args))
        }
        EmulatorChoice::Custom { path, args } => repo::set_system_emulator(
            conn,
            system_id,
            path.as_deref().filter(|p| !p.is_empty()),
            args.as_deref().filter(|a| !a.is_empty()),
        ),
    }
    .map_err(|e| e.to_string())
}

pub(crate) struct Snapshot {
    pub(crate) systems: Vec<SystemDto>,
    dat_names: HashMap<i64, Vec<String>>,
    saved_retroarch: Option<PathBuf>,
}

pub(crate) fn snapshot(app: &tauri::AppHandle) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(Snapshot {
        systems: repo::list_systems(&conn).map_err(|e| e.to_string())?,
        dat_names: repo::dat_names_by_system(&conn).map_err(|e| e.to_string())?,
        saved_retroarch: saved_retroarch(&conn)?,
    })
}

/// Detection walks the disk, so it runs without holding the database lock.
pub(crate) fn environment_for(snapshot: &Snapshot) -> (Environment, bool) {
    let detection = detect::detect();
    let saved = snapshot.saved_retroarch.clone().filter(|p| p.is_file());
    let detected = saved.is_none() && detection.retroarch.is_some();
    (Environment::new(saved.or(detection.retroarch), detection.standalone), detected)
}

pub(crate) fn platforms_of(snapshot: &Snapshot, system: &SystemDto) -> Vec<String> {
    let dats = snapshot.dat_names.get(&system.id).cloned().unwrap_or_default();
    known::platform_names(&system.folder_name, &dats)
}

#[tauri::command]
pub async fn get_emulator_setup(app: tauri::AppHandle) -> Result<EmulatorSetupDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = snapshot(&app)?;
        let (env, detected) = environment_for(&snapshot);
        Ok(EmulatorSetupDto {
            retroarch_path: env.retroarch.as_ref().map(|r| r.exe.to_string_lossy().to_string()),
            retroarch_detected: detected,
            retroarch_cores_dir: env.retroarch.as_ref().map(|r| r.cores_dir.to_string_lossy().to_string()),
            installed_core_count: env.installed_cores.len(),
            standalone: env
                .standalone
                .iter()
                .map(|d| DetectedEmulatorDto { id: d.id.into(), name: d.name.into(), path: d.path.to_string_lossy().to_string() })
                .collect(),
            systems: snapshot
                .systems
                .iter()
                .map(|s| system_setup(s, &platforms_of(&snapshot, s), &env))
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn set_retroarch_path(path: Option<String>, app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => {
            if !Path::new(&p).is_file() {
                return Err(format!("Nothing found at {}.", p));
            }
            repo::set_setting(&conn, RETROARCH_SETTING, &p)
        }
        None => repo::set_setting(&conn, RETROARCH_SETTING, ""),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_system_emulator_choice(system_id: i64, choice: EmulatorChoice, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if matches!(choice, EmulatorChoice::Retroarch { .. }) {
            // A core is useless without RetroArch's location, so remember the
            // detected install the first time one is chosen.
            let saved = { saved_retroarch(&*state.db.lock().map_err(|e| e.to_string())?)? };
            if saved.filter(|p| p.is_file()).is_none() {
                if let Some(found) = detect::detect().retroarch {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    repo::set_setting(&conn, RETROARCH_SETTING, &found.to_string_lossy()).map_err(|e| e.to_string())?;
                }
            }
        }
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        apply_choice(&conn, system_id, &choice)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sets up every system that has no working emulator with the best one
/// available. Systems that already launch are left as they are.
#[tauri::command]
pub async fn auto_configure_emulators(app: tauri::AppHandle) -> Result<AutoConfigureSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = snapshot(&app)?;
        let (env, detected) = environment_for(&snapshot);
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        if detected {
            if let Some(ra) = &env.retroarch {
                repo::set_setting(&conn, RETROARCH_SETTING, &ra.exe.to_string_lossy()).map_err(|e| e.to_string())?;
            }
        }

        let mut summary = AutoConfigureSummary::default();
        for system in &snapshot.systems {
            let platforms = platforms_of(&snapshot, system);
            let setup = system_setup(system, &platforms, &env);
            if setup.kind != "none" && setup.problem.is_none() {
                summary.kept.push(system.name.clone());
                continue;
            }
            let Some((choice, label)) = auto_choice(&setup, &env, &platforms) else {
                summary.not_found.push(format!(
                    "{}: {}",
                    system.name,
                    if platforms.is_empty() { "unrecognised system folder" } else { "no emulator found" }
                ));
                continue;
            };
            // Don't swap one not-yet-installed core for another the user didn't pick.
            if let (EmulatorChoice::Retroarch { core }, Some(current)) = (&choice, setup.core.as_deref()) {
                if !env.installed_cores.contains(core) {
                    summary.cores_to_install.push(format!("{}: {}", system.name, env.core_name(current)));
                    summary.kept.push(system.name.clone());
                    continue;
                }
            }
            apply_choice(&conn, system.id, &choice)?;
            if let EmulatorChoice::Retroarch { core } = &choice {
                if !env.installed_cores.contains(core) {
                    summary.cores_to_install.push(format!("{}: {}", system.name, env.core_name(core)));
                }
            }
            summary.configured.push(format!("{}: {}", system.name, label));
        }
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(name: &str, core: Option<&str>, path: Option<&str>, args: Option<&str>) -> SystemDto {
        SystemDto {
            id: 1,
            name: name.into(),
            folder_name: name.into(),
            emulator_path: path.map(Into::into),
            emulator_args: args.map(Into::into),
            dat_url: None,
            has_dat: true,
            emulator_core: core.map(Into::into),
        }
    }

    fn core(id: &str, name: &str, platforms: &[&str]) -> CoreInfo {
        CoreInfo { id: id.into(), name: name.into(), platforms: platforms.iter().map(|p| p.to_string()).collect() }
    }

    fn env(installed: &[&str], standalone: Vec<DetectedEmulator>) -> Environment {
        let gb = "Nintendo - Game Boy";
        Environment {
            retroarch: Some(RetroArch::at(Path::new(r"D:\RetroArch\retroarch.exe"))),
            cores: vec![
                core("gambatte", "Gambatte", &[gb]),
                core("mgba", "mGBA", &[gb, "Nintendo - Game Boy Advance"]),
                core("tgbdual", "TGB Dual", &[gb]),
                core("dolphin", "Dolphin", &["Nintendo - GameCube"]),
            ],
            installed_cores: installed.iter().map(|c| c.to_string()).collect(),
            standalone,
        }
    }

    fn platforms(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn installed_cores_come_first_and_the_best_installed_one_is_recommended() {
        let e = env(&["tgbdual", "mgba"], vec![DetectedEmulator { id: "mgba", name: "mGBA", path: PathBuf::from(r"D:\mGBA.exe") }]);
        let setup = system_setup(&system("Game Boy", None, None, None), &platforms(&["Nintendo - Game Boy"]), &e);
        let labels: Vec<(&str, bool)> = setup.options.iter().map(|o| (o.label.as_str(), o.recommended)).collect();
        assert_eq!(
            labels,
            vec![
                ("RetroArch · mGBA", true),
                ("RetroArch · TGB Dual", false),
                ("mGBA", false),
                ("RetroArch · Gambatte (not installed)", false),
            ]
        );
    }

    #[test]
    fn cores_sharing_a_name_show_their_id() {
        let mut e = env(&[], vec![]);
        e.cores = vec![
            core("mupen64plus_next", "Mupen64Plus-Next", &["Nintendo - Nintendo 64"]),
            core("mupen64plus_next_gles3", "Mupen64Plus-Next", &["Nintendo - Nintendo 64"]),
        ];
        let setup = system_setup(&system("N64", None, None, None), &platforms(&["Nintendo - Nintendo 64"]), &e);
        assert_eq!(setup.options[1].label, "RetroArch · Mupen64Plus-Next (mupen64plus_next_gles3) (not installed)");
    }

    #[test]
    fn only_one_standalone_is_recommended_and_core_is_when_none_detected() {
        let e = env(
            &["dolphin"],
            vec![
                DetectedEmulator { id: "yuzu", name: "yuzu", path: PathBuf::from(r"D:\yuzu.exe") },
                DetectedEmulator { id: "ryujinx", name: "Ryujinx", path: PathBuf::from(r"D:\Ryujinx.exe") },
            ],
        );
        let switch = system_setup(&system("Switch", None, None, None), &platforms(&["Nintendo - Switch"]), &e);
        assert_eq!(switch.options.iter().map(|o| (o.label.as_str(), o.recommended)).collect::<Vec<_>>(), vec![("Ryujinx", true), ("yuzu", false)]);
        let gc = system_setup(&system("GameCube", None, None, None), &platforms(&["Nintendo - GameCube"]), &e);
        assert!(gc.options[0].recommended);
    }

    #[test]
    fn options_list_recommended_core_first() {
        let e = env(&[], vec![]);
        let setup = system_setup(&system("Game Boy", None, None, None), &platforms(&["Nintendo - Game Boy"]), &e);
        let labels: Vec<&str> = setup.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["RetroArch · Gambatte (not installed)", "RetroArch · mGBA (not installed)", "RetroArch · TGB Dual (not installed)"]
        );
        assert!(setup.options[0].recommended && !setup.options[1].recommended);
    }

    #[test]
    fn bare_retroarch_path_is_a_problem() {
        let e = env(&[], vec![]);
        let setup = system_setup(
            &system("Game Boy", None, Some(r"C:\Windows\notepad.exe"), None),
            &platforms(&["Nintendo - Game Boy"]),
            &e,
        );
        assert_eq!(setup.kind, "custom");
        assert_eq!(setup.problem, None);

        let bare = system_setup(
            &system("Game Boy", None, Some(r"D:\RetroArch\retroarch.exe"), None),
            &platforms(&["Nintendo - Game Boy"]),
            &e,
        );
        assert!(bare.problem.is_some());
    }

    #[test]
    fn uninstalled_core_explains_how_to_install() {
        let e = env(&[], vec![]);
        let setup = system_setup(&system("Game Boy", Some("gambatte"), None, None), &platforms(&["Nintendo - Game Boy"]), &e);
        assert_eq!(setup.kind, "retroarch");
        assert_eq!(
            setup.problem.as_deref(),
            Some("The Gambatte core isn't installed yet. In RetroArch: Main Menu → Online Updater → Core Downloader.")
        );
    }

    #[test]
    fn auto_prefers_an_installed_core_over_the_recommended_one() {
        let e = env(&["mgba"], vec![]);
        let p = platforms(&["Nintendo - Game Boy"]);
        let setup = system_setup(&system("Game Boy", None, None, None), &p, &e);
        let (choice, label) = auto_choice(&setup, &e, &p).unwrap();
        assert!(matches!(choice, EmulatorChoice::Retroarch { ref core } if core == "mgba"));
        assert_eq!(label, "RetroArch · mGBA");
    }

    #[test]
    fn auto_falls_back_to_recommended_core_when_none_installed() {
        let e = env(&[], vec![]);
        let p = platforms(&["Nintendo - Game Boy"]);
        let setup = system_setup(&system("Game Boy", None, None, None), &p, &e);
        let (choice, _) = auto_choice(&setup, &e, &p).unwrap();
        assert!(matches!(choice, EmulatorChoice::Retroarch { ref core } if core == "gambatte"));
    }

    #[test]
    fn auto_prefers_standalone_for_gamecube() {
        let e = env(&["dolphin"], vec![DetectedEmulator { id: "dolphin", name: "Dolphin", path: PathBuf::from(r"C:\Dolphin\Dolphin.exe") }]);
        let p = platforms(&["Nintendo - GameCube"]);
        let setup = system_setup(&system("GameCube", None, None, None), &p, &e);
        let (choice, label) = auto_choice(&setup, &e, &p).unwrap();
        assert!(matches!(choice, EmulatorChoice::Standalone { ref emulator_id, .. } if emulator_id == "dolphin"));
        assert_eq!(label, "Dolphin");
    }

    #[test]
    fn switch_gets_a_detected_standalone_and_unknown_folder_gets_nothing() {
        let e = env(&[], vec![DetectedEmulator { id: "ryujinx", name: "Ryujinx", path: PathBuf::from(r"D:\Switch\Ryujinx.exe") }]);
        let p = platforms(&["Nintendo - Switch"]);
        let setup = system_setup(&system("Switch", None, None, None), &p, &e);
        let (choice, _) = auto_choice(&setup, &e, &p).unwrap();
        assert!(matches!(choice, EmulatorChoice::Standalone { ref emulator_id, .. } if emulator_id == "ryujinx"));

        let none = system_setup(&system("Homebrew", None, None, None), &[], &e);
        assert!(auto_choice(&none, &e, &[]).is_none());
    }
}
