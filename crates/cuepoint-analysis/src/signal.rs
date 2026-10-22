//! Mono mixdown, decimation to about 11 kHz, and a short-time Fourier
//! transform over the result.
//!
//! Beat and key analysis only need content below about 5 kHz, so working
//! at a quarter of 44.1 kHz cuts their cost by four.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

use crate::filter::butterworth8_lowpass;

/// Lowest rate the decimated signal may have.
const TARGET_RATE: u32 = 11_025;

/// Mono audio at a reduced rate.
pub(crate) struct Mono {
    pub(crate) rate: f64,
    pub(crate) samples: Vec<f32>,
}

impl Mono {
    pub(crate) fn duration(&self) -> f64 {
        self.samples.len() as f64 / self.rate
    }

    pub(crate) fn rms(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.samples.iter().map(|&s| f64::from(s).powi(2)).sum();
        (sum / self.samples.len() as f64).sqrt()
    }
}

/// Averages the channels of interleaved stereo audio and decimates by the
/// largest whole factor that keeps the rate at or above 11.025 kHz, behind
/// an 8th-order Butterworth anti-alias filter at 80 % of the new Nyquist.
pub(crate) fn downmix_decimate(samples: &[f32], sample_rate: u32) -> Mono {
    let factor = (sample_rate / TARGET_RATE).max(1) as usize;
    let rate = f64::from(sample_rate) / factor as f64;
    let frames = samples
        .chunks_exact(2)
        .map(|f| 0.5 * (f64::from(f[0]) + f64::from(f[1])));
    let samples = if factor == 1 {
        frames.map(|s| s as f32).collect()
    } else {
        let mut anti_alias = butterworth8_lowpass(f64::from(sample_rate), 0.4 * rate);
        frames
            .enumerate()
            .filter_map(|(i, s)| {
                let y = anti_alias.process(s);
                (i % factor == 0).then_some(y as f32)
            })
            .collect()
    };
    Mono { rate, samples }
}

/// A Hann-windowed real FFT of fixed size.
pub(crate) struct Stft {
    size: usize,
    hop: usize,
    window: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
}

impl Stft {
    pub(crate) fn new(size: usize, hop: usize) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window: Vec<f32> = (0..size)
            .map(|n| {
                let phase = std::f32::consts::TAU * n as f32 / size as f32;
                0.5 - 0.5 * phase.cos()
            })
            .collect();
        Self {
            size,
            hop,
            window,
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            magnitudes: vec![0.0; size / 2 + 1],
            fft,
        }
    }

    pub(crate) fn size(&self) -> usize {
        self.size
    }

    /// Number of frames `signal` yields: one per hop, the first centred on
    /// sample 0, so frame `n` is centred at `n * hop`.
    pub(crate) fn frames(&self, len: usize) -> usize {
        if len == 0 { 0 } else { len.div_ceil(self.hop) }
    }

    /// Calls `f(frame, magnitudes)` for every frame. Magnitudes are scaled
    /// so a full-scale sine centred in a bin reads about 1.0. Samples
    /// outside the signal count as silence.
    pub(crate) fn for_each(&mut self, signal: &[f32], mut f: impl FnMut(usize, &[f32])) {
        let half = self.size / 2;
        // Hann window sum is size / 2; a sine of amplitude A peaks at A * sum / 2.
        let scale = 4.0 / self.size as f32;
        for frame in 0..self.frames(signal.len()) {
            let centre = frame * self.hop;
            for (i, (slot, w)) in self.input.iter_mut().zip(&self.window).enumerate() {
                let index = (centre + i).checked_sub(half);
                let sample = index.and_then(|j| signal.get(j)).copied().unwrap_or(0.0);
                *slot = sample * w;
            }
            // Buffer lengths come from the plan, so this cannot fail.
            if self
                .fft
                .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
                .is_err()
            {
                return;
            }
            for (m, c) in self.magnitudes.iter_mut().zip(&self.spectrum) {
                *m = c.norm() * scale;
            }
            f(frame, &self.magnitudes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_magnitude_is_calibrated() {
        let rate = 11_025.0;
        let bin = 40;
        let freq = bin as f64 * rate / 1024.0;
        let signal: Vec<f32> = (0..8192)
            .map(|n| (0.5 * (std::f64::consts::TAU * freq * n as f64 / rate).sin()) as f32)
            .collect();
        let mut stft = Stft::new(1024, 256);
        let mut peak = 0.0f32;
        stft.for_each(&signal, |frame, mags| {
            if frame == 10 {
                peak = mags[bin];
            }
        });
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }

    #[test]
    fn decimates_44k_by_four() {
        let samples = vec![0.25f32; 2 * 44_100];
        let mono = downmix_decimate(&samples, 44_100);
        assert_eq!(mono.rate, 11_025.0);
        assert_eq!(mono.samples.len(), 11_025);
        // DC passes the anti-alias filter unchanged.
        assert!((mono.samples[11_000] - 0.25).abs() < 1e-4);
    }
}
