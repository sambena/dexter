import { useState } from "react";
import { api } from "../../api/tauri";
import { useJob } from "../../state/useJobStore";
import type { RetroArchExportSummary } from "../../types/rom";

export function RetroArchPanel() {
  const { startJob, endJob } = useJob();
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<RetroArchExportSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function exportPlaylists() {
    setBusy(true);
    setError(null);
    setSummary(null);
    startJob("Exporting to RetroArch");
    try {
      setSummary(await api.exportRetroArchPlaylists());
    } catch (e) {
      setError(String(e));
    } finally {
      endJob();
      setBusy(false);
    }
  }

  return (
    <div className="settings-section">
      <div className="tools-toolbar">
        <div className="settings-row">
          <button className="primary" onClick={exportPlaylists} disabled={busy}>
            {busy && <span className="spinner inline" aria-hidden="true" />}
            Export to RetroArch
          </button>
        </div>
        {error && <p className="hint tools-message">{error}</p>}
      </div>

      <p className="hint">
        Writes a RetroArch playlist for each system that launches with RetroArch, naming games as Dexter does, and
        copies their box art into RetroArch's thumbnails folder so it shows there too. Systems set to another emulator
        (like Cemu) are left out. Export again after downloading art or adding games; only new images are copied.
        If a playlist of the same name already exists, the first one replaced is kept as a .lpl.bak file.
      </p>

      {summary && (
        <div className="hint">
          <p>
            Wrote {summary.playlists.length} playlist{summary.playlists.length === 1 ? "" : "s"} with {summary.games}{" "}
            games to {summary.playlists_dir}. Copied {summary.thumbnails_copied} thumbnail
            {summary.thumbnails_copied === 1 ? "" : "s"}
            {summary.thumbnails_unchanged > 0 && ` (${summary.thumbnails_unchanged} already there)`}
            {summary.without_art > 0 && `; ${summary.without_art} games have no box art`}.
          </p>
          <ul>
            {summary.playlists.map((p) => (
              <li key={p}>{p}</li>
            ))}
            {summary.skipped_systems.map((s) => (
              <li key={s}>Skipped {s}</li>
            ))}
          </ul>
          {summary.errors.length > 0 && <p>Problems: {summary.errors.join("; ")}</p>}
          <p>Restart RetroArch if it's open, so it reloads its playlists.</p>
        </div>
      )}
    </div>
  );
}
