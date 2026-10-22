//! Second-order IIR filters using the RBJ "Audio EQ Cookbook" designs.

use std::f32::consts::PI;

/// Angular frequency for `freq`, kept below Nyquist. A corner at or above
/// half the sample rate (e.g. a 13 kHz shelf at 22.05 kHz) makes the
/// filter unstable, so it is pulled down to just under Nyquist.
fn omega(sample_rate: f32, freq: f32) -> f32 {
    let freq = freq.clamp(1.0, sample_rate * 0.45);
    2.0 * PI * freq / sample_rate
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl BiquadCoeffs {
    /// Passes the signal unchanged.
    pub const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn normalised(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Shelf boosting or cutting everything below `freq`.
    pub fn low_shelf(sample_rate: f32, freq: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = omega(sample_rate, freq);
        let (sin, cos) = w0.sin_cos();
        // Shelf slope S = 1.
        let alpha = sin / 2.0 * 2f32.sqrt();
        let root = 2.0 * a.sqrt() * alpha;
        Self::normalised(
            a * ((a + 1.0) - (a - 1.0) * cos + root),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - root),
            (a + 1.0) + (a - 1.0) * cos + root,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - root,
        )
    }

    /// Shelf boosting or cutting everything above `freq`.
    pub fn high_shelf(sample_rate: f32, freq: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = omega(sample_rate, freq);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * 2f32.sqrt();
        let root = 2.0 * a.sqrt() * alpha;
        Self::normalised(
            a * ((a + 1.0) + (a - 1.0) * cos + root),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - root),
            (a + 1.0) - (a - 1.0) * cos + root,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - root,
        )
    }

    /// Bell around `freq` with quality factor `q`.
    pub fn peaking(sample_rate: f32, freq: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = omega(sample_rate, freq);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalised(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    /// Magnitude response in dB at `freq`, for tests and plots.
    pub fn magnitude_db(&self, sample_rate: f32, freq: f32) -> f32 {
        let w = 2.0 * PI * freq / sample_rate;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = -(self.b1 * s1 + self.b2 * s2);
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = -(self.a1 * s1 + self.a2 * s2);
        let num = num_re * num_re + num_im * num_im;
        let den = den_re * den_re + den_im * den_im;
        10.0 * (num / den).log10()
    }
}

/// One filter channel (transposed direct form II).
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    coeffs: BiquadCoeffs,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new(BiquadCoeffs::IDENTITY)
    }
}

impl Biquad {
    pub fn new(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Swaps coefficients, keeping state so parameter moves don't click.
    pub fn set_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.coeffs = coeffs;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let c = &self.coeffs;
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn shelves_and_peaks_hit_their_gain() {
        let low = BiquadCoeffs::low_shelf(SR, 100.0, -12.0);
        assert!((low.magnitude_db(SR, 20.0) + 12.0).abs() < 0.5);
        assert!(low.magnitude_db(SR, 5_000.0).abs() < 0.2);

        let high = BiquadCoeffs::high_shelf(SR, 10_000.0, 6.0);
        assert!((high.magnitude_db(SR, 20_000.0) - 6.0).abs() < 0.6);
        assert!(high.magnitude_db(SR, 200.0).abs() < 0.2);

        let mid = BiquadCoeffs::peaking(SR, 1_000.0, 0.7, -20.0);
        assert!((mid.magnitude_db(SR, 1_000.0) + 20.0).abs() < 0.1);
    }

    #[test]
    fn zero_gain_is_transparent() {
        for c in [
            BiquadCoeffs::low_shelf(SR, 100.0, 0.0),
            BiquadCoeffs::high_shelf(SR, 10_000.0, 0.0),
            BiquadCoeffs::peaking(SR, 1_000.0, 0.7, 0.0),
        ] {
            let mut f = Biquad::new(c);
            for i in 0..1000 {
                let x = (i as f32 * 0.37).sin();
                assert!((f.process(x) - x).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn stays_stable_when_corner_exceeds_nyquist() {
        for rate in [16_000.0, 22_050.0] {
            for coeffs in [
                BiquadCoeffs::high_shelf(rate, 13_000.0, -6.0),
                BiquadCoeffs::high_shelf(rate, 13_000.0, 6.0),
                BiquadCoeffs::peaking(rate, 13_000.0, 0.7, -26.0),
            ] {
                let mut filter = Biquad::new(coeffs);
                let mut peak = filter.process(1.0).abs();
                for _ in 0..100_000 {
                    peak = peak.max(filter.process(0.0).abs());
                }
                assert!(
                    peak.is_finite() && peak < 4.0,
                    "unstable at {rate} Hz: {peak}"
                );
            }
        }
    }
}
