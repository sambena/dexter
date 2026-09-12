import { useState } from "react";
import { LibraryProvider } from "./state/useLibraryStore";
import { JobProvider } from "./state/useJobStore";
import { ApiSync } from "./components/Layout/ApiSync";
import { GlobalProgress } from "./components/Layout/GlobalProgress";
import { TopBar } from "./components/Layout/TopBar";
import { TwoPaneLayout } from "./components/Layout/TwoPaneLayout";
import { RomList } from "./components/RomList/RomList";
import { DetailsPanel } from "./components/DetailsPanel/DetailsPanel";
import { SettingsDialog } from "./components/Settings/SettingsDialog";
import { ToolsDialog } from "./components/Tools/ToolsDialog";
import "./styles/global.css";

function App() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [toolsOpen, setToolsOpen] = useState(false);

  return (
    <LibraryProvider>
      <JobProvider>
        <div className="app">
          <ApiSync />
          <GlobalProgress />
          <TopBar onOpenSettings={() => setSettingsOpen(true)} onOpenTools={() => setToolsOpen(true)} />
          <TwoPaneLayout left={<RomList />} right={<DetailsPanel />} />
          {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
          {toolsOpen && <ToolsDialog onClose={() => setToolsOpen(false)} />}
        </div>
      </JobProvider>
    </LibraryProvider>
  );
}

export default App;
