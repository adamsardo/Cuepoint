//! Cue points and loops stored with a track.

/// A memory cue, hot cue or saved loop.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cue {
    /// Hot cue slot, 0 for A through 7 for H; `None` for a memory cue.
    pub hot_cue: Option<u8>,
    /// Start in seconds from the start of the track.
    pub start: f64,
    /// Loop end in seconds; `None` for a plain cue.
    pub loop_end: Option<f64>,
    /// The user's label, often empty.
    pub name: String,
    /// `0xRRGGBB` when the cue has a colour.
    pub colour: Option<u32>,
}

impl Cue {
    /// "A" to "H" for hot cues.
    pub fn slot_letter(&self) -> Option<char> {
        self.hot_cue
            .filter(|&slot| slot < 8)
            .map(|slot| char::from(b'A' + slot))
    }
}
