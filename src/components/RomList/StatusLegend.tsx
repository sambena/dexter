const ITEMS: Array<{ status: string; label: string; hint: string }> = [
  { status: "matched", label: "Matched", hint: "Hash matched a known entry in an imported DAT." },
  { status: "unmatched", label: "Unmatched", hint: "Hashed, but no DAT entry matched — see a ROM's details for why." },
  { status: "pending", label: "Not hashed", hint: "Found by Scan Files, waiting for Hash & Match to run." },
  { status: "error", label: "Error", hint: "Hashing failed (e.g. a network read error) — Hash & Match will retry it automatically." },
];

export function StatusLegend() {
  return (
    <div className="status-legend">
      {ITEMS.map((item) => (
        <span key={item.status} className="status-legend-item" title={item.hint}>
          <span className={`match-dot ${item.status}`} />
          {item.label}
        </span>
      ))}
    </div>
  );
}
