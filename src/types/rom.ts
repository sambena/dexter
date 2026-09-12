export interface SystemDto {
  id: number;
  name: string;
  folder_name: string;
  emulator_path: string | null;
  emulator_args: string | null;
  dat_name: string | null;
  dat_version: string | null;
  dat_imported_at: string | null;
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

export interface DatImportSummary {
  games_imported: number;
  roms_imported: number;
  dat_name: string | null;
  dat_version: string | null;
}
