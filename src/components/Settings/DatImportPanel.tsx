import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";

export function DatImportPanel() {
  const { systems, refreshSystems, refreshRoms } = useLibrary();
  const [knownSource, setKnownSource] = useState<Record<number, boolean>>({});
  const [busy, setBusy] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      const entries = await Promise.all(
        systems.map(async (s) => [s.id, await api.hasKnownDatSource(s.folder_name)] as const),
      );
      setKnownSource(Object.fromEntries(entries));
    })();
  }, [systems]);

  async function importFile(systemId: number) {
    const filePath = await api.pickDatFile();
    if (!filePath) return;
    setBusy(systemId);
    setMessage(null);
    try {
      const summary = await api.importDatFile(systemId, filePath);
      setMessage(`Imported ${summary.games_imported} games / ${summary.roms_imported} roms.`);
      await refreshSystems();
      await refreshRoms();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function fetchDat(systemId: number) {
    setBusy(systemId);
    setMessage(null);
    try {
      const summary = await api.fetchDatFile(systemId);
      setMessage(`Fetched and imported ${summary.games_imported} games / ${summary.roms_imported} roms.`);
      await refreshSystems();
      await refreshRoms();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="settings-section">
      <h3>DAT Files</h3>
      <p className="hint">
        Import a No-Intro or Redump DAT file per system to identify your ROMs by hash. No-Intro
        DATs must be downloaded manually from Dat-o-Matic; some systems (e.g. Redump discs) can be
        fetched automatically.
      </p>
      <table className="settings-table">
        <thead>
          <tr>
            <th>System</th>
            <th>DAT</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {systems.map((s) => (
            <tr key={s.id}>
              <td>{s.name}</td>
              <td>
                {s.dat_name ? `${s.dat_name}${s.dat_version ? ` (${s.dat_version})` : ""}` : "Not imported"}
              </td>
              <td className="settings-row">
                <button disabled={busy === s.id} onClick={() => importFile(s.id)}>
                  Import DAT…
                </button>
                {knownSource[s.id] && (
                  <button disabled={busy === s.id} onClick={() => fetchDat(s.id)}>
                    Fetch DAT
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {message && <p className="hint">{message}</p>}
    </div>
  );
}
