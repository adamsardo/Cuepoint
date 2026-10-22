//! Real-time audio engine: decks, mixer, sync, cues, loops, sampler, recording.
//!
//! Milestone 2 covers two decks with play/pause, CDJ-style cue, manual
//! loops, varispeed tempo, and a two-channel mixer, output through `cpal`.

mod audio;
mod deck;
mod engine;
mod mixer;
mod output;
mod sync;

pub use audio::{DecodeError, Pcm, decode_file};
pub use deck::{DECLICK_FRAMES, DeckState};
pub use engine::{Command, DECKS, Engine, EngineHandle, EngineState, MAX_BLOCK, engine};
pub use mixer::{
    CrossfaderAssign, EQ_MAX_DB, EQ_MIN_DB, EqBand, MixerState, TRIM_MAX_DB, TRIM_MIN_DB,
};
pub use output::{AudioOutput, FALLBACK_SAMPLE_RATE, open, open_null};
