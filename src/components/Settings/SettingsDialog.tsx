import { FolderPicker } from "./FolderPicker";
import { DatImportPanel } from "./DatImportPanel";
import { EmulatorSettings } from "./EmulatorSettings";

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Settings</h2>
          <button onClick={onClose}>Close</button>
        </div>
        <div className="modal-body">
          <FolderPicker />
          <DatImportPanel />
          <EmulatorSettings />
        </div>
      </div>
    </div>
  );
}
