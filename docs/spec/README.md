# Cuepoint spec

Cuepoint is a pure-Rust DJ application aiming for feature parity with rekordbox 7.2.19. These documents are the specification it is built from.

## Clean-room process

1. **Evidence.** Reverse engineering happens against the shipped rekordbox app using REA, Ghidra, symbol tables, strings and bundled resources. Raw evidence (symbol dumps, decompiled output, extracted assets) stays outside this repo, under `/opt/rekordbox/<version>/analysis`. `scripts/setup-env.sh` and `scripts/extract-symbols.sh` regenerate it.
2. **Spec.** Findings are written here in our own words: behaviour, data formats, protocols, parameters and feature lists. No decompiled code is copied into this repo.
3. **Build.** Rust code is written from the spec. Each crate's parity tests check our output against real rekordbox output.

Not shipped, ever: AlphaTheta's code, trained model weights, skin artwork, icons, sample packs, mapping files or the rekordbox name and logo. Keys and secrets found in the binary are never recorded here. DRM and licence checks are not circumvented.

## Layout

- `inventory.md`: master feature inventory and subsystem map.
- `subsystems/NN-*.md`: one spec per subsystem, each with a feature parity checklist, formats, architecture, prior art, open questions for Ghidra, and the Rust plan.
