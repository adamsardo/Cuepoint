//! End-to-end tests on synthetic signals built here, so no audio files are
//! needed.

use std::f64::consts::TAU;

use super::*;
use crate::filter::Cascade;

const RATE: u32 = 44_100;

/// Deterministic white noise in -1..1 (a 32-bit LCG).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(self.0 >> 8) / f64::from(1u32 << 23) - 1.0
    }
}

/// Adds `voice(t)` (t in seconds from its start) to both channels for
/// `length` seconds from `start`.
fn add(out: &mut [f32], start: f64, length: f64, mut voice: impl FnMut(f64) -> f64) {
    let rate = f64::from(RATE);
    let first = (start * rate).round() as usize;
    let frames = out.len() / 2;
    for n in first..(first + (length * rate) as usize).min(frames) {
        let v = voice((n - first) as f64 / rate) as f32;
        out[2 * n] += v;
        out[2 * n + 1] += v;
    }
}

/// A synthetic kick: a sine sweeping down from 150 Hz to `base` Hz with a
/// short noise click on the attack.
fn kick(out: &mut [f32], at: f64, gain: f64, base: f64, noise: &mut Noise) {
    let sweep = 0.03;
    add(out, at, 0.3, |t| {
        let phase = TAU * (base * t + (150.0 - base) * sweep * (1.0 - (-t / sweep).exp()));
        let body = phase.sin() * (-t / 0.12).exp();
        let click = if t < 0.003 { 0.3 * noise.next() } else { 0.0 };
        gain * 0.8 * (body + click)
    });
}

/// A quiet high-passed noise burst.
fn hat(out: &mut [f32], at: f64, noise: &mut Noise) {
    let mut last = 0.0;
    add(out, at, 0.04, |t| {
        let n = noise.next();
        let hp = n - last;
        last = n;
        0.08 * hp * (-t / 0.01).exp()
    });
}

struct KickTrack {
    bpm: f64,
    first_beat: f64,
    seconds: f64,
    /// Kicks per bar; the first of each is louder and lower.
    bar: usize,
    hats: bool,
}

impl KickTrack {
    fn new(bpm: f64) -> Self {
        Self {
            bpm,
            first_beat: 0.137,
            seconds: 30.0,
            bar: 4,
            hats: false,
        }
    }

    fn kick_times(&self) -> impl Iterator<Item = (usize, f64)> + '_ {
        let period = 60.0 / self.bpm;
        (0..)
            .map(move |k| (k, self.first_beat + k as f64 * period))
            .take_while(|&(_, t)| t < self.seconds - 0.3)
    }

    fn render(&self) -> Vec<f32> {
        let mut out = vec![0f32; 2 * (self.seconds * f64::from(RATE)) as usize];
        let mut noise = Noise(12345);
        let period = 60.0 / self.bpm;
        for (k, t) in self.kick_times() {
            if k % self.bar == 0 {
                kick(&mut out, t, 1.0, 45.0, &mut noise);
            } else {
                kick(&mut out, t, 0.6, 60.0, &mut noise);
            }
            if self.hats {
                hat(&mut out, t + period / 2.0, &mut noise);
            }
        }
        out
    }
}

/// Analyses a kick track and checks the grid lines up with its kicks: the
/// first beat within 5 ms, and every loud kick on a downbeat. With
/// `quiet_off_downbeat`, quiet kicks must not be downbeats.
fn check_grid(track: &KickTrack, expected_bpm: f64, quiet_off_downbeat: bool) -> Analysis {
    let analysis = analyze(&track.render(), RATE, &AnalysisSettings::default());
    assert_eq!(
        analysis.bpm, expected_bpm,
        "bpm for {} BPM kicks",
        track.bpm
    );
    let marker = analysis.grid.markers()[0];
    assert_eq!(marker.bpm, expected_bpm);
    let error = marker.time - track.first_beat;
    println!(
        "{} BPM: first beat error {:.2} ms",
        track.bpm,
        error * 1000.0
    );
    assert!(error.abs() < 0.005, "first beat at {}", marker.time);
    for (k, t) in track.kick_times() {
        let beat = analysis.grid.beat_at(t).unwrap_or(f64::NAN);
        assert!(
            (beat - beat.round()).abs() < 0.02,
            "kick {k} off grid by {beat}"
        );
        let downbeat = analysis.grid.is_downbeat(beat.round() as i64);
        if k % track.bar == 0 {
            assert!(downbeat, "loud kick {k} is not a downbeat");
        } else if quiet_off_downbeat {
            assert!(!downbeat, "quiet kick {k} is a downbeat");
        }
    }
    analysis
}

#[test]
fn kicks_at_128_with_hats() {
    let mut track = KickTrack::new(128.0);
    track.hats = true;
    check_grid(&track, 128.0, true);
}

#[test]
fn kicks_at_174() {
    check_grid(&KickTrack::new(174.0), 174.0, true);
}

#[test]
fn kicks_at_90() {
    check_grid(&KickTrack::new(90.0), 90.0, true);
}

/// Folding rule: tempos below `bpm_min` double, tempos at or above
/// `bpm_max` halve. With the default 88..175 range, 87 BPM reports 174 (the
/// grid gains a beat between kicks, so only loud kicks are checked) and
/// 180 BPM reports 90.
#[test]
fn tempo_folds_into_range() {
    check_grid(&KickTrack::new(87.0), 174.0, false);
    let mut fast = KickTrack::new(180.0);
    // Loud kicks every 8 at 180 make bars of 4 at 90.
    fast.bar = 8;
    let analysis = analyze(&fast.render(), RATE, &AnalysisSettings::default());
    assert_eq!(analysis.bpm, 90.0);
    let marker = analysis.grid.markers()[0];
    assert!(
        (marker.time - fast.first_beat).abs() < 0.005,
        "{}",
        marker.time
    );
    assert_eq!(marker.beat_in_bar, 1);
}

#[test]
fn non_integer_tempo() {
    let mut track = KickTrack::new(123.45);
    track.seconds = 60.0;
    let analysis = analyze(&track.render(), RATE, &AnalysisSettings::default());
    println!("123.45 BPM measured as {}", analysis.bpm);
    assert!((analysis.bpm - 123.45).abs() <= 0.02, "{}", analysis.bpm);
    let first = analysis.grid.markers()[0].time;
    assert!((first - track.first_beat).abs() < 0.005, "{first}");
}

fn midi_hz(note: u8) -> f64 {
    440.0 * 2f64.powf((f64::from(note) - 69.0) / 12.0)
}

/// A sustained chord plus a melody cycling through `scale`, 0.25 s a note.
fn tones(chord: &[u8], scale: &[u8], seconds: f64) -> Vec<f32> {
    let mut out = vec![0f32; 2 * (seconds * f64::from(RATE)) as usize];
    for &note in chord {
        let hz = midi_hz(note);
        add(&mut out, 0.0, seconds, |t| 0.15 * (TAU * hz * t).sin());
    }
    let mut at = 0.0;
    for &note in scale.iter().cycle() {
        if at >= seconds || scale.is_empty() {
            break;
        }
        let hz = midi_hz(note);
        add(&mut out, at, 0.25, |t| 0.1 * (TAU * hz * t).sin());
        at += 0.25;
    }
    out
}

fn camelot(samples: &[f32]) -> Option<String> {
    analyze(samples, RATE, &AnalysisSettings::default())
        .key
        .map(MusicalKey::camelot)
}

#[test]
fn detects_keys() {
    let a_minor = tones(&[57, 60, 64], &[69, 71, 72, 74, 76, 77, 79, 81], 10.0);
    assert_eq!(camelot(&a_minor).as_deref(), Some("8A"));
    let c_major = tones(&[60, 64, 67], &[72, 74, 76, 77, 79, 81, 83, 84], 10.0);
    assert_eq!(camelot(&c_major).as_deref(), Some("8B"));
    let f_sharp_major = tones(&[54, 58, 61], &[], 10.0);
    assert_eq!(camelot(&f_sharp_major).as_deref(), Some("2B"));
}

#[test]
fn silence_has_no_key_beats_or_loudness() {
    let analysis = analyze(
        &vec![0.0; 2 * 5 * RATE as usize],
        RATE,
        &AnalysisSettings::default(),
    );
    assert_eq!(analysis.key, None);
    assert_eq!(analysis.loudness_lufs, None);
    assert!(analysis.grid.is_empty());
    assert_eq!(analysis.bpm, 0.0);
    assert_eq!(analysis.waveform.len(), 750);
    assert!(analysis.waveform.low.iter().all(|&v| v == 0));
}

#[test]
fn empty_and_tiny_inputs_do_not_panic() {
    for len in [0, 1, 2, 64, 2 * 4410] {
        let analysis = analyze(&vec![0.1; len], RATE, &AnalysisSettings::default());
        assert!(analysis.grid.is_empty());
    }
    let analysis = analyze(&[0.5; 100], 0, &AnalysisSettings::default());
    assert!(analysis.waveform.is_empty());
}

#[test]
fn loudness_of_a_1khz_sine() {
    // EBU Tech 3341 case 1 at -20 dBFS: a stereo 1 kHz sine with peak 0.1
    // on each channel. Each channel's mean square is 0.1^2 / 2 = 0.005, the
    // two are summed, and K-weighting gain at 1 kHz is applied, so
    // L = -0.691 + 10 log10(2 * 0.005 * G^2), which BS.1770's offset makes
    // -20.0 LUFS to within a few hundredths.
    let rate = 48_000;
    let seconds = 10;
    let samples: Vec<f32> = (0..rate * seconds)
        .flat_map(|n| {
            let v = (0.1 * (TAU * 1000.0 * f64::from(n) / f64::from(rate)).sin()) as f32;
            [v, v]
        })
        .collect();
    let gain: f64 = {
        let k: Cascade<2> = loudness::k_weighting(f64::from(rate));
        k.gain(f64::from(rate), 1000.0)
    };
    let expected = -0.691 + 10.0 * (2.0 * 0.005 * gain * gain).log10();
    let measured = loudness::integrated_loudness(&samples, rate).unwrap_or(f32::NAN);
    println!("1 kHz at -20 dBFS: {measured} LUFS (expected {expected:.4})");
    assert!((f64::from(measured) - expected).abs() < 0.01);
    assert!((measured + 20.0).abs() < 0.05);
}

fn sine_waveform(hz: f64, seconds: f64) -> ColourWaveform {
    let frames = (seconds * f64::from(RATE)) as usize;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|n| {
            let v = (0.5 * (TAU * hz * n as f64 / f64::from(RATE)).sin()) as f32;
            [v, v]
        })
        .collect();
    analyze(&samples, RATE, &AnalysisSettings::default()).waveform
}

fn median(values: &[u8]) -> u8 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

#[test]
fn waveform_bands_separate() {
    let seconds = 2.003;
    let expected_len = (seconds * 150.0f64).ceil() as usize;

    let low = sine_waveform(60.0, seconds);
    assert_eq!(low.points_per_second, 150);
    assert_eq!(low.len(), expected_len);
    assert_eq!(low.mid.len(), expected_len);
    assert_eq!(low.high.len(), expected_len);
    println!(
        "60 Hz: low {} mid {} high {}",
        median(&low.low),
        median(&low.mid),
        median(&low.high)
    );
    assert!(median(&low.low) >= 120);
    assert!(median(&low.high) <= 2);

    let high = sine_waveform(6000.0, seconds);
    println!(
        "6 kHz: low {} mid {} high {}",
        median(&high.low),
        median(&high.mid),
        median(&high.high)
    );
    assert!(median(&high.high) >= 120);
    assert!(median(&high.low) <= 2);
}

/// Times a full analysis of a 6-minute track. Run with
/// `cargo test -p cuepoint-analysis --release -- --ignored --nocapture`.
#[test]
#[ignore]
fn six_minute_track_speed() {
    let mut track = KickTrack::new(126.0);
    track.seconds = 360.0;
    track.hats = true;
    let samples = track.render();
    let start = std::time::Instant::now();
    let analysis = analyze(&samples, RATE, &AnalysisSettings::default());
    println!("6 min track analysed in {:?}", start.elapsed());
    assert_eq!(analysis.bpm, 126.0);
}
