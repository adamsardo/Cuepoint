//! Three-band colour waveform (spec 04, section 2.4).
//!
//! Each point covers 1/150 s and holds a level per band, 0..=255. The bands
//! and their envelopes were fitted to rekordbox 7.2.19's own 3-band detail
//! waveform (`PWV7`, whose bytes run 0..=127) on five library tracks:
//!
//! - low: 400 Hz low-pass; mid: 250 Hz to 1.5 kHz; high: 1.5 kHz high-pass,
//!   each a 4th-order Linkwitz-Riley filter (they overlap on purpose);
//! - per point, the band's peak absolute amplitude, then a peak-hold
//!   envelope that decays by a per-band factor each point;
//! - level = `gain * envelope ^ exponent`, doubled to fill 0..=255.
//!
//! The fitted model tracks rekordbox's bands with correlations of 0.98 to
//! 0.99 (low), 0.95 to 0.97 (mid) and 0.95 to 0.97 (high). Values are
//! absolute, not normalised per track, so quiet tracks draw small.

use crate::filter::{linkwitz_riley_highpass, linkwitz_riley_lowpass};

/// rekordbox's detail ("zoom") waveform rate.
pub const WAVEFORM_POINTS_PER_SECOND: u32 = 150;
const LOW_CUTOFF_HZ: f64 = 400.0;
const MID_LOW_HZ: f64 = 250.0;
const MID_HIGH_HZ: f64 = 1500.0;
const HIGH_CUTOFF_HZ: f64 = 1500.0;

/// Per band: envelope decay per point, exponent and gain (on rekordbox's
/// 0..=127 scale).
const LOW_SHAPE: Shape = Shape {
    decay: 0.95,
    exponent: 0.979,
    gain: 103.0,
};
const MID_SHAPE: Shape = Shape {
    decay: 0.90,
    exponent: 0.883,
    gain: 124.5,
};
const HIGH_SHAPE: Shape = Shape {
    decay: 0.86,
    exponent: 1.411,
    gain: 84.0,
};

#[derive(Debug, Clone, Copy)]
struct Shape {
    decay: f64,
    exponent: f64,
    gain: f64,
}

/// Peak-hold envelope of one band, turned into a 0..=255 level.
#[derive(Debug, Clone, Copy)]
struct Envelope {
    shape: Shape,
    value: f64,
}

impl Envelope {
    fn new(shape: Shape) -> Self {
        Self { shape, value: 0.0 }
    }

    fn next(&mut self, peak: f64) -> u8 {
        self.value = peak.max(self.value * self.shape.decay);
        let level = 2.0 * self.shape.gain * self.value.powf(self.shape.exponent);
        level.round().clamp(0.0, 255.0) as u8
    }
}

/// Level per band per point, 0..=255.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColourWaveform {
    pub points_per_second: u32,
    pub low: Vec<u8>,
    pub mid: Vec<u8>,
    pub high: Vec<u8>,
}

impl ColourWaveform {
    pub fn len(&self) -> usize {
        self.low.len()
    }

    pub fn is_empty(&self) -> bool {
        self.low.is_empty()
    }
}

/// The level a steady band at `amplitude` settles to, for tests.
#[cfg(test)]
pub(crate) fn steady_level(band: usize, amplitude: f64) -> u8 {
    let shape = [LOW_SHAPE, MID_SHAPE, HIGH_SHAPE][band];
    let mut envelope = Envelope::new(shape);
    envelope.next(amplitude)
}

pub(crate) fn colour_waveform(samples: &[f32], sample_rate: u32) -> ColourWaveform {
    let rate = f64::from(sample_rate);
    let pps = u64::from(WAVEFORM_POINTS_PER_SECOND);
    let sr = u64::from(sample_rate);
    let frames = (samples.len() / 2) as u64;
    let points = if sr == 0 {
        0
    } else {
        (frames * pps).div_ceil(sr)
    };

    let mut low_lp = linkwitz_riley_lowpass(rate, LOW_CUTOFF_HZ);
    let mut mid_hp = linkwitz_riley_highpass(rate, MID_LOW_HZ);
    let mut mid_lp = linkwitz_riley_lowpass(rate, MID_HIGH_HZ);
    let mut high_hp = linkwitz_riley_highpass(rate, HIGH_CUTOFF_HZ);
    let mut envelopes = [
        Envelope::new(LOW_SHAPE),
        Envelope::new(MID_SHAPE),
        Envelope::new(HIGH_SHAPE),
    ];

    let mut out = ColourWaveform {
        points_per_second: WAVEFORM_POINTS_PER_SECOND,
        low: Vec::with_capacity(points as usize),
        mid: Vec::with_capacity(points as usize),
        high: Vec::with_capacity(points as usize),
    };
    for point in 0..points {
        // Point `p` covers frames from ceil(p * sr / pps) up to the next
        // point's start, so slices tile the track exactly.
        let start = (point * sr).div_ceil(pps) as usize;
        let end = (((point + 1) * sr).div_ceil(pps)).min(frames) as usize;
        let (mut low, mut mid, mut high) = (0f64, 0f64, 0f64);
        for &[l, r] in samples[2 * start..2 * end].as_chunks::<2>().0 {
            let mono = 0.5 * (f64::from(l) + f64::from(r));
            low = low.max(low_lp.process(mono).abs());
            mid = mid.max(mid_lp.process(mid_hp.process(mono)).abs());
            high = high.max(high_hp.process(mono).abs());
        }
        out.low.push(envelopes[0].next(low));
        out.mid.push(envelopes[1].next(mid));
        out.high.push(envelopes[2].next(high));
    }
    out
}
