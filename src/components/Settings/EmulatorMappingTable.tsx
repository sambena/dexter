import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";

export function EmulatorMappingTable() {
  const { systems, refreshSystems } = useLibrary();

  async function browseEmulator(systemId: number, currentArgs: string | null) {
    const path = await api.pickEmulatorPath();
    if (path) {
      await api.setSystemEmulator(systemId, path, currentArgs);
      await refreshSystems();
    }
  }

  async function updateArgs(systemId: number, currentPath: string | null, args: string) {
    await api.setSystemEmulator(systemId, currentPath, args || null);
    await refreshSystems();
  }

  return (
    <div className="settings-section">
      <h3>Emulators</h3>
      <p className="hint">
        Map each system to the emulator that Play launches its ROMs with. In Args, %ROM% marks where the ROM path
        goes; if it's left out, the path is added at the end.
      </p>
      <table className="settings-table">
        <thead>
          <tr>
            <th>System</th>
            <th>Emulator path</th>
            <th>Args</th>
          </tr>
        </thead>
        <tbody>
          {systems.map((s) => (
            <tr key={s.id}>
              <td>{s.name}</td>
              <td className="settings-row">
                <input type="text" readOnly value={s.emulator_path ?? ""} placeholder="Not configured" />
                <button onClick={() => browseEmulator(s.id, s.emulator_args)}>Browse…</button>
              </td>
              <td>
                <input
                  type="text"
                  defaultValue={s.emulator_args ?? ""}
                  placeholder="%ROM%"
                  onBlur={(e) => updateArgs(s.id, s.emulator_path, e.target.value)}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
