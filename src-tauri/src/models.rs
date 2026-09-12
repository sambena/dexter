use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemDto {
    pub id: i64,
    pub name: String,
    pub folder_name: String,
    pub emulator_path: Option<String>,
    pub emulator_args: Option<String>,
    pub dat_name: Option<String>,
    pub dat_version: Option<String>,
    pub dat_imported_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Settings {
    pub rom_root_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatImportSummary {
    pub games_imported: i64,
    pub roms_imported: i64,
    pub dat_name: Option<String>,
    pub dat_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RomListItemDto {
    pub id: i64,
    pub file_name: String,
    pub system_id: Option<i64>,
    pub system_name: Option<String>,
    pub display_name: String,
    pub match_status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RomDetailsDto {
    pub id: i64,
    pub file_name: String,
    pub file_path: String,
    pub archive_member: Option<String>,
    pub system_id: Option<i64>,
    pub system_name: Option<String>,
    pub year: Option<String>,
    pub region: Option<String>,
    pub display_name: String,
    pub match_status: String,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub emulator_path: Option<String>,
    pub emulator_args: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RomFilter {
    pub search_text: Option<String>,
    pub system_id: Option<i64>,
    pub match_status: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ScanSummary {
    pub scanned_files: i64,
    pub matched: i64,
    pub unmatched: i64,
    pub removed: i64,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
}
