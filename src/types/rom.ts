export interface SystemDto {
  id: number;
  name: string;
  folder_name: string;
  emulator_path: string | null;
  emulator_args: string | null;
  dat_url: string | null;
  has_dat: boolean;
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
  match_status: "matched" | "unmatched" | "pending" | "error" | string;
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
}

export interface RenamePlanEntryDto {
  rom_id: number;
  current_name: string;
  new_name: string;
  target_path: string;
  blocked_reason: string | null;
}

export interface MaintenanceSummary {
  succeeded: number;
  skipped: number;
  errors: string[];
}
