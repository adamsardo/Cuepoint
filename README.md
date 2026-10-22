# Cuepoint

A pure-Rust DJ application aiming for feature parity with rekordbox, built clean-room from a written spec. See `docs/spec/README.md` for the process and `docs/spec/inventory.md` for the feature map and build order.

## Status

- **Milestone 1, library reader (done):** opens a rekordbox XML export or a rekordbox 6/7 `master.db` read-only. Shows the collection and playlist tree, with search and column sorting.
- **Milestone 2, two-deck playback (done):** each deck has an overview waveform, click-to-seek, play/pause, CDJ-style cue, manual loops and a tempo slider. Changing tempo also changes pitch for now; there's no key lock yet. The two-channel mixer has trim, 3-band EQ, faders, crossfader with A/THRU/B assign, master level and meters. Double-click a track, or right-click it and choose a deck, to load it.
- **Milestone 3, analysis and sync (in progress):** tracks are analysed when loaded for BPM and beatgrid, key, a 3-band colour waveform and loudness. All of it is DSP; the ML models come later. Grids from a rekordbox XML export take priority over our own analysis. Each deck has SYNC and MASTER buttons and ±4 beat jump, and above the decks are scrolling detail waveforms with beat and bar lines.

## Run

```sh
cargo run -p cuepoint -- examples/demo-library.xml
```

To open your own library, export it from rekordbox (File > Export Collection in xml format) and pass that file, or use File > Open in the app.

To open `master.db` directly, set `CUEPOINT_MASTERDB_KEY` to the database key, or enter the key when the app asks for it. Cuepoint never writes to `master.db`.

On Linux, building audio output needs the ALSA headers (`libasound2-dev` on Debian/Ubuntu). Without an audio device, Cuepoint keeps running and tells you output is silent.

## Develop

```sh
cargo test
cargo clippy --all-targets
cargo fmt --check
```

`scripts/setup-env.sh` downloads rekordbox and Ghidra for the reverse-engineering side; `scripts/extract-symbols.sh` rebuilds the symbol index. Neither is needed to build the app.
