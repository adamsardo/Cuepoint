# Handoff

Cuepoint is a clean-room, pure-Rust rekordbox 7.2.19 rewrite. The plan is in `docs/spec/inventory.md` (decisions, build order) and `docs/spec/subsystems/01-07` (specs).

## Status

| Milestone | State |
|---|---|
| Spec (7 subsystem specs, about 1,340 parity items, 168 Ghidra questions) | Merged |
| M1 Library reader (rekordbox XML, master.db via SQLCipher, egui browser) | Merged ([adamsardo/Cuepoint#3](https://github.com/adamsardo/Cuepoint/pull/3)) |
| M2 Two-deck playback (engine, CDJ cue, loops, mixer, no-alloc audio thread) | Merged ([adamsardo/Cuepoint#4](https://github.com/adamsardo/Cuepoint/pull/4)) |
| M3 Analysis (BPM/grid, key, colour waveform, loudness) and beat sync | Merged ([adamsardo/Cuepoint#5](https://github.com/adamsardo/Cuepoint/pull/5)) |
| M4 USB export (export.pdb, ANLZ .DAT/.EXT, Export menu, `cuepoint export` command) | Merged ([adamsardo/Cuepoint#6](https://github.com/adamsardo/Cuepoint/pull/6)) after three Codex review rounds (six findings fixed, final round clean) |
| Export-screen UI parity (rekordbox's Export mode, one-to-one) | Branch `claude/export-ui-parity`, not yet a PR |

All 111 tests pass (1 ignored). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean on the pinned toolchain.

## Focus

Hardware work is on hold (no CDJ or controller to test with). Work on library management, track analysis and preparation, up to USB export. Visual parity with rekordbox comes first.

## Next steps

1. Review and open a PR for `claude/export-ui-parity`.
2. UI states that need the user present (full-screen control approval): the track-list and tree right-click menus, the EXPORT mode and layout dropdowns, and Preferences. rekordbox draws its own menus; Cuepoint still uses egui's.
3. Library editing with write-back: playlist create/rename/delete, adding tracks by drag, cue and grid edits saved (grid edits currently live only until the track is reloaded).
4. Background analysis of the whole collection (grid, key and waveform cached like the previews), and reading `djmdCue` from `master.db`.
5. Hardware and Performance mode (Milestone 5) when hardware is available.

## UI parity method

- Capture both windows at 2x with `screencapture -l <window id>`, crop matching regions, and compare text boxes and colour runs. The measured positions are in comments in `crates/cuepoint-ui/src/{theme,export_view,browser}.rs`.
- `CUEPOINT_DEV_PLAYLIST`, `CUEPOINT_DEV_LOAD`, `CUEPOINT_DEV_SEEK` and `CUEPOINT_DEV_TAB` (`grid`, `info`, `memory`, `artwork`) preload a state so it matches a rekordbox capture.
- Arial is loaded from macOS at runtime (nothing bundled). The title bar and traffic lights are drawn by the app, at rekordbox's 28 pt and 12 pt sizes.
- The 3-band waveform analysis was fitted to rekordbox's `PWV7` detail data (see `crates/cuepoint-analysis/src/waveform.rs`); the overview and list previews use measured band-height ratios.
- Previews are cached in `~/Library/Caches/Cuepoint/previews`.

## Open items

- **OneLibrary (`exportLibrary.db`):** SQLCipher-encrypted. Plan: the user supplies the key, as for `master.db`.
- **Export behaviour:**
  - Re-export keeps audio already on the stick only when the bytes match.
  - Every file (audio, ANLZ, `export.pdb`) is written to `.cuepoint-N.tmp` first and moved into place only after the whole export is ready. A cancelled or failed export leaves the previous export as it was. Replaced files are set aside and restored if a move fails. Replaced audio therefore needs free space twice over until the export finishes.
  - Bit depth comes from the source file (WAV/AIFF/FLAC/ALAC). Lossy formats are written as 16, an assumption that hasn't been checked against rekordbox.
  - Device paths are capped at 255 UTF-16 units: artist and album folders at 64 each, and the file name is trimmed to fit.
- **Export gaps:** cues (written as empty lists, though XML cues are now read), artwork, PSSI phrases, PQT2, PVBR seek data, `exportExt.pdb`.
- **UI gaps:** phrase edit, the analysis lock, the third and fourth list-layout buttons, the source rail's other panels (Explorer, devices, history), streaming-service icons (drawn as plain glyphs, not brand logos), and column resizing.
- **Placeholder values:** mixer EQ and fader curves are generic DJ-mixer values, not rekordbox's. There is no key lock (needs the time-stretcher).
- **Sync scope:** beat sync only, not bar sync. Analysis is DSP; the ML models are Milestone 7.
- **Decided:**
  - pure Rust, with two temporary exceptions: SQLCipher and isolated platform FFI
  - our own Ableton Link implementation
  - `candle` for ML inference
  - we add HE-AAC to `symphonia`
  - MIT OR Apache-2.0 licence

## Environment

- **Build:** `cargo test` / `cargo run -p cuepoint -- examples/demo-library.xml`. On Linux, building needs `libasound2-dev`.
- **Toolchain:** `rust-toolchain.toml` pins Rust 1.99.1, and rustup installs it on first use. egui 0.37 needs at least 1.95.
- **Reverse engineering:** `scripts/setup-env.sh` downloads rekordbox and Ghidra to `/opt`. `scripts/extract-symbols.sh` rebuilds the symbol index outside the repo. REA setup: `npx -y rea-agents@6.7.0 setup --client claude_code --dry-run`. A full Ghidra pass on the arm64 slice needs a heap of about 8 GB and roughly an hour, and REA doesn't keep the analysed database between runs.
- **Clean-room rule:** no AlphaTheta code, weights, artwork, mapping files or keys in the repo.
- **Test-only dependency:** `rekordcrate` (MPL-2.0) is a dev-dependency for checking export output. It is never shipped.
