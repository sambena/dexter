import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { ArtFetchSummary, ScanProgress, ScanSummary } from "../../types/rom";
import { basename } from "../../utils/path";

type Job = "scan" | "hash" | "art" | null;
type Summary =
  | { job: "scan" | "hash"; summary: ScanSummary }
  | { job: "art"; summary: ArtFetchSummary };

const emptyScanSummary: ScanSummary = {
  scanned_files: 0,
  pending: 0,
  matched: 0,
  unmatched: 0,
  unverifiable: 0,
  removed: 0,
  errors: [],
};
const emptyArtSummary: ArtFetchSummary = { attempted: 0, downloaded: 0, not_found: 0, errors: [] };

export function TopBar({
  onOpenSettings,
  onOpenTools,
}: {
  onOpenSettings: () => void;
  onOpenTools: () => void;
}) {
  const { settings, refreshRoms, refreshSystems } = useLibrary();
  const { startJob, updateJob, endJob } = useJob();
  const [activeJob, setActiveJob] = useState<Job>(null);
  const [lastSummary, setLastSummary] = useState<Summary | null>(null);
  const unlistenRefs = useRef<Array<() => void>>([]);

  useEffect(() => {
    let cancelled = false;
    const onProgress = (p: ScanProgress) =>
      updateJob({ current: p.current, total: p.total, detail: basename(p.current_file) });
    (async () => {
      const listeners = await Promise.all([
        listen<ScanProgress>("scan://progress", (e) => onProgress(e.payload)),
        listen<ScanSummary>("scan://done", (e) => setLastSummary({ job: "scan", summary: e.payload })),
        listen<ScanProgress>("hash://progress", (e) => onProgress(e.payload)),
        listen<ScanSummary>("hash://done", (e) => setLastSummary({ job: "hash", summary: e.payload })),
        listen<ScanProgress>("art://progress", (e) => onProgress(e.payload)),
        listen<ArtFetchSummary>("art://done", (e) => setLastSummary({ job: "art", summary: e.payload })),
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
  }, [updateJob]);

  async function handleScan() {
    if (!settings.rom_root_path) {
      onOpenSettings();
      return;
    }
    setActiveJob("scan");
    setLastSummary(null);
    startJob("Scanning library", { cancellable: true });
    try {
      await api.scanLibrary();
      await refreshRoms();
      await refreshSystems();
    } catch (e) {
      setLastSummary({ job: "scan", summary: { ...emptyScanSummary, errors: [String(e)] } });
    } finally {
      endJob();
      setActiveJob(null);
    }
  }

  async function handleHash() {
    setActiveJob("hash");
    setLastSummary(null);
    startJob("Hashing & matching", { cancellable: true });
    try {
      await api.hashPendingRoms();
      await refreshRoms();
    } catch (e) {
      setLastSummary({ job: "hash", summary: { ...emptyScanSummary, errors: [String(e)] } });
    } finally {
      endJob();
      setActiveJob(null);
    }
  }

  async function handleDownloadArt() {
    setActiveJob("art");
    setLastSummary(null);
    startJob("Downloading box art", { cancellable: true });
    try {
      await api.fetchAllBoxArt();
      await refreshRoms();
    } catch (e) {
      setLastSummary({ job: "art", summary: { ...emptyArtSummary, errors: [String(e)] } });
    } finally {
      endJob();
      setActiveJob(null);
    }
  }

  return (
    <header className="top-bar">
      <h1 className="app-title">Dexter</h1>
      <div className="top-bar-status">
        {!activeJob && lastSummary && (
          <span className="scan-summary">
            {lastSummary.job === "scan" && (
              <>
                Found {lastSummary.summary.scanned_files} files · {lastSummary.summary.pending} ready to hash
                {lastSummary.summary.unverifiable > 0 ? ` · ${lastSummary.summary.unverifiable} can't verify` : ""}
                {lastSummary.summary.removed > 0 ? ` · ${lastSummary.summary.removed} removed` : ""}
              </>
            )}
            {lastSummary.job === "hash" && (
              <>
                Hashed {lastSummary.summary.scanned_files} · {lastSummary.summary.matched} matched ·{" "}
                {lastSummary.summary.unmatched} unmatched
              </>
            )}
            {lastSummary.job === "art" && (
              <>
                Checked {lastSummary.summary.attempted} games · {lastSummary.summary.downloaded} downloaded ·{" "}
                {lastSummary.summary.not_found} not found
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
        <button onClick={handleDownloadArt} disabled={activeJob !== null}>
          {activeJob === "art" ? "Downloading…" : "Download Box Art"}
        </button>
        <button onClick={onOpenTools} disabled={activeJob !== null}>
          Tools
        </button>
        <button onClick={onOpenSettings}>Settings</button>
      </div>
    </header>
  );
}
