//! Key detection by chroma template matching (spec 04, section 2.3).
//!
//! A long-window STFT is pooled into semitone bands (so every semitone
//! weighs the same however many FFT bins it spans, i.e. log-frequency
//! weighting), folded into 12 pitch classes and averaged over the track.
//! The average is correlated with the Krumhansl-Kessler probe-tone profiles
//! (Krumhansl, "Cognitive Foundations of Musical Pitch", 1990) rotated to
//! all 24 keys, following Temperley's and Gómez's template approach. Tuning
//! is assumed to be A = 440 Hz.

use cuepoint_core::MusicalKey;

use crate::signal::{Mono, Stft};

const KRUMHANSL_MAJOR: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const KRUMHANSL_MINOR: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Pitch range considered: A1 (55 Hz) to about D#8 (5 kHz).
const MIN_HZ: f64 = 55.0;
const MAX_HZ: f64 = 5000.0;
/// Window in seconds; 0.74 s at 11 kHz resolves semitones from about A1.
const WINDOW_SECONDS: f64 = 0.74;

/// Mean chroma over the track, index 0 = C.
pub(crate) fn chroma(mono: &Mono) -> [f64; 12] {
    let size = ((WINDOW_SECONDS * mono.rate) as usize).next_power_of_two();
    let mut stft = Stft::new(size, size / 2);
    let bin_hz = mono.rate / stft.size() as f64;
    let max_hz = MAX_HZ.min(0.45 * mono.rate);

    // Semitone index (MIDI note) for every bin in range.
    let lowest_note = midi_note(MIN_HZ);
    let notes = (midi_note(max_hz) - lowest_note + 1) as usize;
    let bin_note: Vec<Option<usize>> = (0..=size / 2)
        .map(|k| {
            let hz = k as f64 * bin_hz;
            (MIN_HZ..=max_hz)
                .contains(&hz)
                .then(|| (midi_note(hz) - lowest_note) as usize)
        })
        .collect();

    let mut total = [0f64; 12];
    let mut energy = vec![0f64; notes];
    stft.for_each(&mono.samples, |_, magnitudes| {
        energy.fill(0.0);
        for (m, note) in magnitudes.iter().zip(&bin_note) {
            if let Some(n) = note {
                energy[*n] += f64::from(m * m);
            }
        }
        for (n, e) in energy.iter().enumerate() {
            total[(lowest_note as usize + n) % 12] += e.sqrt();
        }
    });
    total
}

/// Nearest MIDI note number for a frequency.
fn midi_note(hz: f64) -> i32 {
    (69.0 + 12.0 * (hz / 440.0).log2()).round() as i32
}

fn correlation(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let mean_a = a.iter().sum::<f64>() / 12.0;
    let mean_b = b.iter().sum::<f64>() / 12.0;
    let (mut cov, mut var_a, mut var_b) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        cov += (x - mean_a) * (y - mean_b);
        var_a += (x - mean_a).powi(2);
        var_b += (y - mean_b).powi(2);
    }
    if var_a <= 0.0 || var_b <= 0.0 {
        return 0.0;
    }
    cov / (var_a * var_b).sqrt()
}

/// The best matching key for a chroma vector, or `None` if it is flat.
pub(crate) fn best_key(chroma: &[f64; 12]) -> Option<MusicalKey> {
    if chroma.iter().all(|&c| c <= 0.0) {
        return None;
    }
    let mut best: Option<(f64, MusicalKey)> = None;
    for tonic in 0..12u8 {
        for (profile, minor) in [(&KRUMHANSL_MAJOR, false), (&KRUMHANSL_MINOR, true)] {
            // Rotate the profile so its tonic lands on `tonic`.
            let rotated: [f64; 12] =
                std::array::from_fn(|pc| profile[(pc + 12 - usize::from(tonic)) % 12]);
            let score = correlation(chroma, &rotated);
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, MusicalKey::new(tonic, minor)));
            }
        }
    }
    best.filter(|(score, _)| *score > 0.0).map(|(_, key)| key)
}
