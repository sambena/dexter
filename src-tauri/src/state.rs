use rusqlite::Connection;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Mutex<Connection>,
    /// Shared by scan_library and hash_pending_roms — set by cancel_scan to stop
    /// whichever one is currently running as soon as it next checks.
    pub cancel_flag: Arc<AtomicBool>,
}
