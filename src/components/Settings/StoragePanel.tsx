import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useJob } from "../../state/useJobStore";
import type { StorageLocationsDto } from "../../types/settings";

const isNetworkPath = (path: string) => path.startsWith("\\\\");

export function StoragePanel() {
  const { startJob, endJob } = useJob();
  const [locations, setLocations] = useState<StorageLocationsDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    api.getStorageLocations().then(setLocations, (e) => setMessage(String(e)));
  }, []);

  async function run(label: string, work: () => Promise<string>) {
    setBusy(true);
    setMessage(null);
    startJob(label);
    try {
      setMessage(await work());
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setBusy(false);
      setLocations(await api.getStorageLocations().catch(() => locations));
    }
  }

  async function moveLibrary(toDefault: boolean) {
    const folder = toDefault ? null : await api.pickRomRootFolder();
    if (!toDefault && !folder) return;
    const warning =
      folder && isNetworkPath(folder)
        ? "\n\nThat's a network folder. It works, but the library is slower there and can be damaged if the connection drops mid-write."
        : "";
    const destination = folder ?? locations?.app_folder ?? "the app data folder";
    if (!window.confirm(`Move the library database to ${destination}?${warning}`)) return;
    await run("Moving library", async () => {
      const moved = await api.moveLibraryDatabase(folder);
      return `The library database is now in ${moved.library_folder}.`;
    });
  }

  async function moveArt(toDefault: boolean) {
    const folder = toDefault ? null : await api.pickRomRootFolder();
    if (!toDefault && !folder) return;
    await run("Moving box art", async () => {
      const summary = await api.moveBoxArt(folder);
      const parts = [`Moved ${summary.succeeded} image${summary.succeeded === 1 ? "" : "s"} to ${summary.moved_to[0]}.`];
      if (summary.skipped) parts.push(`${summary.skipped} were already missing.`);
      if (summary.errors.length)
        parts.push(`${summary.errors.length} couldn't be moved and still work where they are: ${summary.errors.join("; ")}`);
      return parts.join(" ");
    });
  }

  return (
    <div className="settings-section">
      <h3>Storage</h3>
      <label className="hint">Library database</label>
      <div className="settings-row">
        <input type="text" readOnly value={locations?.library_folder ?? ""} title={locations?.library_folder} />
        <button onClick={() => moveLibrary(false)} disabled={busy || !locations}>
          Move…
        </button>
        {locations && !locations.library_is_default && (
          <button onClick={() => moveLibrary(true)} disabled={busy}>
            Use default
          </button>
        )}
      </div>
      <label className="hint">Box art</label>
      <div className="settings-row">
        <input type="text" readOnly value={locations?.art_folder ?? ""} title={locations?.art_folder} />
        <button onClick={() => moveArt(false)} disabled={busy || !locations}>
          Move…
        </button>
        {locations && !locations.art_is_default && (
          <button onClick={() => moveArt(true)} disabled={busy}>
            Use default
          </button>
        )}
      </div>
      {message && <p className="hint">{message}</p>}
      <p className="hint">
        Moving carries the existing files over, so nothing is downloaded again. The database is best kept on this PC.
        {locations && ` api.json and the window position always stay in ${locations.app_folder}.`}
      </p>
    </div>
  );
}
