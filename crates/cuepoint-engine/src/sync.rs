//! Beat sync: one master deck sets the tempo, synced decks follow it and
//! keep their beats in phase (spec 03, section 2.5).
//!
//! Follows rekordbox's Beat Sync model: pressing SYNC matches tempo at
//! once; the phase snaps into line when both decks are playing (on engage,
//! or when the synced deck starts) and is then held by small, inaudible
//! speed bends. Tempos are matched at half or double time when that keeps
//! the follower nearer its own tempo. Bar sync and the exact correction
//! curve rekordbox uses are open questions (spec 03, section 6).

use crate::deck::Deck;
use crate::engine::DECKS;

/// Largest speed bend used to hold phase, as a fraction (3%).
const MAX_BEND: f64 = 0.03;
/// Phase errors below this many beats are left alone.
const DEADBAND_BEATS: f64 = 0.002;
/// Errors above this many beats at engage time are fixed with a jump.
const SNAP_BEATS: f64 = 0.005;

#[derive(Debug, Default)]
pub struct SyncManager {
    master: Option<usize>,
    was_playing: [bool; DECKS],
    needs_phase_lock: [bool; DECKS],
    /// Load serial of the master's track, to notice it being replaced.
    master_serial: u64,
}

fn gridded(deck: &Deck) -> bool {
    deck.is_loaded() && deck.grid_bpm().is_some()
}

/// `x` wrapped into -0.5..0.5.
fn wrap_phase(x: f64) -> f64 {
    x - x.round()
}

impl SyncManager {
    pub fn master(&self) -> Option<usize> {
        self.master
    }

    pub fn set_master(&mut self, decks: &[Deck; DECKS], deck: usize) {
        if decks.get(deck).is_some_and(gridded) {
            self.master = Some(deck);
            self.master_serial = decks[deck].load_serial();
        }
    }

    pub fn set_sync(&mut self, decks: &mut [Deck; DECKS], deck: usize, on: bool) {
        let Some(target) = decks.get_mut(deck) else {
            return;
        };
        target.set_sync(on);
        if !on {
            return;
        }
        self.needs_phase_lock[deck] = true;
        if self.master.is_none() || self.master == Some(deck) {
            // Follow another gridded deck, preferring one that is playing;
            // with nobody to follow, this deck leads.
            let others = || (0..DECKS).filter(|&d| d != deck && gridded(&decks[d]));
            self.master = others()
                .find(|&d| decks[d].is_playing())
                .or_else(|| others().next())
                .or(Some(deck));
            if let Some(master) = self.master {
                self.master_serial = decks[master].load_serial();
            }
        }
    }

    /// When the master stops or its track is replaced, a synced deck that is
    /// still playing takes over, so the live mix keeps its tempo (as
    /// rekordbox moves MASTER to a playing deck).
    fn hand_off_stopped_master(&mut self, decks: &[Deck; DECKS]) {
        let Some(master) = self.master else { return };
        let stopped = self.was_playing[master] && !decks[master].is_playing();
        let replaced = decks[master].load_serial() != self.master_serial;
        if stopped || replaced {
            let heir = (0..DECKS).find(|&d| {
                d != master && decks[d].sync() && decks[d].is_playing() && gridded(&decks[d])
            });
            if let Some(heir) = heir {
                self.master = Some(heir);
            }
        }
        if let Some(master) = self.master {
            self.master_serial = decks[master].load_serial();
        }
    }

    /// Runs once per block, before rendering.
    pub fn update(&mut self, decks: &mut [Deck; DECKS]) {
        if self.master.is_some_and(|m| !gridded(&decks[m])) {
            self.master = None;
        }
        self.hand_off_stopped_master(decks);
        for (index, deck) in decks.iter().enumerate() {
            if deck.is_playing() && !self.was_playing[index] && deck.sync() {
                self.needs_phase_lock[index] = true;
            }
            self.was_playing[index] = deck.is_playing();
        }
        for slave in 0..DECKS {
            if !decks[slave].sync() || Some(slave) == self.master {
                decks[slave].set_bend(0.0);
                continue;
            }
            let Some(master) = self.master else {
                decks[slave].set_bend(0.0);
                continue;
            };
            let (Some(master_bpm), Some(master_beat)) = (decks[master].bpm(), decks[master].beat())
            else {
                continue;
            };
            let (Some(own_bpm), Some(own_beat)) = (decks[slave].grid_bpm(), decks[slave].beat())
            else {
                continue;
            };
            // A stopped master (tempo -100% in WIDE) gives no tempo to follow,
            // and would never leave the folding loops below.
            if !(master_bpm > 0.0 && master_bpm.is_finite() && own_bpm > 0.0) {
                decks[slave].set_bend(0.0);
                continue;
            }
            let mut ratio = master_bpm / own_bpm;
            while ratio > 1.5 {
                ratio /= 2.0;
            }
            while ratio < 0.75 {
                ratio *= 2.0;
            }
            // Slave beats per master beat at the matched tempo: 1, 0.5 or 2.
            let scale = own_bpm * ratio / master_bpm;
            let deck = &mut decks[slave];
            deck.set_tempo(ratio);
            if !(deck.is_playing() && self.was_playing[master]) {
                deck.set_bend(0.0);
                continue;
            }
            let error = wrap_phase(own_beat - master_beat * scale);
            if self.needs_phase_lock[slave] {
                self.needs_phase_lock[slave] = false;
                deck.set_bend(0.0);
                if error.abs() > SNAP_BEATS {
                    deck.shift_beats(-error);
                }
            } else if error.abs() < DEADBAND_BEATS {
                deck.set_bend(0.0);
            } else {
                // Remove the error over roughly one beat.
                deck.set_bend((-error).clamp(-MAX_BEND, MAX_BEND));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cuepoint_core::BeatGrid;

    use super::*;
    use crate::audio::Pcm;

    const RATE: u32 = 48_000;

    fn deck_with_grid(bpm: f64, first_beat: f64) -> Deck {
        let mut deck = Deck::new(RATE);
        let pcm = Arc::new(Pcm::from_interleaved(
            RATE,
            vec![0.1; RATE as usize * 120 * 2],
        ));
        deck.load(pcm, 1).unwrap();
        deck.set_grid(Some(Arc::new(BeatGrid::constant(bpm, first_beat, 1))));
        deck
    }

    /// Runs `seconds` of audio in engine-sized blocks.
    fn run(sync: &mut SyncManager, decks: &mut [Deck; DECKS], seconds: f64) {
        let mut buffer = vec![(0.0, 0.0); 512];
        for _ in 0..(seconds * f64::from(RATE) / 512.0) as usize {
            sync.update(decks);
            for deck in decks.iter_mut() {
                deck.render(&mut buffer);
            }
        }
    }

    fn phase_error(decks: &[Deck; DECKS]) -> f64 {
        wrap_phase(decks[1].beat().unwrap() - decks[0].beat().unwrap())
    }

    #[test]
    fn follower_matches_tempo_and_locks_phase() {
        let mut decks = [deck_with_grid(128.0, 0.0), deck_with_grid(124.0, 0.21)];
        let mut sync = SyncManager::default();
        decks[0].play_pause();
        decks[1].play_pause();
        run(&mut sync, &mut decks, 1.0);
        sync.set_sync(&mut decks, 1, true);
        assert_eq!(sync.master(), Some(0));
        run(&mut sync, &mut decks, 2.0);
        assert!((decks[1].bpm().unwrap() - 128.0).abs() < 1e-9);
        assert!(phase_error(&decks).abs() < 0.01, "{}", phase_error(&decks));

        // The master speeds up; the follower keeps up and stays in phase.
        decks[0].set_tempo(1.05);
        run(&mut sync, &mut decks, 8.0);
        assert!((decks[1].bpm().unwrap() - 128.0 * 1.05).abs() < 1e-6);
        assert!(phase_error(&decks).abs() < 0.01, "{}", phase_error(&decks));
    }

    #[test]
    fn small_drift_is_pulled_back_with_bends() {
        let mut decks = [deck_with_grid(128.0, 0.0), deck_with_grid(128.0, 0.0)];
        let mut sync = SyncManager::default();
        decks[0].play_pause();
        decks[1].play_pause();
        sync.set_sync(&mut decks, 1, true);
        run(&mut sync, &mut decks, 0.5);
        // Knock the follower a little out of phase without a re-lock.
        decks[1].shift_beats(0.004);
        run(&mut sync, &mut decks, 0.05);
        decks[1].shift_beats(0.04);
        run(&mut sync, &mut decks, 4.0);
        assert!(
            phase_error(&decks).abs() < DEADBAND_BEATS * 2.0,
            "{}",
            phase_error(&decks)
        );
    }

    #[test]
    fn half_and_double_time_matching() {
        let mut decks = [deck_with_grid(174.0, 0.0), deck_with_grid(87.0, 0.1)];
        let mut sync = SyncManager::default();
        decks[0].play_pause();
        decks[1].play_pause();
        sync.set_sync(&mut decks, 1, true);
        run(&mut sync, &mut decks, 2.0);
        // 87 BPM follows 174 at its own speed, a beat every two master beats.
        assert!((decks[1].tempo() - 1.0).abs() < 1e-9);
        let error = wrap_phase(decks[1].beat().unwrap() - decks[0].beat().unwrap() * 0.5);
        assert!(error.abs() < 0.01, "{error}");
    }

    #[test]
    fn first_deck_to_sync_with_nobody_to_follow_leads() {
        let mut decks = [deck_with_grid(128.0, 0.0), Deck::new(RATE)];
        let mut sync = SyncManager::default();
        sync.set_sync(&mut decks, 0, true);
        assert_eq!(sync.master(), Some(0));
        decks[0].unload();
        sync.update(&mut decks);
        assert_eq!(sync.master(), None);
    }

    #[test]
    fn stopped_master_does_not_hang_sync() {
        let mut decks = [deck_with_grid(128.0, 0.0), deck_with_grid(124.0, 0.0)];
        let mut sync = SyncManager::default();
        decks[0].play_pause();
        decks[1].play_pause();
        sync.set_sync(&mut decks, 1, true);
        decks[0].set_tempo(0.0);
        run(&mut sync, &mut decks, 0.1);
        decks[0].set_tempo(1.0);
        run(&mut sync, &mut decks, 1.0);
        assert!((decks[1].bpm().unwrap() - 128.0).abs() < 1e-9);
    }

    #[test]
    fn master_hands_off_to_playing_follower() {
        let mut decks = [deck_with_grid(128.0, 0.0), deck_with_grid(120.0, 0.0)];
        let mut sync = SyncManager::default();
        decks[0].play_pause();
        decks[1].play_pause();
        sync.set_sync(&mut decks, 1, true);
        run(&mut sync, &mut decks, 0.5);
        assert_eq!(sync.master(), Some(0));

        // Pausing the master passes MASTER to the deck still playing.
        decks[0].play_pause();
        run(&mut sync, &mut decks, 0.1);
        assert_eq!(sync.master(), Some(1));

        // A slower track loaded on the old master leaves the mix alone.
        let pcm = Arc::new(Pcm::from_interleaved(
            RATE,
            vec![0.1; RATE as usize * 60 * 2],
        ));
        decks[0].load(pcm, 2).unwrap();
        decks[0].set_grid(Some(Arc::new(BeatGrid::constant(100.0, 0.0, 1))));
        run(&mut sync, &mut decks, 0.5);
        assert!((decks[1].bpm().unwrap() - 128.0).abs() < 1e-9);
    }

    #[test]
    fn replacing_a_stopped_master_track_hands_off_too() {
        let mut decks = [deck_with_grid(128.0, 0.0), deck_with_grid(120.0, 0.0)];
        let mut sync = SyncManager::default();
        sync.set_sync(&mut decks, 1, true);
        assert_eq!(sync.master(), Some(0));
        decks[1].play_pause();
        run(&mut sync, &mut decks, 0.2);
        let pcm = Arc::new(Pcm::from_interleaved(
            RATE,
            vec![0.1; RATE as usize * 60 * 2],
        ));
        decks[0].load(pcm, 2).unwrap();
        decks[0].set_grid(Some(Arc::new(BeatGrid::constant(100.0, 0.0, 1))));
        run(&mut sync, &mut decks, 0.2);
        assert_eq!(sync.master(), Some(1));
        assert!((decks[1].bpm().unwrap() - 128.0).abs() < 1e-9);
    }
}
