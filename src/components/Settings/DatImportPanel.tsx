import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { DatFolderImportSummary, DatSourceDto } from "../../types/rom";

// Systems where importing more than one DAT actually matters — surfaced right
// in the UI so this isn't something you only find out by asking.
const MULTI_VARIANT_SYSTEM_HINTS: Record<string, string> = {
  n64: "No-Intro splits this into separate BigEndian/ByteSwapped/LittleEndian DATs — the same games hash differently in each, so import more than one for full coverage.",
  nintendo64: "No-Intro splits this into separate BigEndian/ByteSwapped/LittleEndian DATs — the same games hash differently in each, so import more than one for full coverage.",
};

function normalizeFolderKey(folderName: string): string {
  return folderName.toLowerCase().replace(/[^a-z0-9]/g, "");
}

export function DatImportPanel() {
  const { systems, refreshSystems, refreshRoms } = useLibrary();
  const { startJob, endJob } = useJob();
  const [datSources, setDatSources] = useState<DatSourceDto[]>([]);
  const [knownSource, setKnownSource] = useState<Record<number, boolean>>({});
  const [busy, setBusy] = useState<number | null>(null);
  const [removingId, setRemovingId] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [folderBusy, setFolderBusy] = useState(false);
  const [folderResult, setFolderResult] = useState<DatFolderImportSummary | null>(null);
  const [lastFolderPath, setLastFolderPath] = useState<string | null>(null);

  async function refreshDatSources() {
    setDatSources(await api.listDatSources());
  }

  useEffect(() => {
    refreshDatSources();
  }, [systems]);

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
    startJob("Importing DAT");
    try {
      const summary = await api.importDatFile(systemId, filePath);
      setMessage(
        `Imported ${summary.games_imported} games / ${summary.roms_imported} roms — ${summary.newly_matched} ROMs in your library newly matched.`,
      );
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setBusy(null);
      // Always refresh, even on failure — a partially-applied change (or just
      // stale data) should never require closing/reopening Settings to see.
      await refreshSystems();
      await refreshDatSources();
      await refreshRoms();
    }
  }

  async function fetchDat(systemId: number) {
    setBusy(systemId);
    setMessage(null);
    startJob("Fetching DAT");
    try {
      const summary = await api.fetchDatFile(systemId);
      setMessage(
        `Fetched and imported ${summary.games_imported} games / ${summary.roms_imported} roms — ${summary.newly_matched} ROMs in your library newly matched.`,
      );
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setBusy(null);
      await refreshSystems();
      await refreshDatSources();
      await refreshRoms();
    }
  }

  async function removeSource(datSourceId: number) {
    setRemovingId(datSourceId);
    startJob("Removing DAT and re-matching");
    try {
      await api.removeDatSource(datSourceId);
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setRemovingId(null);
      await refreshSystems();
      await refreshDatSources();
      await refreshRoms();
    }
  }

  async function saveDatUrl(systemId: number, url: string) {
    await api.setSystemDatUrl(systemId, url.trim() || null);
    await refreshSystems();
  }

  async function importFolder() {
    const folderPath = await api.pickDatFolder();
    if (!folderPath) return;
    setLastFolderPath(folderPath);
    setFolderBusy(true);
    setFolderResult(null);
    setMessage(null);
    startJob("Importing DAT folder");
    try {
      const result = await api.importDatFolder(folderPath);
      setFolderResult(result);
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setFolderBusy(false);
      await refreshSystems();
      await refreshDatSources();
      await refreshRoms();
    }
  }

  return (
    <div className="settings-section">
      <h3>DAT Files</h3>
      <p className="hint">
        Import a No-Intro or Redump DAT file per system to identify your ROMs by hash. A system can
        have more than one DAT imported — matching checks all of them, which matters for systems
        split into multiple variants (see the note under Nintendo 64 below). Redump systems can
        fetch automatically out of the box; for anything else, paste a direct DAT download URL if
        you have one, or just import a file manually.
      </p>
      <div className="settings-row">
        <input type="text" readOnly value={lastFolderPath ?? "No folder imported yet"} />
        <button disabled={folderBusy} onClick={importFolder}>
          {folderBusy ? (
            <>
              <span className="spinner inline" aria-hidden="true" />
              Importing…
            </>
          ) : (
            "Browse…"
          )}
        </button>
      </div>
      <p className="hint">
        Point at a folder of .dat/.xml/.zip files (e.g. a batch download from Dat-o-Matic) — each
        one is matched to a system by name automatically.
      </p>
      {folderResult && (
        <div className="dat-folder-result">
          {folderResult.imported.length > 0 && (
            <p className="hint">
              Imported:{" "}
              {folderResult.imported
                .map((e) => `${e.system_name} (${e.games_imported} games, ${e.newly_matched} newly matched)`)
                .join(", ")}
            </p>
          )}
          {folderResult.unmatched.length > 0 && (
            <p className="hint">
              Couldn't match to a system, import these manually: {folderResult.unmatched.join(", ")}
            </p>
          )}
          {folderResult.errors.length > 0 && <p className="hint">Errors: {folderResult.errors.join("; ")}</p>}
        </div>
      )}
      <table className="settings-table">
        <thead>
          <tr>
            <th>System</th>
            <th>DATs imported</th>
            <th>Custom DAT URL</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {systems.map((s) => {
            const sources = datSources.filter((d) => d.system_id === s.id);
            const hint = MULTI_VARIANT_SYSTEM_HINTS[normalizeFolderKey(s.folder_name)];
            return (
              <tr key={s.id}>
                <td>{s.name}</td>
                <td>
                  {sources.length === 0 ? (
                    "Not imported"
                  ) : (
                    <ul className="dat-source-list">
                      {sources.map((d) => (
                        <li key={d.id}>
                          {d.dat_name ?? d.file_name}
                          {d.dat_version ? ` (${d.dat_version})` : ""}
                          <button
                            className="dat-source-remove"
                            disabled={removingId === d.id}
                            onClick={() => removeSource(d.id)}
                            title="Remove this DAT"
                          >
                            ×
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                  {hint && <p className="hint dat-variant-hint">{hint}</p>}
                </td>
                <td>
                  <input
                    type="text"
                    defaultValue={s.dat_url ?? ""}
                    placeholder="https://…"
                    onBlur={(e) => saveDatUrl(s.id, e.target.value)}
                  />
                </td>
                <td className="settings-row">
                  <button disabled={busy === s.id} onClick={() => importFile(s.id)}>
                    {busy === s.id && <span className="spinner inline" aria-hidden="true" />}
                    Import DAT…
                  </button>
                  {(knownSource[s.id] || !!s.dat_url) && (
                    <button disabled={busy === s.id} onClick={() => fetchDat(s.id)}>
                      {busy === s.id && <span className="spinner inline" aria-hidden="true" />}
                      Fetch DAT
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {message && <p className="hint">{message}</p>}
    </div>
  );
}
