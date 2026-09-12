import { useEffect, useState } from "react";
import { api } from "../../api/tauri";
import { useLibrary } from "../../state/useLibraryStore";
import { useJob } from "../../state/useJobStore";
import type { RomDetailsDto } from "../../types/rom";
import { basename } from "../../utils/path";

function outerArchivePath(filePath: string, archiveMember: string): string {
  return filePath.slice(0, filePath.length - archiveMember.length - 2);
}

function BoxArt({ details }: { details: RomDetailsDto }) {
  const [boxArt, setBoxArt] = useState<string | null>(null);
  const [knownSource, setKnownSource] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { startJob, endJob } = useJob();

  useEffect(() => {
    setBoxArt(null);
    setError(null);
    api.getBoxArt(details.id).then(setBoxArt);
    if (details.system_name) {
      api.hasKnownBoxArtSource(details.system_name).then(setKnownSource);
    } else {
      setKnownSource(false);
    }
  }, [details.id, details.system_name]);

  async function fetchArt() {
    setBusy(true);
    setError(null);
    startJob("Downloading box art");
    try {
      setBoxArt(await api.fetchBoxArt(details.id));
    } catch (e) {
      setError(String(e));
    } finally {
      endJob();
      setBusy(false);
    }
  }

  async function pickArt() {
    setBusy(true);
    setError(null);
    startJob("Setting box art");
    try {
      setBoxArt(await api.pickAndSetBoxArt(details.id));
    } catch (e) {
      setError(String(e));
    } finally {
      endJob();
      setBusy(false);
    }
  }

  return (
    <div className="box-art-section">
      {boxArt ? (
        <img className="box-art" src={boxArt} alt={`${details.display_name} box art`} />
      ) : (
        <div className="box-art placeholder">No box art</div>
      )}
      <div className="box-art-actions">
        <button
          onClick={fetchArt}
          disabled={busy || details.match_status !== "matched" || !knownSource}
          title={
            details.match_status !== "matched"
              ? "This ROM isn't matched to a DAT entry yet — run Hash & Match, making sure a DAT is imported for this system, then try again."
              : !knownSource
                ? "No known box art source for this system yet."
                : undefined
          }
        >
          {busy ? (
            <>
              <span className="spinner inline" aria-hidden="true" />
              Downloading…
            </>
          ) : (
            "Download Box Art"
          )}
        </button>
        <button onClick={pickArt} disabled={busy}>
          Add Box Art…
        </button>
      </div>
      {error && <p className="hint box-art-error">{error}</p>}
    </div>
  );
}

// Systems No-Intro splits into multiple DAT variants that hash differently
// for the same games (usually by byte order) — worth calling out directly
// since there's no way to know this without hitting a wall of "unmatched".
const MULTI_VARIANT_SYSTEM_HINTS: Record<string, string> = {
  n64: "Nintendo 64 is split by No-Intro into separate BigEndian/ByteSwapped/LittleEndian DAT sets — the same game hashes differently in each. If you've only imported one variant, try importing the others too (a system can now have multiple DATs).",
  nintendo64: "Nintendo 64 is split by No-Intro into separate BigEndian/ByteSwapped/LittleEndian DAT sets — the same game hashes differently in each. If you've only imported one variant, try importing the others too (a system can now have multiple DATs).",
};

function normalizeFolderKey(folderName: string): string {
  return folderName.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function matchStatusExplanation(details: RomDetailsDto, hasDat: boolean, folderName?: string): string | null {
  switch (details.match_status) {
    case "matched":
      return null;
    case "pending":
      return "Found by Scan Files but not hashed yet — run Hash & Match.";
    case "error":
      return "Hashing failed last time (e.g. a network read error). Hash & Match will retry it automatically.";
    case "unmatched": {
      if (!hasDat) {
        return "No DAT has been imported for this system yet, so there's nothing to match against. Import or fetch one in Settings, then re-run Hash & Match.";
      }
      const base =
        "This file's hash didn't match any entry in the imported DAT for this system — it may be a modified/bad dump, a version the DAT doesn't list, or a homebrew/hack.";
      const hint = folderName ? MULTI_VARIANT_SYSTEM_HINTS[normalizeFolderKey(folderName)] : undefined;
      return hint ? `${base} ${hint}` : base;
    }
    default:
      return null;
  }
}

export function DetailsPanel() {
  const { selectedRomId, systems } = useLibrary();
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
      <div className="details-header">
        <BoxArt details={details} />
        <div>
          <h2>{details.display_name}</h2>
          {details.metadata_guessed && (
            <p className="guessed-note" title="This title/region/year is guessed from the filename, not confirmed by a DAT match.">
              Guessed from filename, not verified
            </p>
          )}
        </div>
      </div>
      <dl>
        <dt>File</dt>
        <dd
          className="truncate"
          title={details.file_path + (details.archive_member ? ` :: ${details.archive_member}` : "")}
        >
          {details.archive_member
            ? `${basename(outerArchivePath(details.file_path, details.archive_member))} → ${details.archive_member}`
            : details.file_name}
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
          {(() => {
            const system = systems.find((s) => s.id === details.system_id);
            const explanation = matchStatusExplanation(details, !!system?.has_dat, system?.folder_name);
            return explanation ? <p className="match-explanation">{explanation}</p> : null;
          })()}
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
