import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import type { ScanProgress, ScanSummary } from "../../types/rom";

export function TopBar({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { settings, refreshRoms, refreshSystems } = useLibrary();
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [lastSummary, setLastSummary] = useState<ScanSummary | null>(null);
  const unlistenRefs = useRef<Array<() => void>>([]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const un1 = await listen<ScanProgress>("scan://progress", (e) => setProgress(e.payload));
      const un2 = await listen<ScanSummary>("scan://done", (e) => {
        setLastSummary(e.payload);
      });
      if (cancelled) {
        un1();
        un2();
      } else {
        unlistenRefs.current = [un1, un2];
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
    setScanning(true);
    setProgress(null);
    setLastSummary(null);
    try {
      await api.scanLibrary();
      await refreshRoms();
      await refreshSystems();
    } catch (e) {
      setLastSummary({ scanned_files: 0, matched: 0, unmatched: 0, removed: 0, errors: [String(e)] });
    } finally {
      setScanning(false);
      setProgress(null);
    }
  }

  return (
    <header className="top-bar">
      <h1 className="app-title">ROM Manager</h1>
      <div className="top-bar-status">
        {scanning && progress && (
          <span className="scan-progress">
            Scanning {progress.current}/{progress.total}: {progress.current_file}
          </span>
        )}
        {!scanning && lastSummary && (
          <span className="scan-summary">
            Scanned {lastSummary.scanned_files} · {lastSummary.matched} matched · {lastSummary.unmatched} unmatched
            {lastSummary.removed > 0 ? ` · ${lastSummary.removed} removed` : ""}
            {lastSummary.errors.length > 0 ? ` · ${lastSummary.errors.length} error(s)` : ""}
          </span>
        )}
      </div>
      <div className="top-bar-actions">
        <button onClick={handleScan} disabled={scanning}>
          {scanning ? "Scanning…" : "Scan Library"}
        </button>
        <button onClick={onOpenSettings}>Settings</button>
      </div>
    </header>
  );
}
