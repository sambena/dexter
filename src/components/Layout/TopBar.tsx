import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import type { ScanProgress, ScanSummary } from "../../types/rom";

type Job = "scan" | "hash" | null;

export function TopBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { settings, refreshRoms, refreshSystems } = useLibrary();
  const [activeJob, setActiveJob] = useState<Job>(null);
  const [stopping, setStopping] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [lastSummary, setLastSummary] = useState<{ job: Job; summary: ScanSummary } | null>(null);
  const unlistenRefs = useRef<Array<() => void>>([]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const listeners = await Promise.all([
        listen<ScanProgress>("scan://progress", (e) => setProgress(e.payload)),
        listen<ScanSummary>("scan://done", (e) => setLastSummary({ job: "scan", summary: e.payload })),
        listen<ScanProgress>("hash://progress", (e) => setProgress(e.payload)),
        listen<ScanSummary>("hash://done", (e) => setLastSummary({ job: "hash", summary: e.payload })),
      ]);
      if (cancelled) {
        listeners.forEach((un) => un());
      } else {
        unlistenRefs.current = listeners;
      }
    })();
    return () => {
      cancelled = true;
      unlistenRefs.current.forEach((u) => u());
    };
  }, []);

  async function handleScan() {
    if (!settings.rom_root_path) {
      onOpenSettings();
      return;
    }
    setActiveJob("scan");
    setStopping(false);
    setProgress(null);
    setLastSummary(null);
    try {
      await api.scanLibrary();
      await refreshRoms();
      await refreshSystems();
    } catch (e) {
      setLastSummary({ job: "scan", summary: { scanned_files: 0, pending: 0, matched: 0, unmatched: 0, removed: 0, errors: [String(e)] } });
    } finally {
      setActiveJob(null);
      setStopping(false);
      setProgress(null);
    }
  }

  async function handleHash() {
    setActiveJob("hash");
    setStopping(false);
    setProgress(null);
    setLastSummary(null);
    try {
      await api.hashPendingRoms();
      await refreshRoms();
    } catch (e) {
      setLastSummary({ job: "hash", summary: { scanned_files: 0, pending: 0, matched: 0, unmatched: 0, removed: 0, errors: [String(e)] } });
    } finally {
      setActiveJob(null);
      setStopping(false);
      setProgress(null);
    }
  }

  async function handleStop() {
    setStopping(true);
    try {
      await api.cancelScan();
    } catch {
      // best-effort — the running job will just finish naturally
    }
  }

  return (
    <header className="top-bar">
      <h1 className="app-title">ROM Manager</h1>
      <div className="top-bar-status">
        {activeJob && progress && (
          <span className="scan-progress">
            {activeJob === "scan" ? "Scanning" : "Hashing"} {progress.current}/{progress.total}: {progress.current_file}
          </span>
        )}
        {!activeJob && lastSummary && (
          <span className="scan-summary">
            {lastSummary.job === "scan" ? (
              <>
                Found {lastSummary.summary.scanned_files} files · {lastSummary.summary.pending} ready to hash
                {lastSummary.summary.removed > 0 ? ` · ${lastSummary.summary.removed} removed` : ""}
              </>
            ) : (
              <>
                Hashed {lastSummary.summary.scanned_files} · {lastSummary.summary.matched} matched ·{" "}
                {lastSummary.summary.unmatched} unmatched
              </>
            )}
            {lastSummary.summary.errors.length > 0 ? ` · ${lastSummary.summary.errors.length} error(s)` : ""}
          </span>
        )}
      </div>
      <div className="top-bar-actions">
        <button onClick={handleScan} disabled={activeJob !== null}>
          {activeJob === "scan" ? "Scanning…" : "Scan Files"}
        </button>
        <button onClick={handleHash} disabled={activeJob !== null}>
          {activeJob === "hash" ? "Hashing…" : "Hash & Match"}
        </button>
        {activeJob !== null && (
          <button onClick={handleStop} disabled={stopping}>
            {stopping ? "Stopping…" : "Stop"}
          </button>
        )}
        <button onClick={onOpenSettings}>Settings</button>
      </div>
    </header>
  );
}
