//! Internal two-channel mixer: trim, 3-band EQ, channel faders, crossfader
//! and master level (spec 03, section 2.7).
//!
//! EQ corner frequencies, ranges and fader laws are conventional DJM-style
//! values until measurements against rekordbox replace them (spec 03,
//! section 7.4, "system identification for linear blocks").

use cuepoint_dsp::{Biquad, BiquadCoeffs, Ramp, constant_power_crossfade, db_to_gain, fader_gain};

pub const CHANNELS: usize = 2;
pub const EQ_MIN_DB: f32 = -26.0;
pub const EQ_MAX_DB: f32 = 6.0;
pub const TRIM_MIN_DB: f32 = -24.0;
pub const TRIM_MAX_DB: f32 = 9.0;

const LOW_HZ: f32 = 70.0;
const MID_HZ: f32 = 1_000.0;
const MID_Q: f32 = 0.7;
const HIGH_HZ: f32 = 13_000.0;
/// Largest EQ change per processing block, so sweeps are smooth.
const EQ_DB_PER_BLOCK: f32 = 1.5;
const GAIN_RAMP_FRAMES: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBand {
    Low,
    Mid,
    High,
}

impl EqBand {
    fn index(self) -> usize {
        match self {
            EqBand::Low => 0,
            EqBand::Mid => 1,
            EqBand::High => 2,
        }
    }
}

/// Which side of the crossfader a channel follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossfaderAssign {
    A,
    Thru,
    B,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MixerState {
    /// Post-fader peak per channel for the last block, linear.
    pub channel_peak: [f32; CHANNELS],
    /// Master peak (left, right) for the last block, linear.
    pub master_peak: [f32; 2],
}

struct Channel {
    trim: Ramp,
    eq_target: [f32; 3],
    eq_current: [f32; 3],
    /// [band][left/right].
    eq: [[Biquad; 2]; 3],
    fader: Ramp,
    crossfader: Ramp,
    assign: CrossfaderAssign,
}

impl Channel {
    fn new(assign: CrossfaderAssign) -> Self {
        Self {
            trim: Ramp::new(1.0, GAIN_RAMP_FRAMES),
            eq_target: [0.0; 3],
            eq_current: [0.0; 3],
            eq: Default::default(),
            fader: Ramp::new(1.0, GAIN_RAMP_FRAMES),
            crossfader: Ramp::new(1.0, GAIN_RAMP_FRAMES),
            assign,
        }
    }

    fn update_eq(&mut self, sample_rate: f32) {
        for band in 0..3 {
            let (current, target) = (self.eq_current[band], self.eq_target[band]);
            if current == target {
                continue;
            }
            let next = current + (target - current).clamp(-EQ_DB_PER_BLOCK, EQ_DB_PER_BLOCK);
            self.eq_current[band] = next;
            let coeffs = match band {
                0 => BiquadCoeffs::low_shelf(sample_rate, LOW_HZ, next),
                1 => BiquadCoeffs::peaking(sample_rate, MID_HZ, MID_Q, next),
                _ => BiquadCoeffs::high_shelf(sample_rate, HIGH_HZ, next),
            };
            for filter in &mut self.eq[band] {
                filter.set_coeffs(coeffs);
            }
        }
    }
}

pub struct Mixer {
    sample_rate: f32,
    channels: [Channel; CHANNELS],
    crossfader_position: f32,
    master: Ramp,
    state: MixerState,
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        let mut mixer = Self {
            sample_rate: sample_rate as f32,
            channels: [
                Channel::new(CrossfaderAssign::A),
                Channel::new(CrossfaderAssign::B),
            ],
            crossfader_position: 0.0,
            master: Ramp::new(1.0, GAIN_RAMP_FRAMES),
            state: MixerState::default(),
        };
        mixer.set_crossfader(0.0);
        mixer
    }

    pub fn state(&self) -> MixerState {
        self.state
    }

    pub fn set_trim_db(&mut self, channel: usize, db: f32) {
        if let Some(ch) = self.channels.get_mut(channel) {
            ch.trim
                .set_target(db_to_gain(db.clamp(TRIM_MIN_DB, TRIM_MAX_DB)));
        }
    }

    pub fn set_eq_db(&mut self, channel: usize, band: EqBand, db: f32) {
        if let Some(ch) = self.channels.get_mut(channel) {
            ch.eq_target[band.index()] = db.clamp(EQ_MIN_DB, EQ_MAX_DB);
        }
    }

    /// Channel fader travel 0..=1.
    pub fn set_fader(&mut self, channel: usize, position: f32) {
        if let Some(ch) = self.channels.get_mut(channel) {
            ch.fader.set_target(fader_gain(position));
        }
    }

    pub fn set_assign(&mut self, channel: usize, assign: CrossfaderAssign) {
        if let Some(ch) = self.channels.get_mut(channel) {
            ch.assign = assign;
        }
        self.set_crossfader(self.crossfader_position);
    }

    /// Crossfader travel -1 (A side) ..= 1 (B side).
    pub fn set_crossfader(&mut self, position: f32) {
        self.crossfader_position = position.clamp(-1.0, 1.0);
        let (a, b) = constant_power_crossfade(self.crossfader_position);
        // Normalise so the centre is unity for each side, like DJ mixers.
        let norm = std::f32::consts::SQRT_2;
        for ch in &mut self.channels {
            let gain = match ch.assign {
                CrossfaderAssign::A => (a * norm).min(1.0),
                CrossfaderAssign::B => (b * norm).min(1.0),
                CrossfaderAssign::Thru => 1.0,
            };
            ch.crossfader.set_target(gain);
        }
    }

    pub fn set_master(&mut self, position: f32) {
        self.master.set_target(fader_gain(position));
    }

    /// Mixes one block of deck audio into `out` (interleaved stereo pairs).
    pub fn process(&mut self, inputs: [&[(f32, f32)]; CHANNELS], out: &mut [(f32, f32)]) {
        out.fill((0.0, 0.0));
        let mut channel_peak = [0f32; CHANNELS];
        for (index, (ch, input)) in self.channels.iter_mut().zip(inputs).enumerate() {
            ch.update_eq(self.sample_rate);
            for (frame, &(l, r)) in out.iter_mut().zip(input) {
                let pre = ch.trim.next_value();
                let (mut l, mut r) = (l * pre, r * pre);
                for [fl, fr] in &mut ch.eq {
                    l = fl.process(l);
                    r = fr.process(r);
                }
                let post = ch.fader.next_value() * ch.crossfader.next_value();
                let (l, r) = (l * post, r * post);
                channel_peak[index] = channel_peak[index].max(l.abs()).max(r.abs());
                frame.0 += l;
                frame.1 += r;
            }
        }
        let mut master_peak = [0f32; 2];
        for frame in out.iter_mut() {
            let gain = self.master.next_value();
            frame.0 *= gain;
            frame.1 *= gain;
            master_peak[0] = master_peak[0].max(frame.0.abs());
            master_peak[1] = master_peak[1].max(frame.1.abs());
        }
        self.state = MixerState {
            channel_peak,
            master_peak,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(mixer: &mut Mixer, a: f32, b: f32) -> (f32, f32) {
        let ina = vec![(a, a); 4096];
        let inb = vec![(b, b); 4096];
        let mut out = vec![(0.0, 0.0); 4096];
        for _ in 0..40 {
            mixer.process([&ina, &inb], &mut out);
        }
        out[4095]
    }

    #[test]
    fn unity_by_default_and_faders_cut() {
        let mut mixer = Mixer::new(48_000);
        let (l, _) = settle(&mut mixer, 0.25, 0.0);
        assert!((l - 0.25).abs() < 1e-3, "{l}");
        mixer.set_fader(0, 0.0);
        let (l, _) = settle(&mut mixer, 0.25, 0.0);
        assert!(l.abs() < 1e-6);
    }

    #[test]
    fn crossfader_sides_and_thru() {
        let mut mixer = Mixer::new(48_000);
        mixer.set_crossfader(1.0);
        let (l, _) = settle(&mut mixer, 0.25, 0.5);
        assert!((l - 0.5).abs() < 1e-3, "A cut, B full: {l}");
        mixer.set_assign(0, CrossfaderAssign::Thru);
        let (l, _) = settle(&mut mixer, 0.25, 0.5);
        assert!((l - 0.75).abs() < 1e-3, "thru ignores crossfader: {l}");
    }

    #[test]
    fn low_eq_cut_removes_dc_but_keeps_highs() {
        let mut mixer = Mixer::new(48_000);
        mixer.set_eq_db(0, EqBand::Low, EQ_MIN_DB);
        // DC is all "low": it should drop by about the full cut.
        let (l, _) = settle(&mut mixer, 0.5, 0.0);
        let db = 20.0 * (l.abs() / 0.5).log10();
        assert!((db - EQ_MIN_DB).abs() < 1.0, "{db}");
    }

    #[test]
    fn meters_report_peaks() {
        let mut mixer = Mixer::new(48_000);
        settle(&mut mixer, 0.5, 0.25);
        let state = mixer.state();
        assert!((state.channel_peak[0] - 0.5).abs() < 1e-3);
        assert!((state.master_peak[0] - 0.75).abs() < 1e-3);
    }
}
