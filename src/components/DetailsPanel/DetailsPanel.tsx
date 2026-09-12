import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import type { RomDetailsDto } from "../../types/rom";

export function DetailsPanel() {
  const { selectedRomId } = useLibrary();
  const [details, setDetails] = useState<RomDetailsDto | null>(null);

  useEffect(() => {
    if (selectedRomId == null) {
      setDetails(null);
      return;
    }
    let cancelled = false;
    api.getRomDetails(selectedRomId).then((d) => {
      if (!cancelled) setDetails(d);
    });
    return () => {
      cancelled = true;
    };
  }, [selectedRomId]);

  if (!details) {
    return (
      <div className="details-panel details-empty">
        <p className="hint">Select a ROM to see its details.</p>
      </div>
    );
  }

  return (
    <div className="details-panel">
      <h2>{details.display_name}</h2>
      <dl>
        <dt>File</dt>
        <dd>
          {details.file_path}
          {details.archive_member ? ` :: ${details.archive_member}` : ""}
        </dd>

        <dt>System</dt>
        <dd>{details.system_name ?? "Unknown"}</dd>

        <dt>Year</dt>
        <dd>{details.year ?? "—"}</dd>

        <dt>Region</dt>
        <dd>{details.region ?? "—"}</dd>

        <dt>Match status</dt>
        <dd>
          <span className={`match-dot ${details.match_status}`} /> {details.match_status}
        </dd>

        <dt>Emulator</dt>
        <dd>
          {details.emulator_path ? (
            <>
              {details.emulator_path}
              {details.emulator_args ? ` ${details.emulator_args}` : ""}
            </>
          ) : (
            "Not configured"
          )}
        </dd>

        <dt>CRC32</dt>
        <dd className="mono">{details.crc32 ?? "—"}</dd>

        <dt>MD5</dt>
        <dd className="mono">{details.md5 ?? "—"}</dd>

        <dt>SHA1</dt>
        <dd className="mono">{details.sha1 ?? "—"}</dd>
      </dl>
    </div>
  );
}
