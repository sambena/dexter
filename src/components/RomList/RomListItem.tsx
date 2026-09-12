import type { RomListItemDto } from "../../types/rom";

export function RomListItem({
  rom,
  selected,
  onSelect,
}: {
  rom: RomListItemDto;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <li className={`rom-list-item${selected ? " selected" : ""}`} onClick={onSelect}>
      <span className={`match-dot ${rom.match_status}`} title={rom.match_status} />
      <span className="rom-name">{rom.display_name}</span>
      {rom.system_name && <span className="system-badge">{rom.system_name}</span>}
    </li>
  );
}
