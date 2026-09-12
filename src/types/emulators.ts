export interface EmulatorOptionDto {
  kind: "retroarch" | "standalone";
  core: string | null;
  emulator_id: string | null;
  label: string;
  installed: boolean;
  recommended: boolean;
}

export interface SystemEmulatorDto {
  system_id: number;
  name: string;
  kind: "none" | "retroarch" | "standalone" | "custom";
  core: string | null;
  emulator_id: string | null;
  emulator_path: string | null;
  emulator_args: string | null;
  options: EmulatorOptionDto[];
  problem: string | null;
}

export interface DetectedEmulatorDto {
  id: string;
  name: string;
  path: string;
}

export interface EmulatorSetupDto {
  retroarch_path: string | null;
  retroarch_detected: boolean;
  retroarch_cores_dir: string | null;
  installed_core_count: number;
  standalone: DetectedEmulatorDto[];
  systems: SystemEmulatorDto[];
}

export type EmulatorChoice =
  | { kind: "none" }
  | { kind: "retroarch"; core: string }
  | { kind: "standalone"; emulator_id: string; path: string }
  | { kind: "custom"; path: string | null; args: string | null };

export interface AutoConfigureSummary {
  configured: string[];
  kept: string[];
  not_found: string[];
  cores_to_install: string[];
}
