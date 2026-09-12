use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemDto {
    pub id: i64,
    pub name: String,
    pub folder_name: String,
    pub emulator_path: Option<String>,
    pub emulator_args: Option<String>,
    pub dat_url: Option<String>,
    pub has_dat: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatSourceDto {
    pub id: i64,
    pub system_id: i64,
    pub file_name: String,
    pub dat_name: Option<String>,
    pub dat_version: Option<String>,
    pub imported_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Settings {
    pub rom_root_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatImportSummary {
    pub games_imported: i64,
    pub roms_imported: i64,
    pub newly_matched: i64,
    pub dat_name: Option<String>,
    pub dat_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DatFolderImportedEntry {
    pub newly_matched: i64,
    pub file_name: String,
    pub system_name: String,
    pub games_imported: i64,
    pub roms_imported: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DatFolderImportSummary {
    pub imported: Vec<DatFolderImportedEntry>,
    pub unmatched: Vec<String>,
    pub errors: Vec<String>,
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
    pub metadata_guessed: bool,
    pub match_status: String,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub emulator_path: Option<String>,
    pub emulator_args: Option<String>,
    /// Bytes of header detected at the start of the file, if any.
    pub header_size: Option<i64>,
    pub headerless_crc32: Option<String>,
    /// Bytes past the ROM data at the end of the file, also skipped for matching.
    pub trailer_size: Option<i64>,
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
    pub pending: i64,
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ArtFetchSummary {
    pub attempted: i64,
    pub downloaded: i64,
    pub not_found: i64,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateFileDto {
    pub id: i64,
    pub display_name: String,
    pub file_name: String,
    pub file_path: String,
    pub archive_member: Option<String>,
    pub system_name: Option<String>,
    pub size: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DuplicateGroupDto {
    pub sha1: String,
    pub files: Vec<DuplicateFileDto>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RenamePlanEntryDto {
    pub rom_id: i64,
    pub current_name: String,
    pub new_name: String,
    /// Absolute path of the file that would actually be renamed. For a ROM
    /// inside an archive this is the archive, not the member.
    pub target_path: String,
    /// Set when the entry can't be renamed; the UI shows it and disables it.
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MaintenanceSummary {
    pub succeeded: i64,
    pub skipped: i64,
    pub errors: Vec<String>,
}
