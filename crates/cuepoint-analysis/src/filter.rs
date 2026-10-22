//! Second-order IIR sections designed by bilinear transform.
//!
//! Low and high passes follow the RBJ "Audio EQ Cookbook" formulas. State
//! and coefficients are `f64` so long cascades stay accurate when fed
//! `f32` audio.

use std::f64::consts::{FRAC_1_SQRT_2, PI};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s1: f64,
    s2: f64,
}

impl Biquad {
    /// From `b` (numerator) and `a` (denominator) coefficients; `a[0]` is
    /// normalised away.
    pub(crate) fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
            s1: 0.0,
            s2: 0.0,
        }
    }

    pub(crate) fn lowpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (cos, alpha) = cookbook_terms(sample_rate, freq, q);
        Self::new(
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    pub(crate) fn highpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let (cos, alpha) = cookbook_terms(sample_rate, freq, q);
        Self::new(
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    /// Normalised coefficients `[b0, b1, b2, a1, a2]`.
    #[cfg(test)]
    pub(crate) fn coefficients(&self) -> [f64; 5] {
        [self.b0, self.b1, self.b2, self.a1, self.a2]
    }

    /// Transposed direct form II.
    #[inline]
    pub(crate) fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Magnitude response at `freq`.
    #[cfg(test)]
    pub(crate) fn gain(&self, sample_rate: f64, freq: f64) -> f64 {
        // H(z) evaluated on the unit circle, z^-1 = e^(-jw).
        let w = 2.0 * PI * freq / sample_rate;
        let (s1, c1) = (-w).sin_cos();
        let (s2, c2) = (-2.0 * w).sin_cos();
        let num = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            self.b1 * s1 + self.b2 * s2,
        );
        let den = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            self.a1 * s1 + self.a2 * s2,
        );
        (num.0.hypot(num.1)) / (den.0.hypot(den.1))
    }
}

fn cookbook_terms(sample_rate: f64, freq: f64, q: f64) -> (f64, f64) {
    let freq = freq.clamp(1.0, sample_rate * 0.49);
    let (sin, cos) = (2.0 * PI * freq / sample_rate).sin_cos();
    (cos, sin / (2.0 * q))
}

/// A cascade of `N` sections run in series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cascade<const N: usize>(pub(crate) [Biquad; N]);

impl<const N: usize> Cascade<N> {
    #[inline]
    pub(crate) fn process(&mut self, x: f64) -> f64 {
        self.0
            .iter_mut()
            .fold(x, |acc, section| section.process(acc))
    }

    #[cfg(test)]
    pub(crate) fn gain(&self, sample_rate: f64, freq: f64) -> f64 {
        self.0.iter().map(|s| s.gain(sample_rate, freq)).product()
    }
}

/// 4th-order Linkwitz-Riley low pass: two identical Butterworth sections,
/// -6 dB at `freq`, summing flat with the matching high pass.
pub(crate) fn linkwitz_riley_lowpass(sample_rate: f64, freq: f64) -> Cascade<2> {
    let section = Biquad::lowpass(sample_rate, freq, FRAC_1_SQRT_2);
    Cascade([section; 2])
}

pub(crate) fn linkwitz_riley_highpass(sample_rate: f64, freq: f64) -> Cascade<2> {
    let section = Biquad::highpass(sample_rate, freq, FRAC_1_SQRT_2);
    Cascade([section; 2])
}

/// 8th-order Butterworth low pass, used as the anti-alias filter before
/// decimation. Section Q values are 1 / (2 sin((2k - 1) pi / 16)).
pub(crate) fn butterworth8_lowpass(sample_rate: f64, freq: f64) -> Cascade<4> {
    Cascade(std::array::from_fn(|k| {
        let q = 1.0 / (2.0 * ((2 * k + 1) as f64 * PI / 16.0).sin());
        Biquad::lowpass(sample_rate, freq, q)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(gain: f64) -> f64 {
        20.0 * gain.log10()
    }

    #[test]
    fn linkwitz_riley_is_minus_six_db_at_crossover() {
        let lp = linkwitz_riley_lowpass(44_100.0, 200.0);
        let hp = linkwitz_riley_highpass(44_100.0, 200.0);
        assert!((db(lp.gain(44_100.0, 200.0)) + 6.02).abs() < 0.05);
        assert!((db(hp.gain(44_100.0, 200.0)) + 6.02).abs() < 0.05);
        assert!(db(lp.gain(44_100.0, 2000.0)) < -75.0);
        assert!(db(hp.gain(44_100.0, 20.0)) < -75.0);
    }

    #[test]
    fn butterworth_passband_is_flat() {
        let lp = butterworth8_lowpass(44_100.0, 4410.0);
        assert!(db(lp.gain(44_100.0, 1000.0)).abs() < 0.01);
        assert!((db(lp.gain(44_100.0, 4410.0)) + 3.01).abs() < 0.05);
        assert!(db(lp.gain(44_100.0, 8820.0)) < -45.0);
    }

    #[test]
    fn processing_matches_response() {
        // Steady-state amplitude of a filtered sine equals the gain.
        let mut lp = Biquad::lowpass(48_000.0, 500.0, FRAC_1_SQRT_2);
        let freq = 1000.0;
        let mut peak: f64 = 0.0;
        for n in 0..48_000 {
            let y = lp.process((2.0 * PI * freq * n as f64 / 48_000.0).sin());
            if n > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        let expected = Biquad::lowpass(48_000.0, 500.0, FRAC_1_SQRT_2).gain(48_000.0, freq);
        assert!((peak - expected).abs() < 1e-3, "{peak} vs {expected}");
    }
}
