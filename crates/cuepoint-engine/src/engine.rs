//! The real-time engine and the control-side handle that drives it.
//!
//! The UI talks to the engine only through a bounded command queue and reads
//! a state snapshot published once per block (spec 03, section 7.3 rules 1,
//! 2 and 4). Tracks replaced on a deck are sent back on a trash queue so
//! their memory is freed off the audio thread (rule 3).

use std::sync::Arc;

use triple_buffer::TripleBuffer;

use crate::audio::Pcm;
use crate::deck::{Deck, DeckState};
use crate::mixer::{CHANNELS, CrossfaderAssign, EqBand, Mixer, MixerState};

pub const DECKS: usize = CHANNELS;
/// Frames processed per internal block; device buffers are split to fit.
pub const MAX_BLOCK: usize = 512;
const COMMAND_CAPACITY: usize = 1024;
const TRASH_CAPACITY: usize = 16;

#[derive(Debug, Clone)]
pub enum Command {
    /// Refused while the deck plays. `serial` then appears in the deck's
    /// `rejected_serial`, otherwise in its `load_serial`, so the UI learns
    /// which happened.
    Load {
        deck: usize,
        pcm: Arc<Pcm>,
        serial: u64,
    },
    Unload {
        deck: usize,
    },
    PlayPause {
        deck: usize,
    },
    CueDown {
        deck: usize,
    },
    CueUp {
        deck: usize,
    },
    Seek {
        deck: usize,
        frame: f64,
    },
    SetTempo {
        deck: usize,
        ratio: f64,
    },
    LoopIn {
        deck: usize,
    },
    LoopOut {
        deck: usize,
    },
    ReloopExit {
        deck: usize,
    },
    SetTrimDb {
        channel: usize,
        db: f32,
    },
    SetEqDb {
        channel: usize,
        band: EqBand,
        db: f32,
    },
    SetFader {
        channel: usize,
        position: f32,
    },
    SetAssign {
        channel: usize,
        assign: CrossfaderAssign,
    },
    SetCrossfader {
        position: f32,
    },
    SetMaster {
        position: f32,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EngineState {
    pub sample_rate: u32,
    pub decks: [DeckState; DECKS],
    pub mixer: MixerState,
    /// Blocks processed so far; lets the UI tell whether audio is running.
    pub blocks: u64,
}

/// Lives on the audio thread.
pub struct Engine {
    sample_rate: u32,
    decks: [Deck; DECKS],
    mixer: Mixer,
    commands: rtrb::Consumer<Command>,
    trash: rtrb::Producer<Arc<Pcm>>,
    state: triple_buffer::Input<EngineState>,
    deck_buffers: [Box<[(f32, f32)]>; DECKS],
    mix_buffer: Box<[(f32, f32)]>,
    blocks: u64,
}

/// Lives on the control (UI) thread.
pub struct EngineHandle {
    sample_rate: u32,
    commands: rtrb::Producer<Command>,
    trash: rtrb::Consumer<Arc<Pcm>>,
    state: triple_buffer::Output<EngineState>,
}

/// Creates an engine running at `sample_rate` and the handle that controls it.
pub fn engine(sample_rate: u32) -> (EngineHandle, Engine) {
    let (command_tx, command_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
    let (trash_tx, trash_rx) = rtrb::RingBuffer::new(TRASH_CAPACITY);
    let initial = EngineState {
        sample_rate,
        ..EngineState::default()
    };
    let (state_in, state_out) = TripleBuffer::new(&initial).split();
    let silent = || vec![(0.0, 0.0); MAX_BLOCK].into_boxed_slice();
    let engine = Engine {
        sample_rate,
        decks: Default::default(),
        mixer: Mixer::new(sample_rate),
        commands: command_rx,
        trash: trash_tx,
        state: state_in,
        deck_buffers: [silent(), silent()],
        mix_buffer: silent(),
        blocks: 0,
    };
    let handle = EngineHandle {
        sample_rate,
        commands: command_tx,
        trash: trash_rx,
        state: state_out,
    };
    (handle, engine)
}

impl EngineHandle {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Queues a command. Returns false if the queue is full, which only
    /// happens when the audio thread has stalled.
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.push(command).is_ok()
    }

    /// Latest published state. Also frees tracks the engine has released.
    pub fn state(&mut self) -> EngineState {
        while self.trash.pop().is_ok() {}
        *self.state.read()
    }
}

impl Engine {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Load { deck, pcm, serial } => {
                let Some(d) = self.decks.get_mut(deck) else {
                    return self.discard(pcm);
                };
                match d.load(pcm, serial) {
                    Ok(Some(old)) | Err(old) => self.discard(old),
                    Ok(None) => {}
                }
            }
            Command::Unload { deck } => {
                if let Some(d) = self.decks.get_mut(deck)
                    && let Some(old) = d.unload()
                {
                    self.discard(old);
                }
            }
            Command::PlayPause { deck } => self.with_deck(deck, Deck::play_pause),
            Command::CueDown { deck } => self.with_deck(deck, Deck::cue_down),
            Command::CueUp { deck } => self.with_deck(deck, Deck::cue_up),
            Command::Seek { deck, frame } => self.with_deck(deck, |d| d.seek(frame)),
            Command::SetTempo { deck, ratio } => self.with_deck(deck, |d| d.set_tempo(ratio)),
            Command::LoopIn { deck } => self.with_deck(deck, Deck::loop_in),
            Command::LoopOut { deck } => self.with_deck(deck, Deck::loop_out),
            Command::ReloopExit { deck } => self.with_deck(deck, Deck::reloop_exit),
            Command::SetTrimDb { channel, db } => self.mixer.set_trim_db(channel, db),
            Command::SetEqDb { channel, band, db } => self.mixer.set_eq_db(channel, band, db),
            Command::SetFader { channel, position } => self.mixer.set_fader(channel, position),
            Command::SetAssign { channel, assign } => self.mixer.set_assign(channel, assign),
            Command::SetCrossfader { position } => self.mixer.set_crossfader(position),
            Command::SetMaster { position } => self.mixer.set_master(position),
        }
    }

    fn with_deck(&mut self, index: usize, action: impl FnOnce(&mut Deck)) {
        if let Some(deck) = self.decks.get_mut(index) {
            action(deck);
        }
    }

    /// Hands a released track to the control thread. If that queue is full
    /// the track is dropped here, which allocates nothing but may free.
    fn discard(&mut self, pcm: Arc<Pcm>) {
        let _ = self.trash.push(pcm);
    }

    /// Fills interleaved `out` with `channels` channels; the mix goes to the
    /// first two and any others are silent.
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        if channels == 0 {
            return;
        }
        while let Ok(command) = self.commands.pop() {
            self.apply(command);
        }
        for chunk in out.chunks_mut(MAX_BLOCK * channels) {
            let frames = chunk.len() / channels;
            for (deck, buffer) in self.decks.iter_mut().zip(&mut self.deck_buffers) {
                deck.render(&mut buffer[..frames]);
            }
            let [a, b] = &self.deck_buffers;
            self.mixer
                .process([&a[..frames], &b[..frames]], &mut self.mix_buffer[..frames]);
            for (frame, &(l, r)) in chunk.chunks_mut(channels).zip(self.mix_buffer.iter()) {
                frame[0] = l;
                if channels > 1 {
                    frame[1] = r;
                }
                for extra in frame.iter_mut().skip(2) {
                    *extra = 0.0;
                }
            }
            self.blocks += 1;
        }
        self.publish();
    }

    fn publish(&mut self) {
        let state = self.state.input_buffer_mut();
        state.sample_rate = self.sample_rate;
        for (slot, deck) in state.decks.iter_mut().zip(&self.decks) {
            *slot = deck.state();
        }
        state.mixer = self.mixer.state();
        state.blocks = self.blocks;
        self.state.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, level: f32) -> Arc<Pcm> {
        Arc::new(Pcm::from_interleaved(48_000, vec![level; frames * 2]))
    }

    #[test]
    fn commands_drive_decks_and_state_reaches_the_handle() {
        let (mut handle, mut engine) = engine(48_000);
        handle.send(Command::Load {
            deck: 0,
            pcm: tone(48_000, 0.5),
            serial: 1,
        });
        handle.send(Command::PlayPause { deck: 0 });
        let mut out = vec![0.0; 2 * 4096];
        engine.process(&mut out, 2);
        let state = handle.state();
        assert!(state.decks[0].playing);
        assert!(state.decks[0].position > 4000.0);
        assert!(!state.decks[1].loaded);
        assert!((out[out.len() - 2] - 0.5).abs() < 1e-3);
        assert!(state.blocks >= 8);
    }

    #[test]
    fn replaced_tracks_come_back_for_freeing() {
        let (mut handle, mut engine) = engine(48_000);
        let first = tone(1000, 0.1);
        handle.send(Command::Load {
            deck: 1,
            pcm: first.clone(),
            serial: 1,
        });
        engine.process(&mut [0.0; 64], 2);
        handle.send(Command::Load {
            deck: 1,
            pcm: tone(1000, 0.2),
            serial: 1,
        });
        engine.process(&mut [0.0; 64], 2);
        assert_eq!(
            Arc::strong_count(&first),
            2,
            "engine released it to the trash queue"
        );
        handle.state();
        assert_eq!(Arc::strong_count(&first), 1, "the handle freed it");
    }

    #[global_allocator]
    static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

    /// Spec 03, section 7.3 rule 1: the audio callback never allocates or
    /// frees, even while tracks are swapped, looped, seeked and mixed.
    #[test]
    fn process_is_allocation_free() {
        let (mut handle, mut engine) = engine(48_000);
        let mut out = vec![0.0; 2 * 2048];
        let commands = [
            Command::Load {
                deck: 0,
                pcm: tone(96_000, 0.3),
                serial: 1,
            },
            Command::Load {
                deck: 1,
                pcm: tone(96_000, 0.3),
                serial: 1,
            },
            Command::PlayPause { deck: 0 },
            Command::PlayPause { deck: 1 },
            Command::SetTempo {
                deck: 1,
                ratio: 1.06,
            },
            Command::LoopIn { deck: 0 },
            Command::LoopOut { deck: 0 },
            Command::SetEqDb {
                channel: 0,
                band: EqBand::Mid,
                db: -12.0,
            },
            Command::SetCrossfader { position: 0.5 },
            Command::Seek {
                deck: 1,
                frame: 10_000.0,
            },
            Command::Load {
                deck: 0,
                pcm: tone(96_000, 0.3),
                serial: 1,
            },
            Command::CueDown { deck: 1 },
        ];
        for command in commands {
            handle.send(command);
            assert_no_alloc::assert_no_alloc(|| engine.process(&mut out, 2));
        }
        // The replaced track was handed back rather than freed in process().
        handle.state();
    }

    #[test]
    fn extra_output_channels_are_silent() {
        let (mut handle, mut engine) = engine(48_000);
        handle.send(Command::Load {
            deck: 0,
            pcm: tone(48_000, 0.5),
            serial: 1,
        });
        handle.send(Command::PlayPause { deck: 0 });
        let mut out = vec![1.0; 4 * 1024];
        engine.process(&mut out, 4);
        let last = &out[out.len() - 4..];
        assert!(last[0] > 0.4 && last[2] == 0.0 && last[3] == 0.0);
    }
}
