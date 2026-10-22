//! Integrated loudness per ITU-R BS.1770-4 / EBU R128 (spec 04, section 2.7).
//!
//! K-weighting is a high shelf (head effects) followed by the "RLB" high
//! pass. BS.1770 only tabulates coefficients for 48 kHz, so both stages are
//! designed here from their analogue prototypes by bilinear transform with
//! pre-warping, using the prototype parameters fitted to the 48 kHz table
//! (published with libebur128). That gives matching filters at any rate.
//!
//! Gating: mean square over 400 ms blocks overlapping by 75 %, an absolute
//! gate at -70 LUFS, then a relative gate 10 LU below the loudness of the
//! blocks that passed the absolute gate.

use std::f64::consts::PI;

use crate::filter::{Biquad, Cascade};

const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;
/// Blocks are 400 ms, stepped in 100 ms segments.
const SEGMENTS_PER_BLOCK: usize = 4;

/// The two K-weighting stages for `sample_rate`.
pub(crate) fn k_weighting(sample_rate: f64) -> Cascade<2> {
    // Stage 1: high shelf, about +4 dB above 2 kHz.
    let f0 = 1_681.974_450_955_533;
    let gain_db = 3.999_843_853_973_347;
    let q = 0.707_175_236_955_419_6;
    let k = (PI * f0 / sample_rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let shelf = Biquad::new(
        [
            vh + vb * k / q + k * k,
            2.0 * (k * k - vh),
            vh - vb * k / q + k * k,
        ],
        [
            1.0 + k / q + k * k,
            2.0 * (k * k - 1.0),
            1.0 - k / q + k * k,
        ],
    );

    // Stage 2: second-order high pass at about 38 Hz.
    let f0 = 38.135_470_876_024_44;
    let q = 0.500_327_037_323_877_3;
    let k = (PI * f0 / sample_rate).tan();
    let a0 = 1.0 + k / q + k * k;
    // BS.1770 leaves the numerator as [1, -2, 1] rather than scaling it by
    // 1 / a0, and its -0.691 dB offset assumes that, so we do the same.
    let highpass = Biquad::new(
        [a0, -2.0 * a0, a0],
        [a0, 2.0 * (k * k - 1.0), 1.0 - k / q + k * k],
    );
    Cascade([shelf, highpass])
}

fn to_lufs(mean_square: f64) -> f64 {
    -0.691 + 10.0 * mean_square.log10()
}

/// Integrated loudness of interleaved stereo audio, or `None` when every
/// block is gated out (silence, or shorter than 400 ms).
pub(crate) fn integrated_loudness(samples: &[f32], sample_rate: u32) -> Option<f32> {
    let rate = f64::from(sample_rate);
    let segment_len = (rate / 10.0).round() as usize;
    if segment_len == 0 {
        return None;
    }

    // Sum over both channels (weights 1.0 for left and right) of the
    // K-weighted squares, per 100 ms segment.
    let mut filters = [k_weighting(rate), k_weighting(rate)];
    let segments: Vec<f64> = samples
        .chunks_exact(2 * segment_len)
        .map(|segment| {
            segment
                .chunks_exact(2)
                .map(|frame| {
                    let l = filters[0].process(f64::from(frame[0]));
                    let r = filters[1].process(f64::from(frame[1]));
                    l * l + r * r
                })
                .sum()
        })
        .collect();

    let block_len = (SEGMENTS_PER_BLOCK * segment_len) as f64;
    let blocks: Vec<f64> = segments
        .windows(SEGMENTS_PER_BLOCK)
        .map(|w| w.iter().sum::<f64>() / block_len)
        .filter(|&z| z > 0.0 && to_lufs(z) > ABSOLUTE_GATE_LUFS)
        .collect();
    if blocks.is_empty() {
        return None;
    }

    let mean = |zs: &mut dyn Iterator<Item = f64>| {
        let (sum, count) = zs.fold((0.0, 0usize), |(s, c), z| (s + z, c + 1));
        (count > 0).then(|| sum / count as f64)
    };
    let relative_gate = to_lufs(mean(&mut blocks.iter().copied())?) + RELATIVE_GATE_LU;
    let gated = mean(
        &mut blocks
            .iter()
            .copied()
            .filter(|&z| to_lufs(z) > relative_gate),
    )?;
    Some(to_lufs(gated) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficients_match_bs1770_table_at_48k() {
        let [shelf, highpass] = k_weighting(48_000.0).0;
        let expected_shelf = [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
            -1.690_659_293_182_41,
            0.732_480_774_215_85,
        ];
        let expected_hp = [1.0, -2.0, 1.0, -1.990_047_454_833_98, 0.990_072_250_366_21];
        for (got, want) in shelf.coefficients().iter().zip(expected_shelf) {
            assert!((got - want).abs() < 1e-6, "shelf {got} vs {want}");
        }
        for (got, want) in highpass.coefficients().iter().zip(expected_hp) {
            assert!((got - want).abs() < 1e-6, "high pass {got} vs {want}");
        }
    }
}
