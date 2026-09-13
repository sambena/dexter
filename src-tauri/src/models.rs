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
    /// RetroArch core the system launches with, if it uses RetroArch.
    pub emulator_core: Option<String>,
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

/// Where the library database and box art are kept (see storage).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageLocationsDto {
    pub library_folder: String,
    pub library_is_default: bool,
    pub art_folder: String,
    pub art_is_default: bool,
    /// Holds api.json, the window state and locations.json, which don't move.
    pub app_folder: String,
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
    pub emulator_core: Option<String>,
    /// How a match was found when the file isn't byte-for-byte the DAT's
    /// dump: "overdump", "header", "mirrored" (see scanner::repair) or
    /// "cue-tracks" (a cue sheet whose tracks all matched).
    pub match_note: Option<String>,
    /// From a title folder's own XML (Wii U), when it has one.
    pub title_id: Option<String>,
    pub title_version: Option<i64>,
    /// "game", "update", "dlc" or "demo".
    pub title_kind: Option<String>,
    pub product_code: Option<String>,
    /// The name and region come from a DAT game found by title, not by hash:
    /// the file is identified but not verified.
    pub identified_by_title: bool,
    /// The name and region come from the DAT game the file's name says it is;
    /// its hash matches no dump of that game.
    pub named_from_file_name: bool,
    /// The hash match status underneath `match_status`, which shows
    /// "identified" for files recognised from their own title information.
    pub hash_status: String,
    /// The box art shown was looked up by the file's name, not a DAT entry,
    /// so it may be for another release.
    pub box_art_guessed: bool,
}

/// One way to run a system, as offered in the emulator dropdown.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmulatorOptionDto {
    /// "retroarch" or "standalone".
    pub kind: String,
    /// RetroArch core id, for kind "retroarch".
    pub core: Option<String>,
    /// Known emulator id, for kind "standalone".
    pub emulator_id: Option<String>,
    pub label: String,
    /// False for a RetroArch core that isn't downloaded yet.
    pub installed: bool,
    pub recommended: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemEmulatorDto {
    pub system_id: i64,
    pub name: String,
    /// "none", "retroarch", "standalone" or "custom".
    pub kind: String,
    pub core: Option<String>,
    pub emulator_id: Option<String>,
    pub emulator_path: Option<String>,
    pub emulator_args: Option<String>,
    pub options: Vec<EmulatorOptionDto>,
    /// Why the current setup won't launch, if it won't.
    pub problem: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetectedEmulatorDto {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmulatorSetupDto {
    pub retroarch_path: Option<String>,
    /// The path was found on this PC rather than saved in settings.
    pub retroarch_detected: bool,
    pub retroarch_cores_dir: Option<String>,
    pub installed_core_count: usize,
    pub standalone: Vec<DetectedEmulatorDto>,
    pub systems: Vec<SystemEmulatorDto>,
}

/// What set_system_emulator_choice should do.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum EmulatorChoice {
    None,
    Retroarch { core: String },
    Standalone { emulator_id: String, path: String },
    Custom { path: Option<String>, args: Option<String> },
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AutoConfigureSummary {
    /// "System: choice" for each system set up.
    pub configured: Vec<String>,
    /// Systems left alone because they already launch.
    pub kept: Vec<String>,
    /// "System: reason" for systems nothing was found for.
    pub not_found: Vec<String>,
    /// Cores chosen that RetroArch still needs to download.
    pub cores_to_install: Vec<String>,
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
    /// Found, but in a form hashing can't verify (see scanner::formats).
    pub unverifiable: i64,
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
    /// Of `downloaded`, images for unmatched files looked up by file name.
    #[serde(default)]
    pub guessed: i64,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RetroArchExportSummary {
    pub playlists_dir: String,
    /// "Playlist name: N games" for each playlist written.
    pub playlists: Vec<String>,
    pub games: i64,
    /// "System: reason" for systems left out.
    pub skipped_systems: Vec<String>,
    pub thumbnails_copied: i64,
    /// Already in RetroArch's thumbnails folder from an earlier export.
    pub thumbnails_unchanged: i64,
    pub without_art: i64,
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
    /// A key for the group: the shared SHA1, or a title-based key.
    pub sha1: String,
    pub files: Vec<DuplicateFileDto>,
    /// "identical": the same bytes. "same-title": title folders or discs with
    /// the same ID and version, which can't be compared byte for byte.
    /// "unverified-copy": verified copies of a game, then unmatched files
    /// named as that game (usually altered or bad dumps of it).
    pub kind: String,
    /// For "unverified-copy" groups, how many of `files` (from the start)
    /// are the verified copies. 0 for other kinds.
    pub verified_count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RenamePlanEntryDto {
    pub rom_id: i64,
    pub current_name: String,
    pub new_name: String,
    /// Absolute path of the file that would actually be renamed. For a ROM
    /// inside an archive this is the archive, not the member.
    pub target_path: String,
    /// Save files renamed along with it, by current name.
    pub also_renames: Vec<String>,
    /// Set when the entry can't be renamed; the UI shows it and disables it.
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MaintenanceSummary {
    pub succeeded: i64,
    pub skipped: i64,
    pub errors: Vec<String>,
    /// Where deleted files without a Recycle Bin (network shares, removable
    /// drives) were moved, if any were.
    #[serde(default)]
    pub moved_to: Vec<String>,
}
