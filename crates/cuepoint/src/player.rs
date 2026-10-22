//! Owns the audio engine on the app side: decks, mixer and track loading.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use cuepoint_core::Track;
use cuepoint_engine::{AudioOutput, Command, DECKS, EngineHandle, EngineState, Pcm};
use cuepoint_ui::{DeckView, LoadedTrack, MixerView};

/// Overview resolution; enough for a full-width waveform on large screens.
const OVERVIEW_BINS: usize = 2048;

struct Decoded {
    deck: usize,
    /// Matches `Player::load_ids[deck]` unless a newer load superseded it.
    id: u64,
    track: Track,
    result: Result<(Arc<Pcm>, Vec<f32>), String>,
}

pub struct Player {
    handle: EngineHandle,
    output: AudioOutput,
    pub decks: [DeckView; DECKS],
    pub mixer: MixerView,
    load_ids: [u64; DECKS],
    decoded_tx: Sender<Decoded>,
    decoded_rx: Receiver<Decoded>,
    commands: Vec<Command>,
}

impl Player {
    pub fn new() -> Self {
        let (handle, output) = cuepoint_engine::open();
        let (decoded_tx, decoded_rx) = channel();
        Self {
            handle,
            output,
            decks: std::array::from_fn(DeckView::new),
            mixer: MixerView::default(),
            load_ids: [0; DECKS],
            decoded_tx,
            decoded_rx,
            commands: Vec::new(),
        }
    }

    pub fn output_description(&self) -> &str {
        self.output.description()
    }

    pub fn output_is_silent(&self) -> bool {
        self.output.is_silent()
    }

    pub fn state(&mut self) -> EngineState {
        self.handle.state()
    }

    /// Chooses a deck for a load without an explicit target: the first empty
    /// one, else the first one not playing.
    pub fn free_deck(&mut self) -> Option<usize> {
        let state = self.handle.state();
        let empty = |deck: &usize| {
            self.decks[*deck].loaded.is_none() && self.decks[*deck].message.is_none()
        };
        (0..DECKS)
            .find(empty)
            .or_else(|| (0..DECKS).find(|&deck| !state.decks[deck].playing))
    }

    /// Starts decoding `track` for `deck` on a worker thread. Refuses while
    /// that deck is playing, like rekordbox's default "lock playing deck".
    pub fn load(&mut self, deck: usize, track: Track) -> Result<(), String> {
        if self.handle.state().decks[deck].playing {
            return Err(format!(
                "Deck {} is playing; pause it before loading.",
                deck + 1
            ));
        }
        let Some(path) = track.location.clone() else {
            return Err(format!("\"{}\" has no file location.", track.title));
        };
        self.load_ids[deck] += 1;
        let id = self.load_ids[deck];
        self.decks[deck].message = Some(format!("Loading {}…", track.title));
        let rate = self.handle.sample_rate();
        let tx = self.decoded_tx.clone();
        std::thread::Builder::new()
            .name(format!("cuepoint-load-{deck}"))
            .spawn(move || {
                let result = cuepoint_engine::decode_file(&path, rate)
                    .map(|pcm| {
                        let overview = pcm.overview(OVERVIEW_BINS);
                        (Arc::new(pcm), overview)
                    })
                    .map_err(|e| e.to_string());
                let _ = tx.send(Decoded {
                    deck,
                    id,
                    track,
                    result,
                });
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Hands finished decodes to the engine. Call once per frame.
    pub fn poll_loads(&mut self) {
        while let Ok(decoded) = self.decoded_rx.try_recv() {
            if decoded.id != self.load_ids[decoded.deck] {
                continue;
            }
            let view = &mut self.decks[decoded.deck];
            match decoded.result {
                Ok((pcm, overview)) => {
                    view.message = None;
                    view.loaded = Some(LoadedTrack {
                        track: decoded.track,
                        overview,
                    });
                    self.handle.send(Command::Load {
                        deck: decoded.deck,
                        pcm,
                    });
                }
                Err(error) => {
                    view.message = Some(format!(
                        "Could not load \"{}\": {error}",
                        decoded.track.title
                    ));
                }
            }
        }
    }

    pub fn show_deck(&mut self, ui: &mut egui::Ui, deck: usize, state: &EngineState) {
        self.decks[deck].show(
            ui,
            &state.decks[deck],
            state.sample_rate,
            &mut self.commands,
        );
        self.flush();
    }

    pub fn show_mixer(&mut self, ui: &mut egui::Ui, state: &EngineState) {
        self.mixer.show(ui, &state.mixer, &mut self.commands);
        self.flush();
    }

    fn flush(&mut self) {
        for command in self.commands.drain(..) {
            self.handle.send(command);
        }
    }
}
