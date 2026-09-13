import { useState } from "react";
import { DuplicatesPanel } from "./DuplicatesPanel";
import { RenamePanel } from "./RenamePanel";
import { RetroArchPanel } from "./RetroArchPanel";

type Tab = "duplicates" | "rename" | "retroarch";

export function ToolsDialog({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("duplicates");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Library Tools</h2>
          <button onClick={onClose}>Close</button>
        </div>
        <div className="modal-tabs">
          <button className={tab === "duplicates" ? "active" : ""} onClick={() => setTab("duplicates")}>
            Duplicates
          </button>
          <button className={tab === "rename" ? "active" : ""} onClick={() => setTab("rename")}>
            Rename
          </button>
          <button className={tab === "retroarch" ? "active" : ""} onClick={() => setTab("retroarch")}>
            RetroArch
          </button>
        </div>
        <div className="modal-body">
          {tab === "duplicates" && <DuplicatesPanel />}
          {tab === "rename" && <RenamePanel />}
          {tab === "retroarch" && <RetroArchPanel />}
        </div>
      </div>
    </div>
  );
}
