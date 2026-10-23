//! Shared domain types: tracks, beatgrids, keys, cues, loops, units of time.

mod beatgrid;
mod cue;
mod key;
mod playlist;
mod track;

pub use beatgrid::{BEATS_PER_BAR, BeatGrid, EDIT_BPM_RANGE, TempoMarker};
pub use cue::Cue;
pub use key::MusicalKey;
pub use playlist::{PlaylistKind, PlaylistNode};
pub use track::{Bpm, Rating, Track, TrackColour, TrackId};
