import { useState } from "react";
import { DuplicatesPanel } from "./DuplicatesPanel";
import { RenamePanel } from "./RenamePanel";

type Tab = "duplicates" | "rename";

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
        </div>
        <div className="modal-body">
          {tab === "duplicates" ? <DuplicatesPanel /> : <RenamePanel />}
        </div>
      </div>
    </div>
  );
}
