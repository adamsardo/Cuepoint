//! Tempo, beat phase and bar phase for a constant-tempo grid (spec 04,
//! section 2.2, the "Normal" mode DSP path).
//!
//! 1. Coarse tempo: autocorrelation of the onset envelope, weighted by a
//!    log-normal tempo prior (Ellis, "Beat Tracking by Dynamic
//!    Programming", 2007), then refined on the peak four beats out.
//! 2. Beat phase: the offset whose comb of beats collects the most onset
//!    energy, found in the busiest 16 s of the track.
//! 3. Fit: each predicted beat is snapped to the steepest energy rise near
//!    it and a robust least-squares line (beat number against time) gives
//!    period and phase. The window grows from 8 s to the whole track so the
//!    prediction never drifts far from the audio.
//! 4. The tempo is folded into the BPM range by doubling or halving
//!    (rekordbox `bpmToDouble` / `bpmToHalf`) and rounded.
//! 5. Bar phase: of the four ways to group beats into bars, the one whose
//!    first beats carry the most low-band onset energy.

use cuepoint_core::BEATS_PER_BAR;

use crate::AnalysisSettings;
use crate::onset::{AttackEnvelope, OnsetEnvelope};

/// Widest tempo range searched before folding.
const DETECT_BPM_FLOOR: f64 = 30.0;
const DETECT_BPM_CEILING: f64 = 320.0;
/// Width of the tempo prior in octaves.
const PRIOR_OCTAVES: f64 = 1.0;
/// Seconds of busiest audio used to find the first phase.
const ANCHOR_SECONDS: f64 = 16.0;
/// Initial half width of the fitting window.
const FIRST_FIT_HALF_SECONDS: f64 = 4.0;
/// Beats are searched for within this fraction of a period of prediction.
const SEARCH_FRACTION: f64 = 0.2;
/// Residuals under this are never treated as outliers.
const MIN_OUTLIER_SECONDS: f64 = 0.004;
/// Assumption: rekordbox's Normal mode snaps grids of most electronic music
/// to whole BPM; we snap when the fitted tempo is this close to an integer.
const INTEGER_SNAP_BPM: f64 = 0.03;

/// A constant-tempo beat grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Beats {
    pub(crate) bpm: f64,
    /// Earliest beat at or after time 0.
    pub(crate) first_beat: f64,
    /// Position of `first_beat` within its bar, 1..=4.
    pub(crate) beat_in_bar: u32,
}

/// A beat found in the audio: its number on the grid, time and strength.
#[derive(Debug, Clone, Copy)]
struct Hit {
    beat: i64,
    time: f64,
    strength: f32,
}

pub(crate) fn track(
    onsets: &OnsetEnvelope,
    attacks: &AttackEnvelope,
    duration: f64,
    settings: &AnalysisSettings,
) -> Option<Beats> {
    let coarse = coarse_period(&onsets.combined, onsets.rate, settings)?;
    let anchor = busiest_time(&onsets.combined, onsets.rate, duration);
    let origin = beat_phase(onsets, coarse, anchor);
    let (mut origin, period, mut hits) =
        fit_progressively(attacks, origin, coarse / onsets.rate, anchor, duration)?;

    // Fold into the range. Halving keeps every other beat (or every fourth,
    // and so on), so keep the set of beats that hit hardest.
    let (mut period, mut stride) = (period, 1i64);
    for _ in 0..8 {
        if 60.0 / period < settings.bpm_max {
            break;
        }
        period *= 2.0;
        stride *= 2;
    }
    for _ in 0..8 {
        if 60.0 / period >= settings.bpm_min || 120.0 / period >= settings.bpm_max {
            break;
        }
        period /= 2.0;
    }
    if stride > 1 {
        let strength = |j: i64| -> f32 {
            hits.iter()
                .filter(|h| (h.beat - j).rem_euclid(stride) == 0)
                .map(|h| h.strength)
                .sum()
        };
        let best = (0..stride)
            .max_by(|&a, &b| strength(a).total_cmp(&strength(b)))
            .unwrap_or(0);
        hits.retain(|h| (h.beat - best).rem_euclid(stride) == 0);
        origin += best as f64 * period / stride as f64;
    }

    let bpm = round_bpm(60.0 / period);
    let period = 60.0 / bpm;
    let origin = refit_phase(&hits, origin, period);
    let first_beat = origin.rem_euclid(period);
    let beat_in_bar = bar_phase(onsets, first_beat, period, duration);
    Some(Beats {
        bpm,
        first_beat,
        beat_in_bar,
    })
}

/// Two decimals, or the whole number when within `INTEGER_SNAP_BPM`.
fn round_bpm(bpm: f64) -> f64 {
    let whole = bpm.round();
    if (bpm - whole).abs() < INTEGER_SNAP_BPM {
        whole
    } else {
        (bpm * 100.0).round() / 100.0
    }
}

fn autocorrelation(env: &[f32], lag: usize) -> f64 {
    if lag >= env.len() {
        return 0.0;
    }
    let n = env.len() - lag;
    let sum: f64 = env[..n]
        .iter()
        .zip(&env[lag..])
        .map(|(a, b)| f64::from(a * b))
        .sum();
    sum / n as f64
}

/// Vertex offset of a parabola through three equally spaced points.
fn parabolic_offset(a: f64, b: f64, c: f64) -> f64 {
    let denom = a - 2.0 * b + c;
    if denom < 0.0 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    }
}

/// Beat period in envelope frames.
fn coarse_period(env: &[f32], rate: f64, settings: &AnalysisSettings) -> Option<f64> {
    let slowest = (settings.bpm_min / 2.0).max(DETECT_BPM_FLOOR);
    let fastest = (settings.bpm_max * 2.0).min(DETECT_BPM_CEILING);
    let min_lag = ((60.0 * rate / fastest).floor() as usize).max(2);
    let max_lag = (60.0 * rate / slowest).ceil() as usize;
    if max_lag + 2 >= env.len() {
        return None;
    }
    let acf: Vec<f64> = (0..=max_lag + 1)
        .map(|lag| autocorrelation(env, lag))
        .collect();
    // Prior centred on the middle of the range: it only arbitrates between
    // metrical levels that are not a power of two apart (3:2, 3:1), since
    // folding takes care of octaves.
    let centre = (settings.bpm_min * settings.bpm_max).sqrt();
    let prior = |lag: usize| {
        let octaves = (60.0 * rate / lag as f64 / centre).log2() / PRIOR_OCTAVES;
        (-0.5 * octaves * octaves).exp()
    };
    let lag = (min_lag..=max_lag)
        .filter(|&l| acf[l] > 0.0 && acf[l] >= acf[l - 1] && acf[l] >= acf[l + 1])
        .max_by(|&a, &b| (acf[a] * prior(a)).total_cmp(&(acf[b] * prior(b))))?;
    if acf[lag] <= acf[0] * 1e-3 {
        return None;
    }
    let mut period = lag as f64 + parabolic_offset(acf[lag - 1], acf[lag], acf[lag + 1]);

    // The peak four beats out is four times as precise.
    let span = 4.0 * period;
    let centre_lag = span.round() as usize;
    if centre_lag + 3 < env.len() / 2 {
        let around: Vec<(usize, f64)> = (centre_lag - 2..=centre_lag + 2)
            .map(|l| (l, autocorrelation(env, l)))
            .collect();
        if let Some(&(l, peak)) = around[1..4].iter().max_by(|a, b| a.1.total_cmp(&b.1))
            && peak > 0.5 * acf[lag]
        {
            let refined = l as f64
                + parabolic_offset(
                    autocorrelation(env, l - 1),
                    peak,
                    autocorrelation(env, l + 1),
                );
            if (refined / 4.0 - period).abs() < 1.0 {
                period = refined / 4.0;
            }
        }
    }
    Some(period)
}

/// Centre (seconds) of the `ANCHOR_SECONDS` window with the most onset
/// energy, so the first fit happens where the beat is clearest.
fn busiest_time(env: &[f32], rate: f64, duration: f64) -> f64 {
    let width = (ANCHOR_SECONDS * rate) as usize;
    if env.len() <= width {
        return duration / 2.0;
    }
    let mut sum: f64 = env[..width].iter().map(|&v| f64::from(v)).sum();
    let (mut best, mut best_start) = (sum, 0);
    for start in 1..=env.len() - width {
        sum += f64::from(env[start + width - 1]) - f64::from(env[start - 1]);
        if sum > best {
            (best, best_start) = (sum, start);
        }
    }
    (best_start as f64 + width as f64 / 2.0) / rate
}

/// Time of a beat near `anchor`, choosing the phase of a comb with period
/// `period` frames that collects the most onset energy within 6 s.
fn beat_phase(onsets: &OnsetEnvelope, period: f64, anchor: f64) -> f64 {
    let start = onsets.frame_at(anchor - 6.0).max(0.0);
    let end = onsets
        .frame_at(anchor + 6.0)
        .min(onsets.combined.len() as f64);
    let steps = (period * 4.0).ceil() as usize;
    let score = |phase: f64| -> f32 {
        let mut x = start + phase;
        let mut sum = 0.0;
        while x < end {
            sum += OnsetEnvelope::sample(&onsets.combined, x);
            x += period;
        }
        sum
    };
    let best = (0..steps)
        .map(|s| s as f64 / 4.0)
        .max_by(|&a, &b| score(a).total_cmp(&score(b)))
        .unwrap_or(0.0);
    (start + best) / onsets.rate
}

/// Snaps beats to attacks over a window that doubles each round, refitting
/// as it goes. Returns the time of beat 0, the period in seconds and the
/// beats that were found.
fn fit_progressively(
    attacks: &AttackEnvelope,
    mut origin: f64,
    mut period: f64,
    anchor: f64,
    duration: f64,
) -> Option<(f64, f64, Vec<Hit>)> {
    let coarse = period;
    let mut half = FIRST_FIT_HALF_SECONDS;
    let mut hits = Vec::new();
    loop {
        let lo = (anchor - half).max(0.0);
        let hi = (anchor + half).min(duration);
        let first = ((lo - origin) / period).ceil() as i64;
        let last = ((hi - origin) / period).floor() as i64;
        let found: Vec<Hit> = (first..=last)
            .filter_map(|beat| {
                let predicted = origin + beat as f64 * period;
                let (time, strength) = attacks.attack_near(predicted, SEARCH_FRACTION * period)?;
                Some(Hit {
                    beat,
                    time,
                    strength,
                })
            })
            .collect();
        // A fit that wanders far from the coarse tempo has locked onto
        // something else; keep the previous one.
        if let Some((o, p, kept)) = robust_line_fit(found)
            && (p / coarse - 1.0).abs() < 0.05
        {
            (origin, period, hits) = (o, p, kept);
        }
        if lo <= 0.0 && hi >= duration {
            break;
        }
        half *= 2.0;
    }
    (!hits.is_empty()).then_some((origin, period, hits))
}

/// Least-squares line through (beat, time), rejecting weak hits and then
/// outliers by median absolute deviation. Returns (intercept, slope, inliers).
fn robust_line_fit(mut hits: Vec<Hit>) -> Option<(f64, f64, Vec<Hit>)> {
    let mut strengths: Vec<f32> = hits.iter().map(|h| h.strength).collect();
    strengths.sort_by(f32::total_cmp);
    let median_strength = *strengths.get(strengths.len() / 2)?;
    hits.retain(|h| h.strength >= 0.2 * median_strength);

    let mut line = None;
    for _ in 0..4 {
        let (origin, period) = least_squares(&hits)?;
        line = Some((origin, period));
        let residual = |h: &Hit| (h.time - origin - h.beat as f64 * period).abs();
        let mut residuals: Vec<f64> = hits.iter().map(residual).collect();
        residuals.sort_by(f64::total_cmp);
        let mad = residuals[residuals.len() / 2];
        let limit = (4.0 * 1.4826 * mad).max(MIN_OUTLIER_SECONDS);
        let before = hits.len();
        hits.retain(|h| residual(h) <= limit);
        if hits.len() == before {
            break;
        }
    }
    let (origin, period) = least_squares(&hits).or(line)?;
    Some((origin, period, hits))
}

fn least_squares(hits: &[Hit]) -> Option<(f64, f64)> {
    if hits.len() < 4 {
        return None;
    }
    let n = hits.len() as f64;
    let mean_k = hits.iter().map(|h| h.beat as f64).sum::<f64>() / n;
    let mean_t = hits.iter().map(|h| h.time).sum::<f64>() / n;
    let (mut cov, mut var) = (0.0, 0.0);
    for h in hits {
        let dk = h.beat as f64 - mean_k;
        cov += dk * (h.time - mean_t);
        var += dk * dk;
    }
    if var <= 0.0 {
        return None;
    }
    let slope = cov / var;
    (slope > 0.0).then_some((mean_t - slope * mean_k, slope))
}

/// Best time for some beat given a fixed `period`: the mean offset of the
/// hits from the grid, ignoring any more than 10 ms from the median.
fn refit_phase(hits: &[Hit], origin: f64, period: f64) -> f64 {
    let mut offsets: Vec<f64> = hits
        .iter()
        .map(|h| {
            let beat = ((h.time - origin) / period).round();
            h.time - beat * period - origin
        })
        .collect();
    offsets.sort_by(f64::total_cmp);
    let Some(&median) = offsets.get(offsets.len() / 2) else {
        return origin;
    };
    let near: Vec<f64> = offsets
        .into_iter()
        .filter(|o| (o - median).abs() < 0.010)
        .collect();
    origin + near.iter().sum::<f64>() / near.len() as f64
}

/// Position of the first beat in its bar, 1..=4.
fn bar_phase(onsets: &OnsetEnvelope, first_beat: f64, period: f64, duration: f64) -> u32 {
    let bar = BEATS_PER_BAR as usize;
    let mut sums = [0f64; BEATS_PER_BAR as usize];
    let mut counts = [0usize; BEATS_PER_BAR as usize];
    let mut beat = 0;
    loop {
        let time = first_beat + beat as f64 * period;
        if time >= duration {
            break;
        }
        // The flux frame peaks up to a frame before the attack.
        let centre = onsets.frame_at(time).round() as i64;
        let strength = (centre - 2..=centre + 1)
            .filter_map(|n| usize::try_from(n).ok())
            .filter_map(|n| Some(onsets.low.get(n)? + 0.25 * onsets.combined.get(n)?))
            .fold(0f32, f32::max);
        sums[beat % bar] += f64::from(strength);
        counts[beat % bar] += 1;
        beat += 1;
    }
    let mean = |p: usize| sums[p] / counts[p].max(1) as f64;
    let downbeat = (0..bar)
        .max_by(|&a, &b| mean(a).total_cmp(&mean(b)))
        .unwrap_or(0);
    // Beat `downbeat` starts a bar, so beat 0 sits `bar - downbeat` beats
    // into the previous one (or starts a bar itself).
    ((bar - downbeat) % bar) as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_like_rekordbox() {
        assert_eq!(round_bpm(127.98), 128.0);
        assert_eq!(round_bpm(128.024), 128.0);
        assert_eq!(round_bpm(123.4512), 123.45);
        assert_eq!(round_bpm(128.05), 128.05);
    }

    #[test]
    fn line_fit_rejects_outliers() {
        let mut hits: Vec<Hit> = (0..20)
            .map(|k| Hit {
                beat: k,
                time: 0.1 + k as f64 * 0.5,
                strength: 1.0,
            })
            .collect();
        hits[7].time += 0.05;
        let (origin, period, kept) = robust_line_fit(hits).unwrap_or_default();
        assert!((origin - 0.1).abs() < 1e-9 && (period - 0.5).abs() < 1e-9);
        assert_eq!(kept.len(), 19);
    }
}
