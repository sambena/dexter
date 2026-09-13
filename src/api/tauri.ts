import { invoke } from "@tauri-apps/api/core";
import type {
  ArtFetchSummary,
  DatFolderImportSummary,
  DatImportSummary,
  DatSourceDto,
  DuplicateGroupDto,
  MaintenanceSummary,
  RenamePlanEntryDto,
  RomDetailsDto,
  RomFilter,
  RomListItemDto,
  ScanSummary,
  SystemDto,
} from "../types/rom";
import type { Settings, StorageLocationsDto } from "../types/settings";
import type { AutoConfigureSummary, EmulatorChoice, EmulatorSetupDto } from "../types/emulators";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  pickRomRootFolder: () => invoke<string | null>("pick_rom_root_folder"),
  pickEmulatorPath: () => invoke<string | null>("pick_emulator_path"),
  getStorageLocations: () => invoke<StorageLocationsDto>("get_storage_locations"),
  moveLibraryDatabase: (folder: string | null) => invoke<StorageLocationsDto>("move_library_database", { folder }),
  moveBoxArt: (folder: string | null) => invoke<MaintenanceSummary>("move_box_art", { folder }),

  listSystems: () => invoke<SystemDto[]>("list_systems"),
  setSystemEmulator: (systemId: number, emulatorPath: string | null, emulatorArgs: string | null) =>
    invoke<void>("set_system_emulator", { systemId, emulatorPath, emulatorArgs }),
  getEmulatorSetup: () => invoke<EmulatorSetupDto>("get_emulator_setup"),
  setRetroarchPath: (path: string | null) => invoke<void>("set_retroarch_path", { path }),
  setSystemEmulatorChoice: (systemId: number, choice: EmulatorChoice) =>
    invoke<void>("set_system_emulator_choice", { systemId, choice }),
  autoConfigureEmulators: () => invoke<AutoConfigureSummary>("auto_configure_emulators"),

  pickDatFile: () => invoke<string | null>("pick_dat_file"),
  importDatFile: (systemId: number, filePath: string) =>
    invoke<DatImportSummary>("import_dat_file", { systemId, filePath }),
  pickDatFolder: () => invoke<string | null>("pick_dat_folder"),
  importDatFolder: (folderPath: string) => invoke<DatFolderImportSummary>("import_dat_folder", { folderPath }),
  fetchDatFile: (systemId: number) => invoke<DatImportSummary>("fetch_dat_file", { systemId }),
  hasKnownDatSource: (folderName: string) => invoke<boolean>("has_known_dat_source", { folderName }),
  setSystemDatUrl: (systemId: number, datUrl: string | null) => invoke<void>("set_system_dat_url", { systemId, datUrl }),
  listDatSources: () => invoke<DatSourceDto[]>("list_dat_sources"),
  removeDatSource: (datSourceId: number) => invoke<void>("remove_dat_source", { datSourceId }),

  scanLibrary: () => invoke<ScanSummary>("scan_library"),
  hashPendingRoms: () => invoke<ScanSummary>("hash_pending_roms"),
  cancelScan: () => invoke<void>("cancel_scan"),

  listRoms: (filter: RomFilter) => invoke<RomListItemDto[]>("list_roms", { filter }),
  getRomDetails: (romId: number) => invoke<RomDetailsDto | null>("get_rom_details", { romId }),
  launchRom: (romId: number) => invoke<void>("launch_rom", { romId }),

  listDuplicates: () => invoke<DuplicateGroupDto[]>("list_duplicates"),
  previewRenames: () => invoke<RenamePlanEntryDto[]>("preview_renames"),
  applyRenames: (romIds: number[]) => invoke<MaintenanceSummary>("apply_renames", { romIds }),
  deleteRoms: (romIds: number[]) => invoke<MaintenanceSummary>("delete_roms", { romIds }),

  hasKnownBoxArtSource: (folderName: string) => invoke<boolean>("has_known_box_art_source", { folderName }),
  getBoxArt: (romId: number) => invoke<string | null>("get_box_art", { romId }),
  fetchBoxArt: (romId: number) => invoke<string>("fetch_box_art", { romId }),
  fetchAllBoxArt: () => invoke<ArtFetchSummary>("fetch_all_box_art"),
  pickAndSetBoxArt: (romId: number) => invoke<string>("pick_and_set_box_art", { romId }),
};
