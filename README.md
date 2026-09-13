# Dexter

A Windows app for managing a ROM collection. Dexter scans your ROM folders, verifies every file against No-Intro and Redump DATs, names games properly, downloads box art, finds duplicates, and launches games in RetroArch or standalone emulators. It can also export the whole library to RetroArch as playlists with thumbnails.

Built with [Tauri 2](https://tauri.app/) (Rust) and React + TypeScript.

## Features

### Scanning and verification
- **Scan Files** finds ROMs under a root folder with one subfolder per system (`Roms\NES`, `Roms\SNES`, …), including files inside `.zip` archives and on network shares. Saves, videos and emulator tool folders are skipped.
- **Hash & Match** hashes each file (CRC32, MD5, SHA1) and matches it against imported DATs. Import a DAT file, a whole folder of DATs, or fetch one automatically where a direct source is known.
- Files that aren't byte-for-byte the DAT's dump are still matched when they're only packaged differently:
  - copier headers and trailers, including 512-byte headers on any system
  - overdumps, trimmed dumps (padding restored), mirrored NES dumps and interleaved SNES dumps
  - cue sheets and `.gdi` files, verified through their tracks
  - `.ecm` images, decoded while hashing
  - GameCube `.rvz` images, decompressed while hashing
- Wii U title folders are identified from their own `app.xml` and `meta.xml`, telling games, updates and DLC apart. GameCube and Wii disc images are identified by the game ID and revision in their headers.
- Every file gets a status: **matched**, **unmatched** (hashed, but no DAT dump matches) or **can't verify** (a format no DAT can confirm, such as `.wbfs`, `.chd`, `.nsp`, or an unpacked Wii U folder).

### Box art
- Downloads box art from the [libretro thumbnails](https://github.com/libretro-thumbnails) project for every matched game.
- When no image has the exact DAT name, the closest one is used: same title, sharing a region, preferring the plain release. Virtual Console titles look in the original console's set.
- Unmatched and can't-verify files get art guessed from their file name. The details panel flags it as guessed, and a later DAT match replaces it.
- **Add Box Art…** sets any image by hand.

### Library tools
- **Duplicates** lists byte-identical files, GameCube/Wii discs with the same game ID in different formats, and altered copies (`[a1]`, `[f1]`, `[b1]`, …) of games you already have verified. The copy to keep is chosen for you.
- **Rename** renames files to their DAT names, keeping saves alongside, and never renames files a cue sheet loads.
- **Deleting** sends files to the Recycle Bin. On network shares, which have no Recycle Bin, they're moved to a `_Deleted by Dexter` folder in the ROM root so they can be restored.
- **RetroArch** writes a RetroArch playlist for each system that launches with RetroArch, and copies box art into RetroArch's thumbnails folder under the names RetroArch looks for.

### Emulators
- Detects RetroArch, its installed cores and common standalone emulators (Dolphin, Cemu, Ryujinx, …).
- **Set up automatically** picks the best installed option for each system. Any system can be set by hand to a RetroArch core, a detected emulator, or a custom command line.
- **Play** launches the selected game directly.

### Storage
- The library database and box art live in `%APPDATA%\com.sambena.rom-manager` by default. Either can be moved to another folder from **Settings → Storage**, and the existing files move with it.

## Installing

Download `Dexter_<version>_x64-setup.exe` from [Releases](https://github.com/sambena/rom-manager/releases) and run it. It installs for the current user and adds Dexter to the Start menu. Each release also has an `.msi`, a portable `.exe` that runs without installing, and `dexter-cli`.

To install a build of your own, run the installer from `src-tauri\target\release\bundle\nsis\` after building (see below).

Dexter needs the WebView2 runtime, which is already part of Windows 10 and 11.

## First run

1. **Settings → ROM Library**: choose your ROM root folder.
2. **Settings → DAT Files**: import DATs for your systems. No-Intro DATs come from [DAT-o-MATIC](https://datomatic.no-intro.org/) and Redump DATs from [redump.org](http://redump.org/downloads/).
3. **Scan Files**, then **Hash & Match**.
4. **Download Box Art**.
5. **Settings → Emulators**: **Set up automatically**, or choose per system.

## Building

Requirements:
- [Rust](https://rustup.rs/) (stable, MSVC toolchain)
- [Node.js](https://nodejs.org/) 20+
- Microsoft C++ Build Tools and WebView2, as listed in [Tauri's prerequisites](https://tauri.app/start/prerequisites/)

```bash
npm install
npm run tauri dev      # run with hot reload
npm run tauri build    # release build and installers
```

The release build writes:
- `src-tauri\target\release\rom-manager.exe`: the app itself, which runs without installing
- `src-tauri\target\release\dexter-cli.exe`: the command-line client
- `src-tauri\target\release\bundle\nsis\*.exe` and `bundle\msi\*.msi`: the installers

Run the tests with:

```bash
cd src-tauri
cargo test
```

## Command line and local API

While Dexter is open, it serves a local HTTP API on `127.0.0.1`. The port and a bearer token are written to `%APPDATA%\com.sambena.rom-manager\api.json` each time it starts. Every action in the window is available as `POST /api/<command>`, and responses look like `{"ok": true, "result": …}` or `{"ok": false, "error": "…"}`.

`dexter-cli` runs the same commands from a terminal. It sends them to the open app so the window updates, or, with the app closed, runs them against the library directly.

```bash
dexter-cli list-roms --filter.system_id 3
dexter-cli hash-pending-roms
dexter-cli launch-rom --rom-id 42
dexter-cli export-retroarch-playlists
```

Run `dexter-cli` with no arguments to list every command and its arguments.

## Project layout

```
src/                     React front end
  components/            top bar, ROM list, details panel, Settings and Tools dialogs
  api/tauri.ts           typed wrappers for the Rust commands
src-tauri/src/
  commands/              Tauri commands: scan, hash, art, maintenance, emulators, export, …
  scanner/               file discovery, hashing, repairs, cue/ECM/RVZ readers, Wii U and disc headers
  dat/                   DAT parsing and file name metadata
  db/                    SQLite schema (with migrations) and queries
  art/                   libretro thumbnail lookup
  emulators/             RetroArch cores, emulator detection, playlists
  api/                   command registry and local HTTP server
  cli.rs, bin/           dexter-cli
  storage.rs             library database and box art locations
```
