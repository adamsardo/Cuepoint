//! Waveform resampling and the per-entry encodings of the ANLZ waveform
//! sections (`PWAV`, `PWV2`, `PWV3`, `PWV4`, `PWV5`).
//!
//! Input is the analyser's three-band peak waveform (0 to 255 per band).
//! Heights come from the loudest band; whiteness and colour come from the
//! balance between the bands. Exact rekordbox parity is not attempted, since
//! our band analysis differs from rekordbox's; only the encodings are exact.

use cuepoint_analysis::ColourWaveform;

/// Detail sections hold one entry per half-frame, and there are 75 frames
/// per second.
pub(super) const DETAIL_ENTRIES_PER_SECOND: u32 = 150;

/// Peak level of each band over a stretch of audio, 0 to 255.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Peaks {
    pub low: u8,
    pub mid: u8,
    pub high: u8,
}

impl Peaks {
    fn merge(self, other: Self) -> Self {
        Self {
            low: self.low.max(other.low),
            mid: self.mid.max(other.mid),
            high: self.high.max(other.high),
        }
    }

    pub(super) fn loudest(self) -> u8 {
        self.low.max(self.mid).max(self.high)
    }

    /// How treble-heavy the sound is: 0 when it is all low band, 1 when it
    /// is all high band, with the mid band counting half. Silence is 0.
    fn brightness(self) -> f64 {
        let (low, mid, high) = (
            f64::from(self.low),
            f64::from(self.mid),
            f64::from(self.high),
        );
        let total = low + mid + high;
        if total == 0.0 {
            0.0
        } else {
            (high + 0.5 * mid) / total
        }
    }
}

/// Rescales `value` from 0..=255 to 0..=`max`, rounding to nearest.
fn scale(value: u8, max: u8) -> u8 {
    ((u32::from(value) * u32::from(max) + 127) / 255) as u8
}

/// Rescales a 0..=1 fraction to 0..=`max`, rounding to nearest.
fn scale_fraction(fraction: f64, max: u8) -> u8 {
    (fraction.clamp(0.0, 1.0) * f64::from(max)).round() as u8
}

/// Number of detail entries for a track: `ceil(duration × 150)`.
pub(super) fn detail_len(duration_secs: f64) -> usize {
    if duration_secs.is_finite() && duration_secs > 0.0 {
        (duration_secs * f64::from(DETAIL_ENTRIES_PER_SECOND)).ceil() as usize
    } else {
        0
    }
}

/// `len` entries at 150 per second. Each entry is the per-band maximum of
/// the source points overlapping its 1/150 s (the identity when the source
/// is already at 150 points per second); entries past the end of the source
/// are silent.
pub(super) fn detail_peaks(waveform: &ColourWaveform, len: usize) -> Vec<Peaks> {
    let rate = u64::from(waveform.points_per_second);
    let target = u64::from(DETAIL_ENTRIES_PER_SECOND);
    let source_len = waveform
        .low
        .len()
        .min(waveform.mid.len())
        .min(waveform.high.len());
    let point = |i: usize| Peaks {
        low: waveform.low[i],
        mid: waveform.mid[i],
        high: waveform.high[i],
    };
    (0..len as u64)
        .map(|entry| {
            if rate == 0 {
                return Peaks::default();
            }
            let start = (entry * rate / target) as usize;
            let end = (((entry + 1) * rate).div_ceil(target) as usize).max(start + 1);
            (start..end.min(source_len))
                .map(point)
                .fold(Peaks::default(), Peaks::merge)
        })
        .collect()
}

/// Squeezes (or stretches) `detail` into exactly `columns` columns, each the
/// per-band maximum of the detail entries it spans.
pub(super) fn columns(detail: &[Peaks], columns: usize) -> Vec<Peaks> {
    let len = detail.len();
    (0..columns)
        .map(|column| {
            if len == 0 {
                return Peaks::default();
            }
            let start = column * len / columns;
            let end = ((column + 1) * len / columns).max(start + 1).min(len);
            detail[start..end]
                .iter()
                .copied()
                .fold(Peaks::default(), Peaks::merge)
        })
        .collect()
}

/// `PWAV` and `PWV3` byte: height 0 to 31 in the low five bits, whiteness
/// 0 to 7 in the high three bits (higher is a whiter, less saturated blue).
pub(super) fn mono_entry(peaks: Peaks) -> u8 {
    let height = scale(peaks.loudest(), 31);
    let whiteness = scale_fraction(peaks.brightness(), 7);
    (whiteness << 5) | height
}

/// `PWV2` byte: height 0 to 15 in the low four bits, the rest unused.
pub(super) fn tiny_entry(peaks: Peaks) -> u8 {
    scale(peaks.loudest(), 15)
}

/// `PWV4` column, six one-byte channels. The documents describe byte 2 as
/// energy in the lower half of the spectrum (below about 10 kHz) and bytes
/// 3, 4 and 5 as low, mid and high energy; bytes 0 and 1 are undocumented
/// beyond "somehow encode whiteness", so we put our whiteness there.
/// Channels use 0 to 127; the range rekordbox intends is not documented.
pub(super) fn colour_preview_entry(peaks: Peaks) -> [u8; 6] {
    let whiteness = scale_fraction(peaks.brightness(), 127);
    [
        whiteness,
        whiteness,
        scale(peaks.low.max(peaks.mid), 127),
        scale(peaks.low, 127),
        scale(peaks.mid, 127),
        scale(peaks.high, 127),
    ]
}

/// `PWV5` entry: as a big-endian `u16`, red in bits 15 to 13, green in 12
/// to 10, blue in 9 to 7, height in 6 to 2, and bits 1 and 0 unused. Red,
/// green and blue follow the low, mid and high bands relative to the
/// loudest band, so balanced sound draws white.
pub(super) fn colour_detail_entry(peaks: Peaks) -> [u8; 2] {
    let loudest = peaks.loudest();
    let component = |band: u8| {
        if loudest == 0 {
            0
        } else {
            u16::from(scale_fraction(f64::from(band) / f64::from(loudest), 7))
        }
    };
    let height = u16::from(scale(loudest, 31));
    let packed = (component(peaks.low) << 13)
        | (component(peaks.mid) << 10)
        | (component(peaks.high) << 7)
        | (height << 2);
    packed.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_resamples_by_maxima() {
        let waveform = ColourWaveform {
            points_per_second: 300,
            low: vec![1, 9, 3, 4],
            mid: vec![0; 4],
            high: vec![0; 4],
        };
        let detail = detail_peaks(&waveform, 3);
        let lows: Vec<u8> = detail.iter().map(|p| p.low).collect();
        assert_eq!(lows, [9, 4, 0]);
    }

    #[test]
    fn columns_cover_every_entry_once() {
        let detail: Vec<Peaks> = (0..10)
            .map(|i| Peaks {
                low: i,
                ..Peaks::default()
            })
            .collect();
        let lows: Vec<u8> = columns(&detail, 4).iter().map(|p| p.low).collect();
        assert_eq!(lows, [1, 4, 6, 9]);
        // Fewer entries than columns repeats entries rather than leaving gaps.
        let lows: Vec<u8> = columns(&detail[..2], 4).iter().map(|p| p.low).collect();
        assert_eq!(lows, [0, 0, 1, 1]);
    }

    #[test]
    fn encodings_pack_documented_bits() {
        let bass = Peaks {
            low: 255,
            mid: 0,
            high: 0,
        };
        let treble = Peaks {
            low: 0,
            mid: 0,
            high: 255,
        };
        assert_eq!(mono_entry(bass), 31);
        assert_eq!(mono_entry(treble), (7 << 5) | 31);
        assert_eq!(tiny_entry(bass), 15);
        assert_eq!(
            colour_detail_entry(bass),
            ((7u16 << 13) | (31 << 2)).to_be_bytes()
        );
        assert_eq!(
            colour_detail_entry(treble),
            ((7u16 << 7) | (31 << 2)).to_be_bytes()
        );
        assert_eq!(colour_detail_entry(Peaks::default()), [0, 0]);
        assert_eq!(colour_preview_entry(bass), [0, 0, 127, 127, 0, 0]);
    }
}
