export interface StorageLocationsDto {
  library_folder: string;
  library_is_default: boolean;
  art_folder: string;
  art_is_default: boolean;
  /** Holds api.json, the window state and locations.json, which don't move. */
  app_folder: string;
}

export interface Settings {
  rom_root_path: string | null;
}
