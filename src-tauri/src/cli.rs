//! dexter-cli: run any API command from a terminal.
//!
//!   dexter-cli list-roms --filter.system_id 3
//!   dexter-cli launch-rom --rom-id 42
//!
//! If the app is open, the command is sent to its local API so the window
//! shows the change. Otherwise it runs here against the library directly.

use crate::api::{self, server, COMMANDS};
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Listener};

const USAGE: &str = "\
Usage: dexter-cli <command> [--arg value ...] [--local]

Arguments:
  --rom-id 42                    becomes {\"rom_id\": 42}
  --filter.system_id 3           nested: {\"filter\": {\"system_id\": 3}}
  --rom-ids [1,2,3]              values are read as JSON when they parse, else as text
  --json '{\"rom_id\": 42}'        all arguments as one JSON object
  --local                        run here even if the app is open

Commands:";

fn print_usage() {
    println!("{}", USAGE);
    for c in COMMANDS {
        println!("  {:<26}{}", c.name.replace('_', "-"), c.summary);
        if !c.args.is_empty() {
            println!("  {:<26}  args: {}", "", c.args);
        }
    }
}

fn parse_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

fn insert_path(map: &mut Map<String, Value>, key: &str, value: Value) -> Result<(), String> {
    match key.split_once('.') {
        None => {
            map.insert(key.to_string(), value);
            Ok(())
        }
        Some((head, rest)) => {
            let child = map.entry(head.to_string()).or_insert_with(|| Value::Object(Map::new()));
            match child {
                Value::Object(inner) => insert_path(inner, rest, value),
                _ => Err(format!("`--{}` conflicts with an earlier value for `{}`", key, head)),
            }
        }
    }
}

struct Invocation {
    command: String,
    args: Value,
    local: bool,
}

fn parse(argv: &[String]) -> Result<Option<Invocation>, String> {
    let Some((command, rest)) = argv.split_first() else {
        return Ok(None);
    };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        return Ok(None);
    }

    let mut args = Map::new();
    let mut local = false;
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i]
            .strip_prefix("--")
            .ok_or_else(|| format!("expected an --argument, got `{}`", rest[i]))?;
        if flag == "local" {
            local = true;
            i += 1;
            continue;
        }
        let value = rest.get(i + 1).filter(|v| !v.starts_with("--"));
        if flag == "json" {
            let raw = value.ok_or("--json needs a value")?;
            match serde_json::from_str(raw) {
                Ok(Value::Object(obj)) => args.extend(obj),
                _ => return Err("--json must be a JSON object".to_string()),
            }
        } else {
            let key = flag.replace('-', "_");
            insert_path(&mut args, &key, value.map(|v| parse_value(v)).unwrap_or(Value::Bool(true)))?;
        }
        i += if value.is_some() { 2 } else { 1 };
    }
    Ok(Some(Invocation { command: command.clone(), args: Value::Object(args), local }))
}

enum Remote {
    Done(Result<Value, String>),
    /// Nothing answered at the published address, e.g. api.json left behind
    /// by a crash.
    Unreachable,
}

fn call_running_app(info: &server::ApiInfo, command: &str, args: &Value) -> Remote {
    let client = match reqwest::blocking::Client::builder().timeout(None).build() {
        Ok(c) => c,
        Err(e) => return Remote::Done(Err(e.to_string())),
    };
    let url = format!("http://127.0.0.1:{}/api/{}", info.port, command.replace('-', "_"));
    let request = client
        .post(url)
        .bearer_auth(&info.token)
        .header("Content-Type", "application/json")
        .body(args.to_string());
    let response = match request.send() {
        Ok(r) => r,
        Err(e) if e.is_connect() => return Remote::Unreachable,
        Err(e) => return Remote::Done(Err(e.to_string())),
    };
    let body: Value = match response.text().map_err(|e| e.to_string()).and_then(|t| {
        serde_json::from_str(&t).map_err(|e| e.to_string())
    }) {
        Ok(v) => v,
        Err(e) => return Remote::Done(Err(format!("unreadable response from the app: {}", e))),
    };
    Remote::Done(match body.get("ok") {
        Some(Value::Bool(true)) => Ok(body.get("result").cloned().unwrap_or(Value::Null)),
        _ => Err(body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the app reported an error")
            .to_string()),
    })
}

fn run_locally(app: &AppHandle, command: &str, args: Value) -> Result<Value, String> {
    crate::init_state(app)?;
    let printed_progress = Arc::new(AtomicBool::new(false));
    for event in ["scan://progress", "hash://progress", "art://progress"] {
        let printed_progress = printed_progress.clone();
        app.listen_any(event, move |e| {
            if let Ok(p) = serde_json::from_str::<crate::models::ScanProgress>(e.payload()) {
                eprint!("\r{}/{} {:<60.60}", p.current, p.total, p.current_file);
                printed_progress.store(true, Ordering::Relaxed);
            }
        });
    }
    let result = tauri::async_runtime::block_on(api::dispatch(app, command, args));
    if printed_progress.load(Ordering::Relaxed) {
        eprint!("\r{:<80}\r", "");
    }
    result
}

pub fn run(argv: Vec<String>) -> i32 {
    let invocation = match parse(&argv) {
        Ok(Some(inv)) => inv,
        Ok(None) => {
            print_usage();
            return 0;
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return 2;
        }
    };
    if api::find(&invocation.command).is_none() {
        eprintln!("error: unknown command `{}`. Run `dexter-cli help` for the list.", invocation.command);
        return 2;
    }

    let app = match tauri::Builder::default().build(crate::context()) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("error: {}", e);
            return 1;
        }
    };
    let handle = app.handle().clone();

    let remote = match server::read_info(&handle) {
        Some(info) if !invocation.local => call_running_app(&info, &invocation.command, &invocation.args),
        _ => Remote::Unreachable,
    };
    let result = match remote {
        Remote::Done(result) => result,
        Remote::Unreachable => run_locally(&handle, &invocation.command, invocation.args),
    };

    match result {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            0
        }
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn no_command_or_help_shows_usage() {
        assert!(parse(&[]).unwrap().is_none());
        assert!(parse(&argv(&["help"])).unwrap().is_none());
    }

    #[test]
    fn flags_become_snake_case_json() {
        let inv = parse(&argv(&["launch-rom", "--rom-id", "42"])).unwrap().unwrap();
        assert_eq!(inv.command, "launch-rom");
        assert_eq!(inv.args, json!({ "rom_id": 42 }));
        assert!(!inv.local);
    }

    #[test]
    fn dotted_flags_nest() {
        let inv = parse(&argv(&["list-roms", "--filter.system_id", "3", "--filter.search_text", "mario"]))
            .unwrap()
            .unwrap();
        assert_eq!(inv.args, json!({ "filter": { "system_id": 3, "search_text": "mario" } }));
    }

    #[test]
    fn windows_paths_stay_text() {
        let inv = parse(&argv(&["import-dat-folder", "--folder-path", r"D:\DATs\No-Intro"])).unwrap().unwrap();
        assert_eq!(inv.args, json!({ "folder_path": r"D:\DATs\No-Intro" }));
    }

    #[test]
    fn json_arrays_and_local_flag() {
        let inv = parse(&argv(&["delete-roms", "--rom-ids", "[1,2]", "--local"])).unwrap().unwrap();
        assert_eq!(inv.args, json!({ "rom_ids": [1, 2] }));
        assert!(inv.local);
    }

    #[test]
    fn json_object_argument() {
        let inv = parse(&argv(&["get-rom-details", "--json", r#"{"rom_id": 7}"#])).unwrap().unwrap();
        assert_eq!(inv.args, json!({ "rom_id": 7 }));
    }

    #[test]
    fn positional_values_are_rejected() {
        assert!(parse(&argv(&["launch-rom", "42"])).is_err());
    }
}
