# Cuepoint

A pure-Rust DJ application aiming for feature parity with rekordbox, built clean-room from a written spec. See `docs/spec/README.md` for the process and `docs/spec/inventory.md` for the feature map and build order.

## Status

Milestone 1, the library reader, is in progress. Cuepoint opens a rekordbox XML export or a rekordbox 6/7 `master.db` read-only, and shows the collection and playlist tree with search and column sorting.

## Run

```sh
cargo run -p cuepoint -- examples/demo-library.xml
```

To open your own library, export it from rekordbox (File > Export Collection in xml format) and pass that file, or use File > Open in the app.

To open `master.db` directly, set `CUEPOINT_MASTERDB_KEY` to the database key, or enter the key when the app asks for it. Cuepoint never writes to `master.db`.

## Develop

```sh
cargo test
cargo clippy --all-targets
cargo fmt --check
```

`scripts/setup-env.sh` downloads rekordbox and Ghidra for the reverse-engineering side; `scripts/extract-symbols.sh` rebuilds the symbol index. Neither is needed to build the app.
