//! Local HTTP API, served while the app is open.
//!
//!   GET  /api/commands        list commands
//!   POST /api/<command>       run a command; the body is its JSON arguments
//!
//! It listens on 127.0.0.1 only, and every request needs
//! `Authorization: Bearer <token>`. The port and a fresh random token are
//! written to api.json in the app data folder on each start, so only
//! something that can already read this user's files can call it. That also
//! stops a web page from driving it via the browser, since it can't learn the
//! token.

use super::{dispatch, COMMANDS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Method, Request, Response, Server};

/// Tried first so the address stays stable between runs; any free port is
/// used if it's taken.
const PREFERRED_PORT: u16 = 17420;
const MAX_BODY_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub struct ApiInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

pub fn info_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("api.json"))
}

pub fn read_info(app: &AppHandle) -> Option<ApiInfo> {
    let text = std::fs::read_to_string(info_path(app).ok()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Starts the server on a background thread and publishes api.json.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let server = Server::http(("127.0.0.1", PREFERRED_PORT))
        .or_else(|_| Server::http(("127.0.0.1", 0)))
        .map_err(|e| format!("couldn't start the local API: {}", e))?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .ok_or("local API has no TCP address")?;

    let info = ApiInfo { port, token: new_token()?, pid: std::process::id() };
    let text = serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?;
    std::fs::write(info_path(app)?, text).map_err(|e| e.to_string())?;

    let app = app.clone();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let app = app.clone();
            let token = info.token.clone();
            // One thread per request, so a long scan doesn't block cancel_scan.
            std::thread::spawn(move || handle(request, &app, &token));
        }
    });
    Ok(())
}

/// Removes api.json, but only if it still describes this process — a second
/// instance may have replaced it.
pub fn stop(app: &AppHandle) {
    if read_info(app).is_some_and(|i| i.pid == std::process::id()) {
        if let Ok(path) = info_path(app) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn tokens_match(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn is_authorized(request: &Request, token: &str) -> bool {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
        .is_some_and(|given| tokens_match(given.trim(), token))
}

fn respond(request: Request, status: u16, body: Value) {
    let header = Header::from_bytes("Content-Type", "application/json").expect("static header");
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header);
    let _ = request.respond(response);
}

fn handle(mut request: Request, app: &AppHandle, token: &str) {
    if !is_authorized(&request, token) {
        return respond(request, 401, json!({ "ok": false, "error": "missing or wrong token" }));
    }

    let path = request.url().split('?').next().unwrap_or("").to_string();
    let Some(command) = path.strip_prefix("/api/").map(str::to_string) else {
        return respond(request, 404, json!({ "ok": false, "error": "not found" }));
    };

    match (request.method(), command.as_str()) {
        (Method::Get, "commands") => {
            let list: Vec<Value> = COMMANDS
                .iter()
                .map(|c| json!({ "name": c.name, "args": c.args, "summary": c.summary }))
                .collect();
            respond(request, 200, json!({ "ok": true, "result": list }))
        }
        (Method::Post, _) => {
            let mut body = String::new();
            if let Err(e) = request.as_reader().take(MAX_BODY_BYTES).read_to_string(&mut body) {
                return respond(request, 400, json!({ "ok": false, "error": e.to_string() }));
            }
            let args = if body.trim().is_empty() {
                Value::Null
            } else {
                match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(e) => {
                        return respond(request, 400, json!({ "ok": false, "error": format!("invalid JSON: {}", e) }))
                    }
                }
            };
            if super::find(&command).is_none() {
                return respond(request, 404, json!({ "ok": false, "error": format!("unknown command `{}`", command) }));
            }
            match tauri::async_runtime::block_on(dispatch(app, &command, args)) {
                Ok(result) => respond(request, 200, json!({ "ok": true, "result": result })),
                Err(error) => respond(request, 400, json!({ "ok": false, "error": error })),
            }
        }
        _ => respond(request, 405, json!({ "ok": false, "error": "use GET /api/commands or POST /api/<command>" })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison() {
        assert!(tokens_match("abc", "abc"));
        assert!(!tokens_match("abc", "abd"));
        assert!(!tokens_match("abc", "abcd"));
    }

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
