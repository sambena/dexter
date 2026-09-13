export interface SystemDto {
  id: number;
  name: string;
  folder_name: string;
  emulator_path: string | null;
  emulator_args: string | null;
  dat_url: string | null;
  has_dat: boolean;
  emulator_core: string | null;
}

export interface DatSourceDto {
  id: number;
  system_id: number;
  file_name: string;
  dat_name: string | null;
  dat_version: string | null;
  imported_at: string;
}

export interface RomListItemDto {
  id: number;
  file_name: string;
  system_id: number | null;
  system_name: string | null;
  display_name: string;
  match_status: "matched" | "unmatched" | "unverifiable" | "pending" | "error" | string;
}

export interface RomDetailsDto {
  id: number;
  file_name: string;
  file_path: string;
  archive_member: string | null;
  system_id: number | null;
  system_name: string | null;
  year: string | null;
  region: string | null;
  display_name: string;
  metadata_guessed: boolean;
  match_status: string;
  crc32: string | null;
  md5: string | null;
  sha1: string | null;
  emulator_path: string | null;
  emulator_args: string | null;
  header_size: number | null;
  headerless_crc32: string | null;
  trailer_size: number | null;
  emulator_core: string | null;
  /** How a match was found when the file isn't the DAT's exact dump. */
  match_note: "overdump" | "header" | "mirrored" | "trimmed" | "interleaved" | "cue-tracks" | null;
  /** From a title folder's own XML (Wii U). */
  title_id: string | null;
  title_version: number | null;
  title_kind: "game" | "update" | "dlc" | "demo" | null;
  product_code: string | null;
  /** Named from a DAT game found by title, not verified by hash. */
  identified_by_title: boolean;
  /** The box art was looked up by file name, not a DAT entry: it may be another release. */
  box_art_guessed: boolean;
}

export interface RomFilter {
  search_text?: string;
  system_id?: number;
  match_status?: string;
  sort_by?: "name" | "system";
  sort_dir?: "asc" | "desc";
}

export interface ScanSummary {
  scanned_files: number;
  pending: number;
  matched: number;
  unmatched: number;
  unverifiable: number;
  removed: number;
  errors: string[];
}

export interface ScanProgress {
  current: number;
  total: number;
  current_file: string;
}

export interface ArtFetchSummary {
  attempted: number;
  downloaded: number;
  not_found: number;
  /** Of downloaded, images for unmatched files looked up by file name. */
  guessed: number;
  errors: string[];
}

export interface DatImportSummary {
  games_imported: number;
  roms_imported: number;
  newly_matched: number;
  dat_name: string | null;
  dat_version: string | null;
}

export interface DatFolderImportedEntry {
  file_name: string;
  system_name: string;
  games_imported: number;
  roms_imported: number;
  newly_matched: number;
}

export interface DatFolderImportSummary {
  imported: DatFolderImportedEntry[];
  unmatched: string[];
  errors: string[];
}

export interface DuplicateFileDto {
  id: number;
  display_name: string;
  file_name: string;
  file_path: string;
  archive_member: string | null;
  system_name: string | null;
  size: number | null;
}

export interface DuplicateGroupDto {
  sha1: string;
  files: DuplicateFileDto[];
  /**
   * "identical": same bytes. "same-title": title folders or discs with the same
   * ID and version. "unverified-copy": verified copies of a game first, then
   * unmatched files named as that game.
   */
  kind: "identical" | "same-title" | "unverified-copy";
  /** For "unverified-copy" groups: how many leading files are verified. */
  verified_count: number;
}

export interface RenamePlanEntryDto {
  rom_id: number;
  current_name: string;
  new_name: string;
  target_path: string;
  also_renames: string[];
  blocked_reason: string | null;
}

export interface MaintenanceSummary {
  succeeded: number;
  skipped: number;
  errors: string[];
  /** Folders deleted files were moved to, for files with no Recycle Bin (network shares). */
  moved_to: string[];
}
