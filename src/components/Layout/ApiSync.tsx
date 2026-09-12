import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";

/// Keeps the window in step with commands run through the local API or
/// dexter-cli: their jobs show in the progress strip, and lists reload
/// once they change the library.
export function ApiSync() {
  const { refreshRoms, refreshSystems, refreshSettings } = useLibrary();
  const { startJob, endJob } = useJob();

  // Listeners are registered once; the refs always point at the latest
  // callbacks (refreshRoms changes whenever the filters do).
  const latest = useRef({ refreshRoms, refreshSystems, refreshSettings, startJob, endJob });
  latest.current = { refreshRoms, refreshSystems, refreshSettings, startJob, endJob };

  useEffect(() => {
    let cancelled = false;
    let unlisten: Array<() => void> = [];
    (async () => {
      const listeners = await Promise.all([
        listen<{ label: string; cancellable: boolean }>("api://job-started", (e) =>
          latest.current.startJob(e.payload.label, { cancellable: e.payload.cancellable }),
        ),
        listen("api://job-finished", () => latest.current.endJob()),
        listen("api://library-changed", () => {
          const { refreshRoms, refreshSystems, refreshSettings } = latest.current;
          refreshSettings();
          refreshSystems();
          refreshRoms();
        }),
      ]);
      if (cancelled) {
        listeners.forEach((un) => un());
      } else {
        unlisten = listeners;
      }
    })();
    return () => {
      cancelled = true;
      unlisten.forEach((un) => un());
    };
  }, []);

  return null;
}
