//! egui widgets and panels: decks, waveforms, browser, mixer, preferences.

mod browser;
mod deck;
mod mixer;

pub use browser::{Browser, BrowserAction, Source};
pub use deck::{DeckView, LoadedTrack};
pub use mixer::MixerView;
