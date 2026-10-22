//! egui widgets and panels: decks, waveforms, browser, mixer, preferences.

mod browser;
mod deck;
mod mixer;
mod waveform;

pub use browser::{Browser, BrowserAction, Source};
pub use deck::{DeckView, LoadedTrack};
pub use mixer::MixerView;
pub use waveform::{BandWaveform, detail_waveform};
