import { invoke } from "@tauri-apps/api/core";
import type {
  DatImportSummary,
  RomDetailsDto,
  RomFilter,
  RomListItemDto,
  ScanSummary,
  SystemDto,
} from "../types/rom";
import type { Settings } from "../types/settings";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  pickRomRootFolder: () => invoke<string | null>("pick_rom_root_folder"),
  pickEmulatorPath: () => invoke<string | null>("pick_emulator_path"),

  listSystems: () => invoke<SystemDto[]>("list_systems"),
  setSystemEmulator: (systemId: number, emulatorPath: string | null, emulatorArgs: string | null) =>
    invoke<void>("set_system_emulator", { systemId, emulatorPath, emulatorArgs }),

  pickDatFile: () => invoke<string | null>("pick_dat_file"),
  importDatFile: (systemId: number, filePath: string) =>
    invoke<DatImportSummary>("import_dat_file", { systemId, filePath }),
  fetchDatFile: (systemId: number) => invoke<DatImportSummary>("fetch_dat_file", { systemId }),
  hasKnownDatSource: (folderName: string) => invoke<boolean>("has_known_dat_source", { folderName }),

  scanLibrary: () => invoke<ScanSummary>("scan_library"),
  hashPendingRoms: () => invoke<ScanSummary>("hash_pending_roms"),
  cancelScan: () => invoke<void>("cancel_scan"),

  listRoms: (filter: RomFilter) => invoke<RomListItemDto[]>("list_roms", { filter }),
  getRomDetails: (romId: number) => invoke<RomDetailsDto | null>("get_rom_details", { romId }),
};
