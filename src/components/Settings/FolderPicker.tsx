import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";

export function FolderPicker() {
  const { settings, refreshSettings, refreshRoms, refreshSystems } = useLibrary();

  async function browse() {
    const folder = await api.pickRomRootFolder();
    if (folder) {
      await api.saveSettings({ rom_root_path: folder });
      await refreshSettings();
      await refreshSystems();
      await refreshRoms();
    }
  }

  return (
    <div className="settings-section">
      <h3>ROM Library</h3>
      <div className="settings-row">
        <input type="text" readOnly value={settings.rom_root_path ?? "Not set"} />
        <button onClick={browse}>Browse…</button>
      </div>
      <p className="hint">
        Pick the root folder that contains one subfolder per system (e.g. Roms/NES, Roms/SNES).
      </p>
    </div>
  );
}
