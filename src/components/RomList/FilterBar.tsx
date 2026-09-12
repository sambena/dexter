import { useLibrary } from "../../state/useLibraryStore";

export function FilterBar() {
  const { filters, setFilters, systems } = useLibrary();

  return (
    <div className="filter-bar">
      <input
        type="text"
        placeholder="Search…"
        value={filters.search_text ?? ""}
        onChange={(e) => setFilters({ ...filters, search_text: e.target.value })}
      />
      <select
        value={filters.system_id ?? ""}
        onChange={(e) =>
          setFilters({ ...filters, system_id: e.target.value ? Number(e.target.value) : undefined })
        }
      >
        <option value="">All systems</option>
        {systems.map((s) => (
          <option key={s.id} value={s.id}>
            {s.name}
          </option>
        ))}
      </select>
      <select
        value={filters.match_status ?? "all"}
        onChange={(e) => setFilters({ ...filters, match_status: e.target.value })}
      >
        <option value="all">All</option>
        <option value="pending">Not hashed yet</option>
        <option value="matched">Matched</option>
        <option value="unmatched">Unmatched</option>
        <option value="unverifiable">Can't verify</option>
        <option value="error">Errors</option>
      </select>
    </div>
  );
}
