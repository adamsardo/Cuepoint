# Handoff

Cuepoint is a clean-room, pure-Rust rekordbox 7.2.19 rewrite. The plan is in `docs/spec/inventory.md` (decisions, build order) and `docs/spec/subsystems/01-07` (specs).

## Status

| Milestone | State |
|---|---|
| Spec (7 subsystem specs, about 1,340 parity items, 168 Ghidra questions) | Merged |
| M1 Library reader (rekordbox XML, master.db via SQLCipher, egui browser) | Merged ([adamsardo/Cuepoint#3](https://github.com/adamsardo/Cuepoint/pull/3)) |
| M2 Two-deck playback (engine, CDJ cue, loops, mixer, no-alloc audio thread) | Merged ([adamsardo/Cuepoint#4](https://github.com/adamsardo/Cuepoint/pull/4)) |
| M3 Analysis (BPM/grid, key, colour waveform, loudness) and beat sync | Merged ([adamsardo/Cuepoint#5](https://github.com/adamsardo/Cuepoint/pull/5)) |
| M4 USB export (export.pdb, ANLZ .DAT/.EXT, Export menu, `cuepoint export` command) | Under Codex review: [adamsardo/Cuepoint#6](https://github.com/adamsardo/Cuepoint/pull/6), branch `claude/sweet-lovelace-aobomc` |

All 99 tests pass. `cargo clippy --all-targets` and `cargo fmt --check` are clean.

## Next steps

1. Answer any Codex findings on [adamsardo/Cuepoint#6](https://github.com/adamsardo/Cuepoint/pull/6), then merge.
2. Hardware check: run `cargo run --release -p cuepoint -- export your.xml /Volumes/STICK` and load the stick on a CDJ.
3. Milestone 5, Performance mode: 4 decks, FX families, sampler/sequencer, MIDI controllers (spec 03 and 06).

## Open items

- **OneLibrary (`exportLibrary.db`):** SQLCipher-encrypted. Plan: the user supplies the key, as for `master.db`.
- **Export gaps:** cues (written as empty lists), artwork, PSSI phrases, PQT2, PVBR seek data, `exportExt.pdb`.
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
- **Reverse engineering:** `scripts/setup-env.sh` downloads rekordbox and Ghidra to `/opt`. `scripts/extract-symbols.sh` rebuilds the symbol index outside the repo. REA setup: `npx -y rea-agents@6.7.0 setup --client claude_code --dry-run`. A full Ghidra pass on the arm64 slice needs a heap of about 8 GB and roughly an hour, and REA doesn't keep the analysed database between runs.
- **Clean-room rule:** no AlphaTheta code, weights, artwork, mapping files or keys in the repo.
- **Test-only dependency:** `rekordcrate` (MPL-2.0) is a dev-dependency for checking export output. It is never shipped.
