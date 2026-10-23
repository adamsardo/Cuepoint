//! Owns the audio engine on the app side: decks, mixer and track loading.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use cuepoint_analysis::AnalysisSettings;
use cuepoint_core::{MusicalKey, Track};
use cuepoint_engine::{AudioOutput, Command, DECKS, EngineHandle, EngineState, Pcm};
use cuepoint_ui::{BandWaveform, DeckView, ExportView, LoadedTrack, MixerView};

use crate::analysis_cache;

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
/// library has it (rekordbox's own grid and key), so imported grids match,
/// and then only the waveform is computed. Resampling to the engine rate
/// and the analysis run in parallel on the decoded audio.
fn prepare(
    track: Track,
    rate: u32,
    cache: Option<&Path>,
) -> Result<(Arc<Pcm>, LoadedTrack), String> {
    let path = track.location.clone().ok_or("no file location")?;
    let started = std::time::Instant::now();
    let native = cuepoint_engine::decode_native(&path).map_err(|e| e.to_string())?;
    let decoded = started.elapsed();
    let library_key = MusicalKey::parse(&track.key);
    let needs_analysis = track.beatgrid.is_empty() || library_key.is_none();
    // A cached analysis is enough when it has what the library lacks.
    let cached = cache
        .and_then(|dir| analysis_cache::read(dir, &path))
        .filter(|c| !needs_analysis || (!c.grid.is_empty() && c.key.is_some()));
    let from_cache = cached.is_some();
    let (resampled, (grid, key, waveform)) = std::thread::scope(|scope| {
        let analysis = scope.spawn(|| {
            if let Some(c) = cached {
                return (Some(c.grid), c.key, c.waveform);
            }
            let (samples, source_rate) = (native.samples(), native.sample_rate());
            let result = if needs_analysis {
                let a =
                    cuepoint_analysis::analyze(samples, source_rate, &AnalysisSettings::default());
                (Some(a.grid), a.key, a.waveform)
            } else {
                (
                    None,
                    None,
                    cuepoint_analysis::colour_waveform(samples, source_rate),
                )
            };
            if let Some(dir) = cache {
                let entry = analysis_cache::Cached {
                    grid: result.0.clone().unwrap_or_default(),
                    key: result.1,
                    waveform: result.2.clone(),
                };
                analysis_cache::write(dir, &path, &entry);
            }
            result
        });
        let resampled = cuepoint_engine::resampled(&native, rate);
        (
            resampled,
            analysis.join().expect("analysis thread panicked"),
        )
    });
    let pcm = resampled.map_err(|e| e.to_string())?.unwrap_or(native);
    if timing() {
        eprintln!(
            "[timing] {}: decode {:.0} ms, then resample to {rate} Hz with {} {:.0} ms",
            track.title,
            decoded.as_secs_f64() * 1e3,
            match (from_cache, needs_analysis) {
                (true, _) => "cached analysis",
                (false, true) => "full analysis",
                (false, false) => "waveform",
            },
            (started.elapsed() - decoded).as_secs_f64() * 1e3
        );
    }
    let grid = if track.beatgrid.is_empty() {
        grid.unwrap_or_default()
    } else {
        track.beatgrid.clone()
    };
    let key = library_key.or(key);
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

/// `CUEPOINT_DEV_TIMING` prints load timings to stderr.
fn timing() -> bool {
    std::env::var_os("CUEPOINT_DEV_TIMING").is_some()
}

pub struct Player {
    handle: EngineHandle,
    output: AudioOutput,
    pub decks: [DeckView; DECKS],
    pub mixer: MixerView,
    load_ids: [u64; DECKS],
    /// Where analysis results are kept between loads.
    cache: Option<PathBuf>,
    /// When each deck's current load was requested, for timing.
    load_started: [Option<std::time::Instant>; DECKS],
    /// Decoded tracks sent to the engine, waiting for it to accept or refuse.
    awaiting: [Option<(u64, LoadedTrack)>; DECKS],
    decoded_tx: Sender<Decoded>,
    decoded_rx: Receiver<Decoded>,
    commands: Vec<Command>,
}

impl Player {
    pub fn new(cache: Option<PathBuf>) -> Self {
        let (handle, output) = cuepoint_engine::open();
        let (decoded_tx, decoded_rx) = channel();
        Self {
            handle,
            output,
            decks: std::array::from_fn(DeckView::new),
            mixer: MixerView::default(),
            load_ids: [0; DECKS],
            cache,
            load_started: [None; DECKS],
            awaiting: Default::default(),
            decoded_tx,
            decoded_rx,
            commands: Vec::new(),
        }
    }

    /// True while a load is decoding or waiting for the engine, or a deck
    /// is playing: the UI must keep redrawing.
    pub fn is_busy(&self, state: &EngineState) -> bool {
        self.decks.iter().any(|d| d.loading.is_some()) || state.decks.iter().any(|d| d.playing)
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
            let view = &self.decks[*deck];
            view.loaded.is_none() && view.loading.is_none() && view.message.is_none()
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
            return Err(if track.kind == cuepoint_library::xml::STREAMING_KIND {
                format!(
                    "\"{}\" is a streaming track; Cuepoint can only load local files.",
                    track.title
                )
            } else {
                format!("\"{}\" has no file location.", track.title)
            });
        }
        self.load_ids[deck] += 1;
        let id = self.load_ids[deck];
        self.decks[deck].loading = Some(track.title.clone());
        self.decks[deck].message = None;
        self.load_started[deck] = Some(std::time::Instant::now());
        let rate = self.handle.sample_rate();
        let tx = self.decoded_tx.clone();
        let cache = self.cache.clone();
        std::thread::Builder::new()
            .name(format!("cuepoint-load-{deck}"))
            .spawn(move || {
                let title = track.title.clone();
                // A panic in decoding or analysis must still end the load,
                // or the deck would show it loading forever.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    prepare(track, rate, cache.as_deref())
                }))
                .unwrap_or_else(|_| Err("decoding or analysis crashed".into()));
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
                        self.decks[deck].loading = None;
                        self.decks[deck].message = Some("Audio engine is not responding.".into());
                    }
                }
                Err(error) => {
                    self.decks[deck].loading = None;
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
            // A newer load may have been requested since this one was sent.
            let superseded = *serial != self.load_ids[deck];
            if deck_state.load_serial == *serial {
                let (_, loaded) = slot.take().expect("checked above");
                // The engine now holds this track, so show it as loaded, but
                // keep the newer load's progress showing until it arrives.
                if superseded {
                    self.decks[deck].loaded = Some(loaded);
                    continue;
                }
                if timing()
                    && let Some(started) = self.load_started[deck].take()
                {
                    eprintln!(
                        "[timing] load ready on deck {} after {:.0} ms",
                        deck + 1,
                        started.elapsed().as_secs_f64() * 1e3
                    );
                }
                self.decks[deck].loaded = Some(loaded);
                self.decks[deck].loading = None;
                self.decks[deck].message = None;
            } else if deck_state.rejected_serial == *serial {
                *slot = None;
                if superseded {
                    continue;
                }
                self.decks[deck].loading = None;
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
