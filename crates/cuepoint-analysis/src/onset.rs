//! Onset detection functions for beat tracking (spec 04, section 2.2).
//!
//! Two views of the same audio:
//! - [`OnsetEnvelope`]: log-magnitude spectral flux, half-wave rectified
//!   (Bello et al., "A Tutorial on Onset Detection in Music Signals", 2005;
//!   log compression as in Böck and Widmer, 2013) at about 86 frames per
//!   second, with the band below 150 Hz weighted up so kicks dominate. Used
//!   to find the tempo, beat phase and bar phase.
//! - [`AttackEnvelope`]: the slope of a Hann-smoothed energy envelope at
//!   1 ms resolution. Its peak sits on the attack of a percussive hit with
//!   little bias because the smoothing kernel is symmetric, so it places
//!   individual beats far more precisely than an STFT frame can.

use crate::signal::{Mono, Stft};

/// STFT size and hop at about 11 kHz: 46 ms window, 11.6 ms hop.
const FLUX_SIZE: usize = 512;
const FLUX_HOP: usize = 128;
/// Compression constant for log(1 + gamma * magnitude).
const LOG_GAMMA: f32 = 1000.0;
/// Upper edge of the kick band.
const LOW_BAND_HZ: f64 = 150.0;
/// Width of the moving mean removed from the envelope.
const DETREND_SECONDS: f64 = 0.5;

/// Spectral flux, one value per STFT hop.
pub(crate) struct OnsetEnvelope {
    /// Frames per second.
    pub(crate) rate: f64,
    /// Combined full-band and low-band flux, detrended and rectified.
    pub(crate) combined: Vec<f32>,
    /// Flux below 150 Hz only, as computed (not detrended).
    pub(crate) low: Vec<f32>,
}

impl OnsetEnvelope {
    pub(crate) fn compute(mono: &Mono) -> Self {
        let mut stft = Stft::new(FLUX_SIZE, FLUX_HOP);
        let frames = stft.frames(mono.samples.len());
        let bins = FLUX_SIZE / 2 + 1;
        let low_bins = ((LOW_BAND_HZ * FLUX_SIZE as f64 / mono.rate).round() as usize).max(2);
        let mut previous = vec![0f32; bins];
        let mut all = Vec::with_capacity(frames);
        let mut low = Vec::with_capacity(frames);
        stft.for_each(&mono.samples, |_, magnitudes| {
            let (mut flux_all, mut flux_low) = (0f32, 0f32);
            // Skip the DC bin.
            for (k, (&m, prev)) in magnitudes
                .iter()
                .zip(previous.iter_mut())
                .enumerate()
                .skip(1)
            {
                let level = (1.0 + LOG_GAMMA * m).ln();
                let rise = (level - *prev).max(0.0);
                *prev = level;
                flux_all += rise;
                if k <= low_bins {
                    flux_low += rise;
                }
            }
            all.push(flux_all / (bins - 1) as f32);
            low.push(flux_low / low_bins as f32);
        });

        let rate = mono.rate / FLUX_HOP as f64;
        let raw: Vec<f32> = all.iter().zip(&low).map(|(a, l)| a + l).collect();
        let combined = detrend(&raw, (DETREND_SECONDS * rate).round() as usize);
        Self {
            rate,
            combined,
            low,
        }
    }

    /// Frame index (fractional) for a time in seconds.
    pub(crate) fn frame_at(&self, time: f64) -> f64 {
        time * self.rate
    }

    /// Linearly interpolated value of `values` at fractional frame `x`.
    pub(crate) fn sample(values: &[f32], x: f64) -> f32 {
        if x < 0.0 {
            return 0.0;
        }
        let i = x.floor() as usize;
        let frac = (x - i as f64) as f32;
        match (values.get(i), values.get(i + 1)) {
            (Some(&a), Some(&b)) => a + (b - a) * frac,
            (Some(&a), None) => a * (1.0 - frac),
            _ => 0.0,
        }
    }
}

/// Subtracts a centred moving mean of `width` frames and clips at zero, so
/// only peaks standing above their surroundings remain.
fn detrend(values: &[f32], width: usize) -> Vec<f32> {
    let half = width / 2;
    let mut prefix = Vec::with_capacity(values.len() + 1);
    prefix.push(0f64);
    for &v in values {
        prefix.push(prefix[prefix.len() - 1] + f64::from(v));
    }
    (0..values.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(values.len());
            let mean = (prefix[hi] - prefix[lo]) / (hi - lo) as f64;
            (values[i] - mean as f32).max(0.0)
        })
        .collect()
}

/// Smoothing kernel half width, giving a 16 ms Hann window.
const ATTACK_KERNEL_SECONDS: f64 = 0.008;
/// Envelope resolution.
const ATTACK_STEP_SECONDS: f64 = 0.001;

/// Slope of the short-term energy, used to place beats precisely.
pub(crate) struct AttackEnvelope {
    /// Seconds between points.
    step: f64,
    slope: Vec<f32>,
}

impl AttackEnvelope {
    pub(crate) fn compute(mono: &Mono) -> Self {
        let hop = ((ATTACK_STEP_SECONDS * mono.rate).round() as usize).max(1);
        let half = ((ATTACK_KERNEL_SECONDS * mono.rate).round() as usize).max(1);
        let kernel: Vec<f32> = (0..=2 * half)
            .map(|j| {
                let phase = std::f32::consts::PI * j as f32 / (2 * half) as f32;
                phase.sin().powi(2)
            })
            .collect();
        let squares: Vec<f32> = mono.samples.iter().map(|s| s * s).collect();
        let points = squares.len().div_ceil(hop);
        let energy: Vec<f32> = (0..points)
            .map(|m| {
                // Kernel centred on sample m * hop.
                let centre = m * hop;
                let first = centre.saturating_sub(half);
                let last = (centre + half + 1).min(squares.len());
                let offset = first + half - centre;
                squares[first..last]
                    .iter()
                    .zip(&kernel[offset..])
                    .map(|(x, w)| x * w)
                    .sum()
            })
            .collect();
        // Central difference: slope at point m spans m - 1 to m + 1.
        let slope = (0..points)
            .map(|m| {
                let next = energy.get(m + 1).copied().unwrap_or(0.0);
                let prev = m.checked_sub(1).map_or(0.0, |p| energy[p]);
                next - prev
            })
            .collect();
        Self {
            step: hop as f64 / mono.rate,
            slope,
        }
    }

    /// The steepest rise within `radius` seconds of `time`, as (time,
    /// strength), refined by parabolic interpolation. `None` if the
    /// energy does not rise anywhere in the window.
    pub(crate) fn attack_near(&self, time: f64, radius: f64) -> Option<(f64, f32)> {
        let lo = ((time - radius) / self.step).ceil().max(1.0) as usize;
        let hi =
            (((time + radius) / self.step).floor() as usize).min(self.slope.len().checked_sub(2)?);
        if lo > hi {
            return None;
        }
        let (best, &peak) = self.slope[lo..=hi]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))?;
        if peak <= 0.0 {
            return None;
        }
        let i = lo + best;
        let (a, b, c) = (self.slope[i - 1], peak, self.slope[i + 1]);
        let denom = a - 2.0 * b + c;
        let shift = if denom < 0.0 {
            f64::from(0.5 * (a - c) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        Some(((i as f64 + shift) * self.step, peak))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_lands_on_a_step() {
        let rate = 11_025.0;
        let onset = 0.5;
        let samples = (0..rate as usize)
            .map(|n| {
                let t = n as f64 / rate;
                if t >= onset {
                    (0.5 * (std::f64::consts::TAU * 80.0 * (t - onset)).sin()) as f32
                } else {
                    0.0
                }
            })
            .collect();
        let attacks = AttackEnvelope::compute(&Mono { rate, samples });
        let (time, _) = attacks.attack_near(0.49, 0.05).unwrap_or_default();
        assert!((time - onset).abs() < 0.002, "{time}");
    }
}
