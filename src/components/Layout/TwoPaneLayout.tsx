import type { ReactNode } from "react";

export function TwoPaneLayout({ left, right }: { left: ReactNode; right: ReactNode }) {
  return (
    <div className="two-pane">
      <div className="pane pane-left">{left}</div>
      <div className="pane pane-right">{right}</div>
    </div>
  );
}
