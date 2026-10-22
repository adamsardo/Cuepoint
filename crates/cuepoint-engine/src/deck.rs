//! One player deck: transport, cue and manual loop behaviour.
//!
//! Cue follows rekordbox's default CDJ-style behaviour (spec 03, section
//! 2.3): while paused, CUE stores the current position; held at the cue
//! point it previews and returns on release; while playing, it returns to
//! the cue point and pauses. Every jump or stop is covered by a short fade
//! so playback never clicks.

use std::sync::Arc;

use cuepoint_core::BeatGrid;
use cuepoint_dsp::{Ramp, hermite};

use crate::audio::Pcm;

/// Length of de-click fades, about 2.7 ms at 48 kHz.
pub const DECLICK_FRAMES: u32 = 128;

/// Positions within this many frames count as "at the cue point".
const AT_CUE_TOLERANCE: f64 = 1.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeckState {
    pub loaded: bool,
    pub frames: usize,
    /// Playhead in frames at the engine rate.
    pub position: f64,
    pub playing: bool,
    /// Playback speed ratio, 1.0 = original tempo.
    pub tempo: f64,
    pub cue: f64,
    pub loop_in: Option<f64>,
    pub loop_out: Option<f64>,
    pub loop_active: bool,
    /// Serial of the last load this deck accepted, 0 if none.
    pub load_serial: u64,
    /// Serial of the last load refused because the deck was playing.
    pub rejected_serial: u64,
    /// Beat sync engaged.
    pub sync: bool,
    /// This deck is the sync master.
    pub master: bool,
    /// Current tempo in BPM (grid tempo × playback speed), when gridded.
    pub bpm: Option<f64>,
    /// Fractional beat position on the grid, when gridded.
    pub beat: Option<f64>,
}

pub struct Deck {
    pcm: Option<Arc<Pcm>>,
    position: f64,
    playing: bool,
    tempo: f64,
    cue: f64,
    previewing: bool,
    loop_in: Option<f64>,
    loop_out: Option<f64>,
    loop_active: bool,
    /// Transport gain, ramping to 0 on stop and 1 on play.
    gain: Ramp,
    /// Where to land once a fade-out finishes.
    pending_seek: Option<f64>,
    /// Position faded out during a cross-faded jump, and frames left.
    fade_from: Option<(f64, u32)>,
    load_serial: u64,
    rejected_serial: u64,
    sample_rate: f64,
    grid: Option<Arc<BeatGrid>>,
    /// Temporary speed offset used by sync to pull the phase into line,
    /// e.g. 0.01 plays 1% fast. Not shown as tempo.
    bend: f64,
    sync: bool,
}

impl Default for Deck {
    fn default() -> Self {
        Self {
            pcm: None,
            position: 0.0,
            playing: false,
            tempo: 1.0,
            cue: 0.0,
            previewing: false,
            loop_in: None,
            loop_out: None,
            loop_active: false,
            gain: Ramp::new(0.0, DECLICK_FRAMES),
            pending_seek: None,
            fade_from: None,
            load_serial: 0,
            rejected_serial: 0,
            sample_rate: 48_000.0,
            grid: None,
            bend: 0.0,
            sync: false,
        }
    }
}

impl Deck {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: f64::from(sample_rate),
            ..Self::default()
        }
    }

    /// Loads a track, stopped at its start, unless the deck is playing
    /// (rekordbox's default "lock playing deck"); then the track is handed
    /// back as `Err`. On success returns the previous track. Either way the
    /// caller frees the returned track off the audio thread.
    pub fn load(&mut self, pcm: Arc<Pcm>, serial: u64) -> Result<Option<Arc<Pcm>>, Arc<Pcm>> {
        if self.playing {
            self.rejected_serial = serial;
            return Err(pcm);
        }
        let old = self.reset(Some(pcm));
        self.load_serial = serial;
        Ok(old)
    }

    pub fn unload(&mut self) -> Option<Arc<Pcm>> {
        self.reset(None)
    }

    /// Fresh state holding `pcm`, keeping tempo, sync, grid and load
    /// bookkeeping. The grid is replaced separately with `set_grid`.
    fn reset(&mut self, pcm: Option<Arc<Pcm>>) -> Option<Arc<Pcm>> {
        let old = self.pcm.take();
        *self = Self {
            pcm,
            tempo: self.tempo,
            load_serial: self.load_serial,
            rejected_serial: self.rejected_serial,
            sample_rate: self.sample_rate,
            grid: self.grid.take(),
            sync: self.sync,
            ..Self::default()
        };
        old
    }

    /// Replaces the beat grid, returning the old one for freeing off the
    /// audio thread.
    pub fn set_grid(&mut self, grid: Option<Arc<BeatGrid>>) -> Option<Arc<BeatGrid>> {
        std::mem::replace(&mut self.grid, grid)
    }

    pub fn is_loaded(&self) -> bool {
        self.pcm.is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Serial of the load that put the current track on this deck.
    pub fn load_serial(&self) -> u64 {
        self.load_serial
    }

    pub fn sync(&self) -> bool {
        self.sync
    }

    pub fn set_sync(&mut self, on: bool) {
        self.sync = on;
        if !on {
            self.bend = 0.0;
        }
    }

    pub fn set_bend(&mut self, bend: f64) {
        self.bend = bend;
    }

    /// Playback speed including any sync bend.
    fn speed(&self) -> f64 {
        self.tempo * (1.0 + self.bend)
    }

    fn seconds(&self) -> f64 {
        self.position / self.sample_rate
    }

    /// Beat position at the playhead, from the grid.
    pub fn beat(&self) -> Option<f64> {
        self.grid.as_ref()?.beat_at(self.seconds())
    }

    /// The grid's own tempo at the playhead, ignoring playback speed.
    pub fn grid_bpm(&self) -> Option<f64> {
        self.grid.as_ref()?.bpm_at(self.seconds())
    }

    /// Tempo heard now: grid tempo times the tempo setting.
    pub fn bpm(&self) -> Option<f64> {
        Some(self.grid_bpm()? * self.tempo)
    }

    #[cfg(test)]
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Moves the playhead by `beats` on the grid, cross-faded if audible.
    /// Moves the playhead by `beats` grid beats, following the grid
    /// through any tempo changes.
    pub fn shift_beats(&mut self, beats: f64) {
        let Some(grid) = &self.grid else { return };
        let target = grid
            .beat_at(self.seconds())
            .and_then(|beat| grid.time_of_beat(beat + beats));
        if let Some(target) = target {
            self.seek(target * self.sample_rate);
        }
    }

    fn frames(&self) -> f64 {
        self.pcm.as_ref().map_or(0.0, |p| p.frames() as f64)
    }

    pub fn state(&self) -> DeckState {
        DeckState {
            loaded: self.pcm.is_some(),
            frames: self.frames() as usize,
            position: self.pending_seek.unwrap_or(self.position),
            playing: self.playing,
            tempo: self.tempo,
            cue: self.cue,
            loop_in: self.loop_in,
            loop_out: self.loop_out,
            loop_active: self.loop_active,
            load_serial: self.load_serial,
            rejected_serial: self.rejected_serial,
            sync: self.sync,
            master: false,
            bpm: self.bpm(),
            beat: self.beat(),
        }
    }

    pub fn play_pause(&mut self) {
        if self.pcm.is_none() {
            return;
        }
        if self.previewing {
            // Play pressed while holding CUE: keep playing after release.
            self.previewing = false;
        } else if self.playing {
            self.stop_at(self.position);
        } else {
            self.start();
        }
    }

    pub fn cue_down(&mut self) {
        if self.pcm.is_none() {
            return;
        }
        if self.playing {
            self.previewing = false;
            self.stop_at(self.cue);
        } else if (self.state().position - self.cue).abs() <= AT_CUE_TOLERANCE {
            self.previewing = true;
            self.start();
        } else {
            self.cue = self.state().position;
        }
    }

    pub fn cue_up(&mut self) {
        if self.previewing {
            self.previewing = false;
            self.stop_at(self.cue);
        }
    }

    pub fn seek(&mut self, frame: f64) {
        let frame = frame.clamp(0.0, self.frames());
        if self.playing {
            self.jump(frame);
        } else if self.gain.is_settled() {
            self.position = frame;
        } else {
            self.pending_seek = Some(frame);
        }
    }

    pub fn set_tempo(&mut self, ratio: f64) {
        self.tempo = ratio.clamp(0.0, 4.0);
    }

    pub fn loop_in(&mut self) {
        if self.pcm.is_none() || self.loop_active {
            return;
        }
        self.loop_in = Some(self.state().position);
        self.loop_out = None;
    }

    pub fn loop_out(&mut self) {
        let Some(start) = self.loop_in else { return };
        let end = self.state().position;
        if end <= start {
            return;
        }
        self.loop_out = Some(end);
        self.loop_active = true;
        if self.playing {
            self.jump(start);
        }
    }

    /// Exits an active loop, or re-enters the last one at its start.
    pub fn reloop_exit(&mut self) {
        if self.loop_active {
            self.loop_active = false;
        } else if let (Some(start), Some(_)) = (self.loop_in, self.loop_out) {
            self.loop_active = true;
            self.seek(start);
        }
    }

    fn start(&mut self) {
        if let Some(target) = self.pending_seek.take() {
            self.position = target;
        }
        if self.position >= self.frames() {
            return;
        }
        self.playing = true;
        self.gain.set_target(1.0);
    }

    fn stop_at(&mut self, frame: f64) {
        self.playing = false;
        self.gain.set_target(0.0);
        self.pending_seek = Some(frame);
    }

    /// Cross-faded jump while audible.
    fn jump(&mut self, frame: f64) {
        self.fade_from = Some((self.position, DECLICK_FRAMES));
        self.position = frame;
    }

    #[inline]
    fn sample(pcm: &Pcm, position: f64) -> (f32, f32) {
        let index = position.floor() as isize;
        let t = (position - position.floor()) as f32;
        let (a0, b0) = pcm.frame(index - 1);
        let (a1, b1) = pcm.frame(index);
        let (a2, b2) = pcm.frame(index + 1);
        let (a3, b3) = pcm.frame(index + 2);
        (hermite(a0, a1, a2, a3, t), hermite(b0, b1, b2, b3, t))
    }

    /// Renders `out.len()` frames, adding nothing when unloaded or silent.
    pub fn render(&mut self, out: &mut [(f32, f32)]) {
        let Some(pcm) = self.pcm.clone() else {
            out.fill((0.0, 0.0));
            return;
        };
        let end = pcm.frames() as f64;
        let speed = self.speed();
        for frame in out.iter_mut() {
            let gain = self.gain.next_value();
            if gain == 0.0 && !self.playing {
                if let Some(target) = self.pending_seek.take() {
                    self.position = target;
                    self.fade_from = None;
                }
                *frame = (0.0, 0.0);
                continue;
            }
            let (mut l, mut r) = Self::sample(&pcm, self.position);
            if let Some((old, left)) = self.fade_from {
                let w = left as f32 / DECLICK_FRAMES as f32;
                let (ol, or) = Self::sample(&pcm, old);
                l = l * (1.0 - w) + ol * w;
                r = r * (1.0 - w) + or * w;
                self.fade_from = (left > 1).then_some((old + speed, left - 1));
            }
            *frame = (l * gain, r * gain);

            self.position += speed;
            if self.loop_active
                && let (Some(start), Some(stop)) = (self.loop_in, self.loop_out)
                && self.position >= stop
            {
                let wrapped = start + (self.position - stop);
                self.jump(wrapped);
            }
            // Start the stop fade while samples remain so the end never clicks,
            // unless an active loop will wrap before the end is reached.
            let loop_wraps = self.loop_active && self.loop_out.is_some_and(|stop| stop <= end);
            if self.playing
                && !loop_wraps
                && self.position + f64::from(DECLICK_FRAMES) * speed >= end
            {
                self.previewing = false;
                self.stop_at(end);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ramp 0, 1, 2, ... so sample values reveal positions.
    fn ramp_track(frames: usize) -> Arc<Pcm> {
        let samples = (0..frames).flat_map(|i| [i as f32, i as f32]).collect();
        Arc::new(Pcm::from_interleaved(48_000, samples))
    }

    fn run(deck: &mut Deck, frames: usize) -> Vec<f32> {
        let mut out = vec![(0.0, 0.0); frames];
        deck.render(&mut out);
        out.into_iter().map(|(l, _)| l).collect()
    }

    fn settle(deck: &mut Deck) {
        run(deck, DECLICK_FRAMES as usize + 1);
    }

    #[test]
    fn plays_at_tempo_and_pauses_in_place() {
        let mut deck = Deck::default();
        deck.load(ramp_track(10_000), 1).unwrap();
        deck.play_pause();
        settle(&mut deck);
        let out = run(&mut deck, 4);
        assert_eq!(out[1] - out[0], 1.0);

        deck.set_tempo(1.5);
        let out = run(&mut deck, 4);
        assert!((out[3] - out[2] - 1.5).abs() < 1e-3);

        let paused_at = deck.state().position;
        deck.play_pause();
        settle(&mut deck);
        assert_eq!(deck.state().position, paused_at);
        assert!(!deck.state().playing);
        assert!(run(&mut deck, 8).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn cdj_cue_sets_previews_and_returns() {
        let mut deck = Deck::default();
        deck.load(ramp_track(100_000), 1).unwrap();
        deck.seek(5_000.0);
        // Paused away from the cue: CUE stores the position.
        deck.cue_down();
        deck.cue_up();
        assert_eq!(deck.state().cue, 5_000.0);
        assert!(!deck.state().playing);

        // Paused at the cue: holding CUE previews, release returns.
        deck.cue_down();
        assert!(deck.state().playing);
        run(&mut deck, 2_000);
        deck.cue_up();
        settle(&mut deck);
        assert!(!deck.state().playing);
        assert_eq!(deck.state().position, 5_000.0);

        // Playing: CUE returns to the cue point and pauses.
        deck.play_pause();
        run(&mut deck, 3_000);
        deck.cue_down();
        deck.cue_up();
        settle(&mut deck);
        assert_eq!(deck.state().position, 5_000.0);
        assert!(!deck.state().playing);
    }

    #[test]
    fn play_during_cue_preview_keeps_playing() {
        let mut deck = Deck::default();
        deck.load(ramp_track(100_000), 1).unwrap();
        deck.cue_down();
        deck.play_pause();
        deck.cue_up();
        run(&mut deck, 1_000);
        assert!(deck.state().playing);
    }

    #[test]
    fn manual_loop_wraps_and_exits() {
        let mut deck = Deck::default();
        deck.load(ramp_track(100_000), 1).unwrap();
        deck.play_pause();
        run(&mut deck, 1_000);
        deck.loop_in();
        run(&mut deck, 500);
        deck.loop_out();
        let start = deck.state().loop_in.unwrap();
        let stop = deck.state().loop_out.unwrap();
        assert_eq!(stop - start, 500.0);
        // Many passes later the playhead is still inside the loop.
        run(&mut deck, 10_000);
        let pos = deck.state().position;
        assert!(pos >= start && pos < stop, "{pos} not in {start}..{stop}");

        deck.reloop_exit();
        run(&mut deck, 2_000);
        assert!(deck.state().position > stop);

        // Reloop jumps back to the loop start.
        deck.reloop_exit();
        assert!(deck.state().loop_active);
        assert!(deck.state().position < stop);
    }

    #[test]
    fn stops_at_track_end() {
        let mut deck = Deck::default();
        deck.load(ramp_track(1_000), 1).unwrap();
        deck.play_pause();
        run(&mut deck, 2_000);
        assert!(!deck.state().playing);
        assert_eq!(deck.state().position, 1_000.0);
        // Play at the end does nothing.
        deck.play_pause();
        assert!(!deck.state().playing);
    }

    #[test]
    fn jumps_and_stops_do_not_click() {
        // A constant signal: any discontinuity shows as a step.
        let mut deck = Deck::default();
        deck.load(
            Arc::new(Pcm::from_interleaved(48_000, vec![0.5; 200_000])),
            1,
        )
        .unwrap();
        deck.play_pause();
        let mut out = run(&mut deck, 1_000);
        deck.seek(50_000.0);
        out.extend(run(&mut deck, 1_000));
        deck.play_pause();
        out.extend(run(&mut deck, 1_000));
        let max_step = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(
            max_step <= 0.5 / DECLICK_FRAMES as f32 + 1e-6,
            "step {max_step}"
        );
    }

    #[test]
    fn track_end_fades_instead_of_clicking() {
        let mut deck = Deck::default();
        deck.load(Arc::new(Pcm::from_interleaved(48_000, vec![0.5; 2_000])), 1)
            .unwrap();
        deck.play_pause();
        let out = run(&mut deck, 1_500);
        let max_step = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(
            max_step <= 0.5 / DECLICK_FRAMES as f32 + 1e-6,
            "step {max_step}"
        );
        assert!(!deck.state().playing);
        assert_eq!(deck.state().position, 1_000.0);
    }

    #[test]
    fn refuses_loads_while_playing() {
        let mut deck = Deck::default();
        deck.load(ramp_track(10_000), 1).unwrap();
        deck.play_pause();
        assert!(deck.load(ramp_track(10_000), 2).is_err());
        let state = deck.state();
        assert_eq!((state.load_serial, state.rejected_serial), (1, 2));
        assert!(state.playing);

        deck.play_pause();
        settle(&mut deck);
        assert!(deck.load(ramp_track(10_000), 3).unwrap().is_some());
        assert_eq!(deck.state().load_serial, 3);
    }

    #[test]
    fn loop_ending_at_track_end_keeps_looping() {
        let mut deck = Deck::default();
        deck.load(ramp_track(10_000), 1).unwrap();
        deck.seek(9_000.0);
        deck.loop_in();
        deck.seek(10_000.0);
        deck.loop_out();
        deck.seek(9_000.0);
        deck.play_pause();
        run(&mut deck, 5_000);
        let state = deck.state();
        assert!(state.playing && state.loop_active);
        assert!(
            (9_000.0..10_000.0).contains(&state.position),
            "{}",
            state.position
        );
    }

    #[test]
    fn beat_jump_follows_tempo_changes() {
        use cuepoint_core::TempoMarker;
        let mut deck = Deck::new(48_000);
        deck.load(ramp_track(48_000 * 30), 1).unwrap();
        deck.set_grid(Some(Arc::new(BeatGrid::from_markers(vec![
            TempoMarker {
                time: 0.0,
                bpm: 120.0,
                beat_in_bar: 1,
            },
            TempoMarker {
                time: 10.0,
                bpm: 60.0,
                beat_in_bar: 1,
            },
        ]))));
        deck.seek(9.0 * 48_000.0);
        deck.shift_beats(4.0);
        // Beats 18, 19 at 120 BPM, then 20 at 10 s and 21, 22 at 60 BPM.
        assert!((deck.state().position / 48_000.0 - 12.0).abs() < 1e-9);
        deck.shift_beats(-4.0);
        assert!((deck.state().position / 48_000.0 - 9.0).abs() < 1e-9);
    }
}
