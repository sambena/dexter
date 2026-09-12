import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { DuplicateGroupDto } from "../../types/rom";

function formatSize(bytes: number | null): string {
  if (bytes == null) return "";
  const mb = bytes / (1024 * 1024);
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function DuplicatesPanel() {
  const { refreshRoms } = useLibrary();
  const { startJob, endJob } = useJob();
  const [groups, setGroups] = useState<DuplicateGroupDto[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    setBusy(true);
    startJob("Finding duplicates");
    try {
      const result = await api.listDuplicates();
      setGroups(result);
      // Preselect every copy but the first in each group: keeping one is
      // almost always the intent, and the choice stays editable.
      const preset = new Set<number>();
      result.forEach((g) => g.files.slice(1).forEach((f) => preset.add(f.id)));
      setSelected(preset);
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setBusy(false);
    }
  }

  useEffect(() => {
    load();
  }, []);

  function toggle(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function deleteSelected() {
    const ids = [...selected];
    if (ids.length === 0) return;
    const ok = window.confirm(
      `Send ${ids.length} file${ids.length === 1 ? "" : "s"} to the Recycle Bin?\n\n` +
        `They can be restored from Windows if this was a mistake.`,
    );
    if (!ok) return;

    setBusy(true);
    startJob("Deleting duplicates");
    try {
      const summary = await api.deleteRoms(ids);
      setMessage(
        `Deleted ${summary.succeeded}` +
          (summary.skipped ? `, skipped ${summary.skipped}` : "") +
          (summary.errors.length ? ` — ${summary.errors.join("; ")}` : ""),
      );
      await load();
      await refreshRoms();
    } catch (e) {
      setMessage(String(e));
    } finally {
      endJob();
      setBusy(false);
    }
  }

  const totalDupes = groups?.reduce((n, g) => n + g.files.length - 1, 0) ?? 0;

  return (
    <div className="settings-section">
      <h3>Duplicates</h3>
      <p className="hint">
        Files that are byte-for-byte identical, grouped by hash. Every copy in a group is
        interchangeable, so keeping any one of them loses nothing. Deleting sends files to the
        Recycle Bin.
      </p>

      {groups === null && <p className="hint">Scanning…</p>}
      {groups !== null && groups.length === 0 && (
        <p className="hint">No duplicates found. Note that only hashed ROMs can be compared — run Hash &amp; Match first if your library still has unhashed files.</p>
      )}

      {groups !== null && groups.length > 0 && (
        <>
          <p className="hint">
            {groups.length} group{groups.length === 1 ? "" : "s"} · {totalDupes} redundant file
            {totalDupes === 1 ? "" : "s"} · {selected.size} selected
          </p>
          <div className="dupe-groups">
            {groups.map((g) => (
              <div key={g.sha1} className="dupe-group">
                <div className="dupe-group-title">
                  {g.files[0].display_name}
                  <span className="mono dupe-hash" title={g.sha1}>
                    {g.sha1.slice(0, 12)}
                  </span>
                </div>
                {g.files.map((f) => (
                  <label key={f.id} className="dupe-file">
                    <input
                      type="checkbox"
                      checked={selected.has(f.id)}
                      onChange={() => toggle(f.id)}
                      disabled={busy}
                    />
                    <span className="dupe-path" title={f.file_path}>
                      {f.file_path}
                      {f.archive_member ? ` :: ${f.archive_member}` : ""}
                    </span>
                    <span className="dupe-meta">
                      {f.system_name ?? ""} {formatSize(f.size)}
                    </span>
                  </label>
                ))}
              </div>
            ))}
          </div>
          <div className="settings-row tools-actions">
            <button onClick={deleteSelected} disabled={busy || selected.size === 0}>
              {busy && <span className="spinner inline" aria-hidden="true" />}
              Delete {selected.size} selected…
            </button>
            <button onClick={load} disabled={busy}>
              Rescan
            </button>
          </div>
        </>
      )}
      {message && <p className="hint">{message}</p>}
    </div>
  );
}
