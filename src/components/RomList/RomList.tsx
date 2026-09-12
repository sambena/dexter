import { useLibrary } from "../../state/useLibraryStore";
import { FilterBar } from "./FilterBar";
import { RomListItem } from "./RomListItem";
import { StatusLegend } from "./StatusLegend";

export function RomList() {
  const { roms, loading, selectedRomId, setSelectedRomId, settings } = useLibrary();

  return (
    <div className="rom-list-container">
      <FilterBar />
      <StatusLegend />
      {loading && <p className="hint">Loading…</p>}
      {!loading && !settings.rom_root_path && (
        <p className="hint">No ROM folder configured yet. Open Settings to choose one.</p>
      )}
      {!loading && settings.rom_root_path && roms.length === 0 && (
        <p className="hint">No ROMs found. Try Scan Files.</p>
      )}
      <ul className="rom-list">
        {roms.map((rom) => (
          <RomListItem
            key={rom.id}
            rom={rom}
            selected={rom.id === selectedRomId}
            onSelect={() => setSelectedRomId(rom.id)}
          />
        ))}
      </ul>
    </div>
  );
}
