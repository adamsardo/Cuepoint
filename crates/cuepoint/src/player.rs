//! Owns the audio engine on the app side: decks, mixer and track loading.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use cuepoint_analysis::AnalysisSettings;
use cuepoint_core::{MusicalKey, Track};
use cuepoint_engine::{AudioOutput, Command, DECKS, EngineHandle, EngineState, Pcm};
use cuepoint_ui::{BandWaveform, DeckView, ExportView, LoadedTrack, MixerView};

/// Overview resolution; enough for a full-width waveform on large screens.
const OVERVIEW_BINS: usize = 2048;

struct Decoded {
    deck: usize,
    /// Matches `Player::load_ids[deck]` unless a newer load superseded it.
    id: u64,
    title: String,
    result: Result<(Arc<Pcm>, LoadedTrack), String>,
}

/// Decodes and analyses a track. Library data wins over analysis where the
/// library has it (rekordbox's own grid and key), so imported grids match.
fn prepare(track: Track, rate: u32) -> Result<(Arc<Pcm>, LoadedTrack), String> {
    let path = track.location.clone().ok_or("no file location")?;
    let pcm = cuepoint_engine::decode_file(&path, rate).map_err(|e| e.to_string())?;
    let analysis = cuepoint_analysis::analyze(pcm.samples(), rate, &AnalysisSettings::default());
    let grid = if track.beatgrid.is_empty() {
        analysis.grid
    } else {
        track.beatgrid.clone()
    };
    let key = MusicalKey::parse(&track.key).or(analysis.key);
    let waveform = analysis.waveform;
    let loaded = LoadedTrack {
        overview: pcm.overview(OVERVIEW_BINS),
        bands: Some(BandWaveform {
            points_per_second: waveform.points_per_second,
            low: waveform.low,
            mid: waveform.mid,
            high: waveform.high,
        }),
        grid,
        key,
        track,
    };
    Ok((Arc::new(pcm), loaded))
}

pub struct Player {
    handle: EngineHandle,
    output: AudioOutput,
    pub decks: [DeckView; DECKS],
    pub mixer: MixerView,
    load_ids: [u64; DECKS],
    /// Decoded tracks sent to the engine, waiting for it to accept or refuse.
    awaiting: [Option<(u64, LoadedTrack)>; DECKS],
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
            awaiting: Default::default(),
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
        if track.location.is_none() {
            return Err(format!("\"{}\" has no file location.", track.title));
        }
        self.load_ids[deck] += 1;
        let id = self.load_ids[deck];
        self.decks[deck].message = Some(format!("Loading {}…", track.title));
        let rate = self.handle.sample_rate();
        let tx = self.decoded_tx.clone();
        std::thread::Builder::new()
            .name(format!("cuepoint-load-{deck}"))
            .spawn(move || {
                let title = track.title.clone();
                let result = prepare(track, rate);
                let _ = tx.send(Decoded {
                    deck,
                    id,
                    title,
                    result,
                });
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Hands finished decodes to the engine and applies its verdicts. Call
    /// once per frame.
    pub fn poll_loads(&mut self) {
        while let Ok(decoded) = self.decoded_rx.try_recv() {
            let deck = decoded.deck;
            if decoded.id != self.load_ids[deck] {
                continue;
            }
            match decoded.result {
                Ok((pcm, loaded)) => {
                    let grid = (!loaded.grid.is_empty()).then(|| Arc::new(loaded.grid.clone()));
                    let sent = self.handle.send(Command::Load {
                        deck,
                        pcm,
                        grid,
                        serial: decoded.id,
                    });
                    if sent {
                        self.awaiting[deck] = Some((decoded.id, loaded));
                    } else {
                        self.decks[deck].message = Some("Audio engine is not responding.".into());
                    }
                }
                Err(error) => {
                    self.decks[deck].message =
                        Some(format!("Could not load \"{}\": {error}", decoded.title));
                }
            }
        }
        // The engine re-checks the playing-deck lock when it applies a load,
        // since the deck may have started while the track was decoding.
        let state = self.handle.state();
        for (deck, slot) in self.awaiting.iter_mut().enumerate() {
            let Some((serial, _)) = slot else { continue };
            let deck_state = &state.decks[deck];
            if deck_state.load_serial == *serial {
                let (_, loaded) = slot.take().expect("checked above");
                self.decks[deck].loaded = Some(loaded);
                self.decks[deck].message = None;
            } else if deck_state.rejected_serial == *serial {
                *slot = None;
                self.decks[deck].message = Some(format!(
                    "Deck {} started playing, so the load was cancelled.",
                    deck + 1
                ));
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

    /// Both decks' scrolling detail waveforms, stacked.
    pub fn show_detail_waveforms(&self, ui: &mut egui::Ui, state: &EngineState) {
        for (deck, view) in self.decks.iter().enumerate() {
            cuepoint_ui::detail_waveform(
                ui,
                72.0,
                view.loaded.as_ref(),
                &state.decks[deck],
                state.sample_rate,
            );
        }
    }

    /// Export mode's single player, driving deck 1.
    pub fn show_export(
        &mut self,
        view: &mut ExportView,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        state: &EngineState,
        artwork: &mut cuepoint_ui::ArtworkCache,
    ) {
        view.player(
            ui,
            rect,
            &mut self.decks[0],
            &state.decks[0],
            state.sample_rate,
            &mut self.commands,
            artwork,
        );
        self.flush();
    }

    pub fn play_pause(&mut self, deck: usize) {
        self.handle.send(Command::PlayPause { deck });
    }

    pub fn seek(&mut self, deck: usize, frame: f64) {
        self.handle.send(Command::Seek { deck, frame });
    }

    /// The track on deck 1, for marking it in the browser.
    pub fn loaded_id(&self, deck: usize) -> Option<&cuepoint_core::TrackId> {
        self.decks[deck].loaded.as_ref().map(|l| &l.track.id)
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
