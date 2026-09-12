import { useState } from "react";
import { LibraryProvider } from "./state/useLibraryStore";
import { TopBar } from "./components/Layout/TopBar";
import { TwoPaneLayout } from "./components/Layout/TwoPaneLayout";
import { RomList } from "./components/RomList/RomList";
import { DetailsPanel } from "./components/DetailsPanel/DetailsPanel";
import { SettingsDialog } from "./components/Settings/SettingsDialog";
import "./styles/global.css";

function App() {
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <LibraryProvider>
      <div className="app">
        <TopBar onOpenSettings={() => setSettingsOpen(true)} />
        <TwoPaneLayout left={<RomList />} right={<DetailsPanel />} />
        {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
      </div>
    </LibraryProvider>
  );
}

export default App;
