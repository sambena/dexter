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
// for the same games — worth calling out directly since there's no way to
// know this without hitting a wall of "unmatched".
const N64_HINT =
  "No-Intro splits Nintendo 64 into BigEndian (.z64), ByteSwapped (.v64) and LittleEndian (.n64) DATs, and the same game hashes differently in each. Import the one matching your files' format.";
const THREE_DS_HINT =
  "No-Intro has separate Encrypted and Decrypted DATs for Nintendo 3DS, and a game hashes differently in each. Most dumps shared today are decrypted (for Citra/Azahar), so if you imported the Encrypted DAT, import the Decrypted one too.";
const MULTI_VARIANT_SYSTEM_HINTS: Record<string, string> = {
  n64: N64_HINT,
  nintendo64: N64_HINT,
  "3ds": THREE_DS_HINT,
  nintendo3ds: THREE_DS_HINT,
};

function normalizeFolderKey(folderName: string): string {
  return folderName.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${+(bytes / (1024 * 1024)).toFixed(2)} MB`;
  if (bytes >= 1024) return `${+(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} bytes`;
}

/** Why a matched file isn't byte-for-byte the DAT's dump, when that's the case. */
function matchNoteExplanation(details: RomDetailsDto): string | null {
  const trailer = details.trailer_size ?? 0;
  switch (details.match_note) {
    case "overdump":
      return `Overdump: the verified game data is followed by ${formatBytes(trailer)} of extra data from reading the cartridge past its end. Emulators ignore it, so it plays the same as the verified dump.`;
    case "header":
      return `This file has a ${formatBytes(details.header_size ?? 0)} header in front of the game data, added by an old copier device. Without it, the data is the verified dump.`;
    case "mirrored":
      return "This old dump stores the cartridge's chips twice over (a mirrored dump). With the repeats removed, it's the verified dump.";
    case "cue-tracks":
      return "This cue sheet's own text differs from Redump's (usually just the track file names), but every track it loads is a verified dump of this game.";
    default:
      return null;
  }
}

// GoodTools marked altered copies with bracketed codes. No-Intro only lists
// unaltered dumps, so these explain themselves.
const GOODTOOLS_FLAGS: [RegExp, string][] = [
  [/\[b\d*\]/i, "a bad dump (damaged or incomplete)"],
  [/\[f\d*\]/i, "a \"fixed\" copy, changed so it runs on old emulators or copiers"],
  [/\[h[^\]]*\]/i, "a hack (modified game)"],
  [/\[p\d*\]/i, "a pirate release"],
  [/\[t\d*\]/i, "a copy with a cheat trainer added"],
  [/\[o\d*\]/i, "an overdump"],
  [/\[a\d*\]/i, "an alternate dump that differs from the verified one"],
  [/\[T[+-][^\]]*\]/, "a fan translation"],
];

function goodToolsExplanation(fileName: string): string | null {
  const found = GOODTOOLS_FLAGS.filter(([pattern]) => pattern.test(fileName)).map(([, meaning]) => meaning);
  if (!found.length) return null;
  return `The file name's GoodTools tags mark it as ${found.join(" and ")}. DATs only list unaltered dumps, so a copy like this won't match. The verified version of the game can replace it.`;
}

function matchStatusExplanation(details: RomDetailsDto, hasDat: boolean, folderName?: string): string | null {
  switch (details.match_status) {
    case "matched":
      return matchNoteExplanation(details);
    case "pending":
      return "Found by Scan Files but not hashed yet — run Hash & Match.";
    case "unverifiable":
      return details.archive_member == null && !/\.[a-z0-9]{1,6}$/i.test(details.file_name)
        ? "This is an extracted title folder (thousands of files), which no DAT describes as one dump, so it can't be verified."
        : /\.(nsp|xci|nsz|xcz)$/i.test(details.file_name)
          ? "Switch game files can't be checked against a DAT: every dump carries data specific to the console or dumping tool (tickets, cartridge padding), so no two copies of a game hash the same."
          : "This format can't be checked against a DAT: compressed or trimmed disc images (.rvz, .wbfs, .chd, …) store the disc re-encoded, and Dexter can't look inside .rar or .7z. To verify it, convert it back to the original dump (e.g. .iso, or .bin/.cue) and re-scan.";
    case "error":
      return "Hashing failed last time (e.g. a network read error). Hash & Match will retry it automatically.";
    case "unmatched": {
      if (!hasDat) {
        return "No DAT has been imported for this system yet, so there's nothing to match against. Import or fetch one in Settings, then re-run Hash & Match.";
      }
      const base =
        "This file's hash didn't match any entry in the imported DAT for this system — it may be a modified/bad dump, a version the DAT doesn't list, or a homebrew/hack.";
      const hints = [
        goodToolsExplanation(details.archive_member ?? details.file_name),
        folderName ? MULTI_VARIANT_SYSTEM_HINTS[normalizeFolderKey(folderName)] : undefined,
        details.header_size != null
          ? "The file has a header, and neither the whole file nor the data without it matched. For NES, No-Intro's Headerless DAT is the most reliable, since old dumps often have junk in their headers."
          : undefined,
      ].filter(Boolean);
      return [base, ...hints].join(" ");
    }
    default:
      return null;
  }
}

export function DetailsPanel() {
  const { selectedRomId, systems, setSelectedRomId, refreshRoms } = useLibrary();
  const { startJob, endJob } = useJob();
  const [details, setDetails] = useState<RomDetailsDto | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  async function launch(rom: RomDetailsDto) {
    setLaunching(true);
    setActionError(null);
    try {
      await api.launchRom(rom.id);
    } catch (e) {
      setActionError(String(e));
    } finally {
      setLaunching(false);
    }
  }

  async function deleteRom(rom: RomDetailsDto) {
    const ok = window.confirm(
      `Delete this file?

${rom.file_path}

` +
        `Files on this PC go to the Recycle Bin. Files on a network share are moved to a ` +
        `"_Deleted by Dexter" folder in your ROM folder, so they can still be restored.`,
    );
    if (!ok) return;
    setDeleting(true);
    setActionError(null);
    startJob("Deleting ROM");
    try {
      const summary = await api.deleteRoms([rom.id]);
      if (summary.succeeded > 0) {
        setSelectedRomId(null);
      } else {
        setActionError(summary.errors.join("; ") || "Nothing was deleted.");
      }
      await refreshRoms();
    } catch (e) {
      setActionError(String(e));
    } finally {
      endJob();
      setDeleting(false);
    }
  }

  useEffect(() => {
    if (selectedRomId == null) {
      setDetails(null);
      return;
    }
    let cancelled = false;
    setActionError(null);
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
          {details.emulator_core ? (
            `RetroArch · ${details.emulator_core} core`
          ) : details.emulator_path ? (
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

        {details.header_size != null && (
          <>
            <dt>Skipped</dt>
            <dd title="Bytes that aren't part of the game data DATs hash, so matching leaves them out.">
              {[
                details.header_size ? `${formatBytes(details.header_size)} header` : null,
                details.trailer_size
                  ? details.match_note === "mirrored"
                    ? `${formatBytes(details.trailer_size)} of repeated chip data`
                    : `${formatBytes(details.trailer_size)} at the end`
                  : null,
              ]
                .filter(Boolean)
                .join(", ") || "Nothing"}
              {details.headerless_crc32 && (
                <>
                  {" "}
                  (CRC32 of the rest: <span className="mono">{details.headerless_crc32}</span>)
                </>
              )}
            </dd>
          </>
        )}

        <dt>MD5</dt>
        <dd className="mono">{details.md5 ?? "—"}</dd>

        <dt>SHA1</dt>
        <dd className="mono">{details.sha1 ?? "—"}</dd>
      </dl>
      <div className="details-actions">
        <button
          onClick={() => launch(details)}
          disabled={launching || !(details.emulator_path || details.emulator_core)}
          className="primary"
          title={
            details.emulator_path || details.emulator_core
              ? undefined
              : `No emulator is set for ${details.system_name ?? "this system"}. Choose one in Settings → Emulators.`
          }
        >
          {launching && <span className="spinner inline" aria-hidden="true" />}
          Play
        </button>
        <button onClick={() => deleteRom(details)} disabled={deleting} className="danger">
          {deleting && <span className="spinner inline" aria-hidden="true" />}
          Delete…
        </button>
        <span className="hint details-actions-note">Deleted files can be restored.</span>
      </div>
      {actionError && <p className="hint box-art-error">{actionError}</p>}
    </div>
  );
}
