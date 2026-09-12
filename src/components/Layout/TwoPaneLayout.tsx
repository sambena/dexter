import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

const MIN_LEFT = 220;
const MAX_LEFT = 720;
const DEFAULT_LEFT = 340;
const STORAGE_KEY = "rom-manager.leftPaneWidth";

function loadStoredWidth(): number {
  try {
    const stored = Number(localStorage.getItem(STORAGE_KEY));
    return stored >= MIN_LEFT && stored <= MAX_LEFT ? stored : DEFAULT_LEFT;
  } catch {
    return DEFAULT_LEFT;
  }
}

export function TwoPaneLayout({ left, right }: { left: ReactNode; right: ReactNode }) {
  const [leftWidth, setLeftWidth] = useState<number>(loadStoredWidth);
  const draggingRef = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const onMouseMove = useCallback((e: MouseEvent) => {
    if (!draggingRef.current || !containerRef.current) return;
    const originX = containerRef.current.getBoundingClientRect().left;
    const next = Math.min(MAX_LEFT, Math.max(MIN_LEFT, e.clientX - originX));
    setLeftWidth(next);
  }, []);

  const stopDragging = useCallback(() => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    setLeftWidth((w) => {
      try {
        localStorage.setItem(STORAGE_KEY, String(w));
      } catch {
        // per-viewer convenience only — fine to skip if storage is unavailable
      }
      return w;
    });
  }, []);

  useEffect(() => {
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", stopDragging);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", stopDragging);
    };
  }, [onMouseMove, stopDragging]);

  function startDragging() {
    draggingRef.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }

  return (
    <div ref={containerRef} className="two-pane" style={{ gridTemplateColumns: `${leftWidth}px 5px 1fr` }}>
      <div className="pane pane-left">{left}</div>
      <div className="pane-resizer" onMouseDown={startDragging} />
      <div className="pane pane-right">{right}</div>
    </div>
  );
}
