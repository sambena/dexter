import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { RenamePlanEntryDto } from "../../types/rom";

export function RenamePanel() {
  const { refreshRoms } = useLibrary();
  const { startJob, endJob } = useJob();
  const [plan, setPlan] = useState<RenamePlanEntryDto[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    setBusy(true);
    startJob("Building rename plan");
    try {
      const result = await api.previewRenames();
      setPlan(result);
      setSelected(new Set(result.filter((e) => !e.blocked_reason).map((e) => e.rom_id)));
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

  function toggle(romId: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(romId)) next.delete(romId);
      else next.add(romId);
      return next;
    });
  }

  async function apply() {
    const ids = [...selected];
    if (ids.length === 0) return;
    setBusy(true);
    startJob("Renaming files");
    try {
      const summary = await api.applyRenames(ids);
      setMessage(
        `Renamed ${summary.succeeded}` +
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

  const renameable = plan?.filter((e) => !e.blocked_reason) ?? [];
  const blocked = plan?.filter((e) => e.blocked_reason) ?? [];

  return (
    <div className="settings-section">
      <h3>Rename to DAT names</h3>
      <p className="hint">
        Renames matched ROMs to the canonical name from the DAT. Only matched files appear — an
        unmatched file has no authoritative name to rename to. Save files named after a ROM are renamed
        with it, and files a cue sheet or playlist loads by name are left alone. Nothing is touched until you
        apply.
      </p>

      {plan === null && <p className="hint">Building plan…</p>}
      {plan !== null && plan.length === 0 && (
        <p className="hint">Every matched ROM already uses its DAT name. Nothing to rename.</p>
      )}

      {plan !== null && plan.length > 0 && (
        <>
          <p className="hint">
            {renameable.length} can be renamed · {selected.size} selected
            {blocked.length > 0 ? ` · ${blocked.length} blocked` : ""}
          </p>
          <table className="settings-table rename-table">
            <thead>
              <tr>
                <th></th>
                <th>Current name</th>
                <th>New name</th>
              </tr>
            </thead>
            <tbody>
              {plan.map((e) => (
                <tr key={e.rom_id} className={e.blocked_reason ? "rename-blocked" : ""}>
                  <td>
                    <input
                      type="checkbox"
                      checked={selected.has(e.rom_id)}
                      onChange={() => toggle(e.rom_id)}
                      disabled={busy || !!e.blocked_reason}
                    />
                  </td>
                  <td title={e.target_path}>{e.current_name}</td>
                  <td>
                    {e.blocked_reason ? (
                      <span className="hint rename-reason">{e.blocked_reason}</span>
                    ) : (
                      <>
                        {e.new_name}
                        {e.also_renames.length > 0 && (
                          <span className="hint rename-reason"> · also renames {e.also_renames.join(", ")}</span>
                        )}
                      </>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="settings-row tools-actions">
            <button onClick={apply} disabled={busy || selected.size === 0}>
              {busy && <span className="spinner inline" aria-hidden="true" />}
              Rename {selected.size} file{selected.size === 1 ? "" : "s"}
            </button>
            <button onClick={load} disabled={busy}>
              Refresh plan
            </button>
          </div>
        </>
      )}
      {message && <p className="hint">{message}</p>}
    </div>
  );
}
