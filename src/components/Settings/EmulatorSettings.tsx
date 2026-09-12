import { useCallback, useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { AutoConfigureSummary, EmulatorChoice, EmulatorSetupDto, SystemEmulatorDto } from "../../types/emulators";

/** Select values: "none", "custom", "ra:<core>" or "sa:<emulator id>". */
function currentValue(s: SystemEmulatorDto): string {
  switch (s.kind) {
    case "retroarch":
      return `ra:${s.core}`;
    case "standalone":
      return `sa:${s.emulator_id}`;
    default:
      return s.kind;
  }
}

function SystemRow({
  system,
  setup,
  busy,
  onChoose,
}: {
  system: SystemEmulatorDto;
  setup: EmulatorSetupDto;
  busy: boolean;
  onChoose: (systemId: number, choice: EmulatorChoice) => Promise<void>;
}) {
  const [showCustom, setShowCustom] = useState(system.kind === "custom");
  const value = showCustom ? "custom" : currentValue(system);
  const standalone = system.options.filter((o) => o.kind === "standalone");
  const cores = system.options.filter((o) => o.kind === "retroarch");
  // A core picked earlier that this RetroArch doesn't list still needs to show.
  const unknownCore = system.kind === "retroarch" && !cores.some((o) => o.core === system.core);

  useEffect(() => setShowCustom(system.kind === "custom"), [system.kind]);

  function change(v: string) {
    if (v === "custom") {
      setShowCustom(true);
      return;
    }
    setShowCustom(false);
    if (v === "none") onChoose(system.system_id, { kind: "none" });
    else if (v.startsWith("ra:")) onChoose(system.system_id, { kind: "retroarch", core: v.slice(3) });
    else if (v.startsWith("sa:")) {
      const id = v.slice(3);
      const detected = setup.standalone.find((d) => d.id === id);
      if (detected) onChoose(system.system_id, { kind: "standalone", emulator_id: id, path: detected.path });
    }
  }

  async function browseCustom() {
    const path = await api.pickEmulatorPath();
    if (path) await onChoose(system.system_id, { kind: "custom", path, args: system.emulator_args });
  }

  return (
    <tr>
      <td className="emulator-system">{system.name}</td>
      <td>
        <select className="emulator-select" value={value} disabled={busy} onChange={(e) => change(e.target.value)}>
          <option value="none">Not configured</option>
          {standalone.length > 0 && (
            <optgroup label="Standalone emulators">
              {standalone.map((o) => (
                <option key={o.emulator_id} value={`sa:${o.emulator_id}`}>
                  {o.label}
                  {o.recommended ? " — recommended" : ""}
                </option>
              ))}
            </optgroup>
          )}
          {(cores.length > 0 || unknownCore) && (
            <optgroup label="RetroArch cores">
              {unknownCore && <option value={`ra:${system.core}`}>RetroArch · {system.core}</option>}
              {cores.map((o) => (
                <option key={o.core} value={`ra:${o.core}`}>
                  {o.label}
                  {o.recommended ? " — recommended" : ""}
                </option>
              ))}
            </optgroup>
          )}
          <option value="custom">Custom program…</option>
        </select>
        {showCustom && (
          <div className="emulator-custom">
            <div className="settings-row">
              <input type="text" readOnly value={system.emulator_path ?? ""} placeholder="Emulator program (.exe)" />
              <button onClick={browseCustom} disabled={busy}>
                Browse…
              </button>
            </div>
            <input
              type="text"
              key={`${system.system_id}-${system.emulator_args ?? ""}`}
              defaultValue={system.emulator_args ?? ""}
              placeholder="Arguments, e.g. -f %ROM%  (%ROM% is the game's path; added at the end if left out)"
              disabled={busy || !system.emulator_path}
              onBlur={(e) => {
                if (e.target.value !== (system.emulator_args ?? "")) {
                  onChoose(system.system_id, { kind: "custom", path: system.emulator_path, args: e.target.value || null });
                }
              }}
            />
          </div>
        )}
      </td>
      <td className="emulator-status">
        {system.problem ? (
          <span className="emulator-problem">{system.problem}</span>
        ) : system.kind === "none" ? (
          <span className="hint">
            {system.options.length === 0 ? "No emulator found for this system" : ""}
          </span>
        ) : (
          <span className="emulator-ok">Ready</span>
        )}
      </td>
    </tr>
  );
}

function describeSummary(summary: AutoConfigureSummary): string {
  const parts = [];
  if (summary.configured.length) parts.push(`Set up ${summary.configured.join(", ")}.`);
  if (summary.cores_to_install.length)
    parts.push(
      `Still to download in RetroArch (Main Menu → Online Updater → Core Downloader): ${summary.cores_to_install.join(", ")}.`,
    );
  if (summary.not_found.length) parts.push(`Nothing found for ${summary.not_found.join(", ")}.`);
  if (!summary.configured.length && !summary.cores_to_install.length && !summary.not_found.length)
    parts.push("Every system already has a working emulator.");
  return parts.join(" ");
}

export function EmulatorSettings() {
  const { refreshSystems } = useLibrary();
  const { startJob, endJob } = useJob();
  const [setup, setSetup] = useState<EmulatorSetupDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setSetup(await api.getEmulatorSetup());
    } catch (e) {
      setMessage(String(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function run(label: string, action: () => Promise<void>) {
    setBusy(true);
    startJob(label);
    try {
      await action();
      await load();
      await refreshSystems();
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setBusy(false);
    }
  }

  const choose = (systemId: number, choice: EmulatorChoice) =>
    run("Saving emulator", async () => {
      setMessage(null);
      await api.setSystemEmulatorChoice(systemId, choice);
    });

  const autoConfigure = () =>
    run("Setting up emulators", async () => {
      setMessage(describeSummary(await api.autoConfigureEmulators()));
    });

  const browseRetroarch = () =>
    run("Saving RetroArch location", async () => {
      const path = await api.pickEmulatorPath();
      if (path) await api.setRetroarchPath(path);
    });

  return (
    <div className="settings-section">
      <h3>Emulators</h3>
      <p className="hint">
        Choose how each system's games launch. RetroArch is set up once here, and each system just picks a core.
      </p>

      {setup === null ? (
        <p className="hint">
          <span className="spinner inline" aria-hidden="true" /> Looking for emulators on this PC…
        </p>
      ) : (
        <>
          <div className="retroarch-box">
            <div className="settings-row">
              <strong className="retroarch-label">RetroArch</strong>
              <input type="text" readOnly value={setup.retroarch_path ?? ""} placeholder="Not found" />
              <button onClick={browseRetroarch} disabled={busy}>
                Browse…
              </button>
            </div>
            <p className="hint">
              {setup.retroarch_path
                ? `${setup.retroarch_detected ? "Found on this PC. " : ""}${setup.installed_core_count} core${
                    setup.installed_core_count === 1 ? "" : "s"
                  } installed. Get more in RetroArch: Main Menu → Online Updater → Core Downloader.`
                : "RetroArch wasn't found. If it's installed, point to retroarch.exe."}
              {setup.standalone.length > 0 && ` Other emulators found: ${setup.standalone.map((d) => d.name).join(", ")}.`}
            </p>
          </div>

          <div className="settings-row tools-actions">
            <button onClick={autoConfigure} disabled={busy}>
              {busy && <span className="spinner inline" aria-hidden="true" />}
              Set up automatically
            </button>
            <button onClick={() => run("Looking for emulators", async () => {})} disabled={busy}>
              Look again
            </button>
            <span className="hint">Fills in every system that isn't ready, using the best emulator available.</span>
          </div>
          {message && <p className="hint emulator-message">{message}</p>}

          <table className="settings-table emulator-table">
            <thead>
              <tr>
                <th>System</th>
                <th>Emulator</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {setup.systems.map((s) => (
                <SystemRow key={s.system_id} system={s} setup={setup} busy={busy} onChoose={choose} />
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
