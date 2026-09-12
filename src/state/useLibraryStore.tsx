import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { api } from "../api/tauri";
import type { RomFilter, RomListItemDto, SystemDto } from "../types/rom";
import type { Settings } from "../types/settings";

interface LibraryState {
  roms: RomListItemDto[];
  systems: SystemDto[];
  settings: Settings;
  filters: RomFilter;
  selectedRomId: number | null;
  loading: boolean;
  error: string | null;
  setFilters: (f: RomFilter) => void;
  setSelectedRomId: (id: number | null) => void;
  refreshRoms: () => Promise<void>;
  refreshSystems: () => Promise<void>;
  refreshSettings: () => Promise<void>;
  clearError: () => void;
}

const LibraryContext = createContext<LibraryState | null>(null);

export function LibraryProvider({ children }: { children: ReactNode }) {
  const [roms, setRoms] = useState<RomListItemDto[]>([]);
  const [systems, setSystems] = useState<SystemDto[]>([]);
  const [settings, setSettings] = useState<Settings>({ rom_root_path: null });
  const [filters, setFilters] = useState<RomFilter>({});
  const [selectedRomId, setSelectedRomId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshRoms = useCallback(async () => {
    setLoading(true);
    try {
      const result = await api.listRoms(filters);
      setRoms(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [filters]);

  const refreshSystems = useCallback(async () => {
    try {
      setSystems(await api.listSystems());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const refreshSettings = useCallback(async () => {
    try {
      setSettings(await api.getSettings());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refreshSettings();
    refreshSystems();
  }, [refreshSettings, refreshSystems]);

  useEffect(() => {
    refreshRoms();
  }, [refreshRoms]);

  return (
    <LibraryContext.Provider
      value={{
        roms,
        systems,
        settings,
        filters,
        selectedRomId,
        loading,
        error,
        setFilters,
        setSelectedRomId,
        refreshRoms,
        refreshSystems,
        refreshSettings,
        clearError: () => setError(null),
      }}
    >
      {children}
    </LibraryContext.Provider>
  );
}

export function useLibrary() {
  const ctx = useContext(LibraryContext);
  if (!ctx) throw new Error("useLibrary must be used within a LibraryProvider");
  return ctx;
}
