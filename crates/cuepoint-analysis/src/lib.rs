//! Track analysis: beatgrid, key, waveforms, phrases, loudness, cue prediction, stems.
//!
//! This milestone is the DSP path of spec 04 (sections 2.2 to 2.4 and 2.7):
//! a constant-tempo beat grid with bar phase, key, a 3-band colour waveform
//! and EBU R128 loudness. Neural models (high precision beats, vocals,
//! phrases, cues) come later.
//!
//! [`analyze`] runs the loudness and waveform passes on their own threads
//! alongside the key and beat passes, which share a mono mix decimated to
//! about 11 kHz.

mod filter;
mod key;
mod loudness;
mod onset;
mod signal;
mod tempo;
mod waveform;

#[cfg(test)]
mod tests;

use std::thread;

use cuepoint_core::{BeatGrid, MusicalKey};

pub use waveform::{ColourWaveform, WAVEFORM_POINTS_PER_SECOND};

use onset::{AttackEnvelope, OnsetEnvelope};

/// Below this RMS level (about -70 dBFS) a track counts as silent: no key
/// and no beats.
const SILENCE_RMS: f64 = 3.2e-4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalysisSettings {
    /// The detected tempo is doubled or halved until it falls in
    /// `bpm_min..bpm_max`. A range narrower than an octave (the default is
    /// just under one: 87.5 doubles to exactly 175) leaves a sliver of
    /// tempos that fit nowhere; those stay just below `bpm_min`.
    pub bpm_min: f64,
    pub bpm_max: f64,
}

impl Default for AnalysisSettings {
    /// rekordbox's default BPM range, 88 to 175.
    fn default() -> Self {
        Self {
            bpm_min: 88.0,
            bpm_max: 175.0,
        }
    }
}

impl AnalysisSettings {
    /// The settings with an unusable range (not finite, not positive, or
    /// empty) replaced by the default.
    fn sanitised(self) -> Self {
        let valid = self.bpm_min.is_finite()
            && self.bpm_max.is_finite()
            && self.bpm_min > 0.0
            && self.bpm_max > self.bpm_min;
        if valid { self } else { Self::default() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// Constant-tempo grid with one marker on the first beat at or after
    /// time 0, its `beat_in_bar` set from downbeat estimation. Empty when
    /// no beat was found (silence, or too short to analyse).
    pub grid: BeatGrid,
    /// Rounded to 2 decimals, or 0.0 when no beat was found.
    pub bpm: f64,
    pub key: Option<MusicalKey>,
    pub waveform: ColourWaveform,
    /// EBU R128 integrated loudness, `None` when everything is gated out.
    pub loudness_lufs: Option<f32>,
}

/// Analyses interleaved stereo `samples` at `sample_rate`.
/// Only the 3-band colour waveform of interleaved stereo `samples`, for
/// previews that do not need the rest of the analysis.
pub fn colour_waveform(samples: &[f32], sample_rate: u32) -> ColourWaveform {
    waveform::colour_waveform(samples, sample_rate)
}

pub fn analyze(samples: &[f32], sample_rate: u32, settings: &AnalysisSettings) -> Analysis {
    let settings = settings.sanitised();
    if sample_rate == 0 {
        return Analysis {
            grid: BeatGrid::default(),
            bpm: 0.0,
            key: None,
            waveform: ColourWaveform::default(),
            loudness_lufs: None,
        };
    }
    thread::scope(|scope| {
        let loudness = scope.spawn(|| loudness::integrated_loudness(samples, sample_rate));
        let waveform = scope.spawn(|| waveform::colour_waveform(samples, sample_rate));

        let mono = signal::downmix_decimate(samples, sample_rate);
        let audible = mono.rms() >= SILENCE_RMS;
        let key = audible
            .then(|| key::best_key(&key::chroma(&mono)))
            .flatten();
        let beats = audible
            .then(|| {
                let onsets = OnsetEnvelope::compute(&mono);
                let attacks = AttackEnvelope::compute(&mono);
                tempo::track(&onsets, &attacks, mono.duration(), &settings)
            })
            .flatten();

        let (grid, bpm) = match beats {
            Some(b) => (
                BeatGrid::constant(b.bpm, b.first_beat, b.beat_in_bar),
                b.bpm,
            ),
            None => (BeatGrid::default(), 0.0),
        };
        Analysis {
            grid,
            bpm,
            key,
            waveform: join(waveform),
            loudness_lufs: join(loudness),
        }
    })
}

/// Joins a scoped thread, passing on any panic.
fn join<T>(handle: thread::ScopedJoinHandle<'_, T>) -> T {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}
