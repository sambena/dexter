import { api } from "../../api/tauri";
import { useJob } from "../../state/useJobStore";
import { useState } from "react";

/// Sits above everything (including the Settings modal) so any long-running
/// work is visible from wherever it was started.
export function GlobalProgress() {
  const { job } = useJob();
  const [stopping, setStopping] = useState(false);

  if (!job) return null;

  const determinate = job.total !== null && job.total > 0 && job.current !== null;
  const pct = determinate ? Math.min(100, Math.round((job.current! / job.total!) * 100)) : 0;

  async function stop() {
    setStopping(true);
    try {
      await api.cancelScan();
    } catch {
      // best-effort — the running job will finish on its own
    }
  }

  return (
    <div className="global-progress" role="status" aria-live="polite">
      <div className={`progress-track ${determinate ? "" : "indeterminate"}`}>
        <div className="progress-fill" style={determinate ? { width: `${pct}%` } : undefined} />
      </div>
      <div className="global-progress-text">
        <span className="spinner" aria-hidden="true" />
        <strong>{job.label}</strong>
        {determinate && (
          <span className="global-progress-count">
            {job.current}/{job.total} ({pct}%)
          </span>
        )}
        {job.detail && (
          <span className="global-progress-detail" title={job.detail}>
            {job.detail}
          </span>
        )}
        {job.cancellable && (
          <button className="global-progress-stop" onClick={stop} disabled={stopping}>
            {stopping ? "Stopping…" : "Stop"}
          </button>
        )}
      </div>
    </div>
  );
}
