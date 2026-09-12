import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { DuplicateFileDto, DuplicateGroupDto, MaintenanceSummary } from "../../types/rom";

function formatSize(bytes: number | null): string {
  if (bytes == null) return "";
  const mb = bytes / (1024 * 1024);
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** The file's location inside the ROM folder, with archives shown as "archive → member". */
function displayPath(file: DuplicateFileDto, romRoot: string | null): string {
  let path = file.archive_member
    ? file.file_path.slice(0, file.file_path.length - file.archive_member.length - 2)
    : file.file_path;
  if (romRoot && path.toLowerCase().startsWith(romRoot.toLowerCase())) {
    path = path.slice(romRoot.length).replace(/^[\\/]+/, "");
  }
  return file.archive_member ? `${path} → ${file.archive_member}` : path;
}

/** Name without folder or extension, with the characters renaming swaps for "_" normalised. */
function comparableStem(name: string): string {
  return name
    .replace(/\.[^.\\/]*$/, "")
    .replace(/[<>:"/\\|?*]/g, "_")
    .toLowerCase();
}

/**
 * The copy to keep: the one already named after its DAT entry, since the
 * others are what block renaming. Otherwise the first.
 */
function keeperOf(group: DuplicateGroupDto): number {
  const named = group.files.find((f) => {
    const onDisk = f.archive_member
      ? f.file_path.slice(0, f.file_path.length - f.archive_member.length - 2)
      : f.file_path;
    const base = onDisk.split(/[\\/]/).pop() ?? "";
    return comparableStem(base) === comparableStem(f.display_name + ".x");
  });
  return (named ?? group.files[0]).id;
}

export function describeDeletion(summary: MaintenanceSummary): string {
  const parts = [`Deleted ${summary.succeeded} file${summary.succeeded === 1 ? "" : "s"}.`];
  if (summary.moved_to.length)
    parts.push(`Network folders have no Recycle Bin, so they were moved to ${summary.moved_to.join(", ")}.`);
  if (summary.skipped) parts.push(`Skipped ${summary.skipped}.`);
  if (summary.errors.length) parts.push(`Problems: ${summary.errors.join("; ")}`);
  return parts.join(" ");
}

export function DuplicatesPanel() {
  const { refreshRoms, settings } = useLibrary();
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
      // Preselect every copy but one in each group: keeping one is almost
      // always the intent, and the choice stays editable.
      const preset = new Set<number>();
      result.forEach((g) => {
        const keep = keeperOf(g);
        g.files.forEach((f) => f.id !== keep && preset.add(f.id));
      });
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
      `Delete ${ids.length} file${ids.length === 1 ? "" : "s"}?\n\n` +
        `Files on this PC go to the Recycle Bin. Files on a network share are moved to a ` +
        `"_Deleted by Dexter" folder in your ROM folder, so they can still be restored.`,
    );
    if (!ok) return;

    setBusy(true);
    setMessage(null);
    startJob("Deleting duplicates");
    try {
      const summary = await api.deleteRoms(ids);
      await load();
      setMessage(describeDeletion(summary));
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
      <div className="tools-toolbar">
        <div className="settings-row">
          <button className="primary" onClick={deleteSelected} disabled={busy || selected.size === 0}>
            {busy && <span className="spinner inline" aria-hidden="true" />}
            Delete {selected.size} selected…
          </button>
          <button onClick={load} disabled={busy}>
            Rescan
          </button>
          {groups !== null && groups.length > 0 && (
            <span className="hint">
              {groups.length} group{groups.length === 1 ? "" : "s"} · {totalDupes} redundant file
              {totalDupes === 1 ? "" : "s"} · {selected.size} selected
            </span>
          )}
        </div>
        {message && <p className="hint tools-message">{message}</p>}
      </div>

      <p className="hint">
        Files that are byte-for-byte identical, grouped by hash, plus extracted title folders (Wii U) with the
        same title and version. Every copy in a group is interchangeable, so keeping any one of them loses nothing. Deleted files can be restored: from the Recycle Bin for files on
        this PC, or from the "_Deleted by Dexter" folder for files on a network share.
      </p>

      {groups === null && <p className="hint">Scanning…</p>}
      {groups !== null && groups.length === 0 && (
        <p className="hint">No duplicates found. Note that only hashed ROMs can be compared — run Hash &amp; Match first if your library still has unhashed files.</p>
      )}

      {groups !== null && groups.length > 0 && (
        <div className="dupe-groups">
          {groups.map((g) => (
            <div key={g.sha1} className="dupe-group">
              <div className="dupe-group-title">
                {g.files[0].display_name}
                {g.same_title ? (
                  <span className="dupe-hash" title="Extracted title folders can't be compared byte for byte, but these have the same title ID and version.">
                    same title and version
                  </span>
                ) : (
                  <span className="mono dupe-hash" title={g.sha1}>
                    {g.sha1.slice(0, 12)}
                  </span>
                )}
              </div>
              {g.files.map((f) => (
                <label key={f.id} className="dupe-file">
                  <input type="checkbox" checked={selected.has(f.id)} onChange={() => toggle(f.id)} disabled={busy} />
                  <span className="dupe-path" title={f.file_path}>
                    <bdi>{displayPath(f, settings.rom_root_path)}</bdi>
                  </span>
                  <span className="dupe-meta">
                    {f.system_name ?? ""} {formatSize(f.size)}
                  </span>
                </label>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
