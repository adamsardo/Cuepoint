# Master feature inventory

Target: rekordbox 7.2.19 for macOS (`com.pioneerdj.rekordboxdj`, build 7.2.19.0342, minimum macOS 12). Universal binary; analysis uses the arm64 slice (135 MB, SHA-256 `2dfa9738…f907`).

## What rekordbox is made of

- **One C++ app built on JUCE.** About 288,000 symbols survive in the shipped binary, giving roughly 6,000 named C++ classes. Main namespaces: `rbxfrm` (audio engine), `djplay` (decks, performance UI, settings, HID), `db` and `rb::middleware::db` (library), `browse` (browser UI), `analyzer` (track analysis), `streaming` (streaming services), `editapp` (EDIT mode), `prodjlink`, `dmx` and `lighting_db`.
- **Bundled native libraries:** SQLCipher, OpenSSL, mpg123, TagLib, ICU, harfbuzz, TensorFlow 2.16, ONNX Runtime 1.18, Demucs and Spleeter wrappers, Syphon, FTDI D2XX (DMX), Bugsnag, and `libFilSiNE` (NFS server for Link Export).
- **Helper processes:** `rekordboxAgent` (Electron, local HTTP API on port 30001, cloud sync), `edb_streamd` and `kill_daemon` (DeviceSQL daemon), `rb_http_server` (OAuth login listener), `rbinit` (licence manager setup), `Upmgr` (updater).
- **Resources:** encrypted `master.db` and `product.db` templates, ML models, 69 controller mapping CSVs, 18 UI languages, 78 MB of skins, sample packs and a lighting macro database.

## Subsystem map

| # | Subsystem | Spec | Parity checklist items | Open Ghidra questions | Crates |
|---|---|---|---|---|---|
| 1 | Library and database | [01](subsystems/01-library-database.md) | ~126 | 20 | `cuepoint-library` (+ optional `cuepoint-sqlcipher`) |
| 2 | Device export and Pro DJ Link | [02](subsystems/02-device-export-prodjlink.md) | ~169 | 22 | `cuepoint-export`, `cuepoint-prolink` |
| 3 | Audio engine | [03](subsystems/03-audio-engine.md) | ~245 | 40 | `cuepoint-engine`, `cuepoint-dsp`, `cuepoint-core` |
| 4 | Analysis and ML | [04](subsystems/04-analysis-ml.md) | ~132 | 23 | `cuepoint-analysis` |
| 5 | UI, modes and preferences | [05](subsystems/05-ui.md) | ~258 | 21 | `cuepoint-ui`, `cuepoint-render` (proposed) |
| 6 | Controllers, lighting and video | [06](subsystems/06-controllers-lighting-video.md) | ~239 | 20 | `cuepoint-control`, `cuepoint-lighting`, `cuepoint-video` |
| 7 | Online services and cloud | [07](subsystems/07-online-services.md) | ~171 | 22 | `cuepoint-services` |

Totals: about 1,340 checklist lines and 168 open questions. Item counts are bullet counts, so they include some sub-points.

## Headline findings

- **Library:** about 45 `master.db` tables reconstructed with columns, plus the meaning of the sync bookkeeping columns (`rb_data_status` etc.), which public docs list as unknown. Writes go either straight to SQLite or, with Cloud Library Sync on, through the agent's REST API.
- **Export:** `export.pdb` (DeviceSQL) and `exportLibrary.db` (OneLibrary, SQLCipher) are both produced; 22 OneLibrary tables recorded. Link Export runs its own portmapper, mount, NFS and remote DB server. The `.2EX` vocal section is undocumented publicly.
- **Engine:** an in-binary table of about 620 MIDI-learnable commands acts as a near-complete engine feature list. High-quality stretch runs on a look-ahead worker with a real-time fallback.
- **Analysis:** beat models are madmom-style networks (non-commercial weights, so replaced). Vocal detection, an embedding tagger with genre/energy heads, and Intelligent Cue (SampleCNN + 850-tree booster + per-user trend) are all identified. Stem weights download on demand. Phrase analysis, key and waveforms are DSP.
- **UI:** 4 modes, 10 deck layouts, ~40 dialogs, ~460 persisted settings across 11 preference categories. Metal waveform rendering at display refresh.
- **Controllers:** mapping CSV format fully documented (15 columns, 1,269 function names); 55 models with built-in mappings, 50 HID player classes.
- **Services:** Beatport, TIDAL and SoundCloud are buildable against partner or public APIs. Spotify and Apple Music deck playback depend on closed SDKs and DRM, so they're out. rekordbox Cloud gets a serverless replacement using the user's own storage.

## Not shipped and not reimplemented

AlphaTheta code, model weights, skins, icons, sample packs, mapping CSVs, lighting macros, the rekordbox name and logo. No DRM decryption, licence or activation checks, or telemetry. `DeviceAuth` (player HID handshake) is reimplemented only if Ghidra shows it is not an anti-clone check.

## Decisions

Agreed 2026-10-22.

1. **Pure Rust, with two temporary exceptions.** C is allowed only for SQLite/SQLCipher, behind a trait (`rusqlite` with bundled SQLCipher), until a pure-Rust SQLCipher page codec replaces it. Platform FFI (CoreAudio via `cpal`, Syphon, IOHID) lives in small, isolated crates that may use `unsafe`.
2. **Ableton Link:** our own clean-room Rust implementation of the wire protocol, built from observed network behaviour. No GPL code.
3. **ML inference:** `candle`.
4. **HE-AAC:** we add HE-AAC (SBR/PS) decoding to `symphonia` ourselves and offer it upstream.
5. **Licence:** MIT OR Apache-2.0.

## Build order

Each milestone ends with parity tests against real rekordbox output.

1. **Library reader:** open a real `master.db` and rekordbox XML, browse tracks and playlists in an egui shell.
2. **Two-deck playback:** decode, play, cue, loop, tempo, two-channel mixer, audio out.
3. **Analysis:** beatgrid, BPM, key, waveforms, loudness; write ANLZ files. Beat sync.
4. **USB export:** `export.pdb`, OneLibrary and ANLZ that real CDJs accept.
5. **Performance:** 4 decks, all FX families, sampler, sequencer, slicer, MIDI controllers from our own mappings.
6. **Pro DJ Link:** status, sync, Link Export to CDJs.
7. **Advanced analysis:** stems, phrases, vocal, Intelligent Cue, related tracks.
8. **Lighting, video, streaming services and cloud sync.**

## Evidence status

- Symbol, class and string index: done (`scripts/extract-symbols.sh`).
- Full Ghidra analysis of the arm64 slice: running; snapshot target `/opt/rekordbox/7.2.19/analysis/rekordbox-arm64.snapshot`. The 168 open questions are answered against it, highest-impact first (sync columns, stretch and sync algorithms, `DeviceAuth`, waveform rates, smart playlist codes).
