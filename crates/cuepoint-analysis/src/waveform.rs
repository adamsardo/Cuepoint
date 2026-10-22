//! Three-band colour waveform (spec 04, section 2.4).
//!
//! The mono mix is split by 4th-order Linkwitz-Riley crossovers at 200 Hz
//! and 2.5 kHz. Each point holds the peak absolute amplitude of each band
//! over 1/150 s, scaled so full scale is 255. Values are absolute, not
//! normalised per track, so quiet tracks draw small.

use crate::filter::{linkwitz_riley_highpass, linkwitz_riley_lowpass};

/// rekordbox's detail ("zoom") waveform rate.
pub const WAVEFORM_POINTS_PER_SECOND: u32 = 150;
const LOW_MID_CROSSOVER_HZ: f64 = 200.0;
const MID_HIGH_CROSSOVER_HZ: f64 = 2500.0;

/// Peak level per band per point, 0..=255.
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

fn to_byte(peak: f64) -> u8 {
    (peak * 255.0).round().clamp(0.0, 255.0) as u8
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

    let mut low_lp = linkwitz_riley_lowpass(rate, LOW_MID_CROSSOVER_HZ);
    let mut low_hp = linkwitz_riley_highpass(rate, LOW_MID_CROSSOVER_HZ);
    let mut high_lp = linkwitz_riley_lowpass(rate, MID_HIGH_CROSSOVER_HZ);
    let mut high_hp = linkwitz_riley_highpass(rate, MID_HIGH_CROSSOVER_HZ);

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
        for frame in samples[2 * start..2 * end].chunks_exact(2) {
            let mono = 0.5 * (f64::from(frame[0]) + f64::from(frame[1]));
            let above_low = low_hp.process(mono);
            low = low.max(low_lp.process(mono).abs());
            mid = mid.max(high_lp.process(above_low).abs());
            high = high.max(high_hp.process(above_low).abs());
        }
        out.low.push(to_byte(low));
        out.mid.push(to_byte(mid));
        out.high.push(to_byte(high));
    }
    out
}
