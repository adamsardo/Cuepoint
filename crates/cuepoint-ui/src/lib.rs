//! egui widgets and panels: decks, waveforms, browser, mixer, preferences.

mod artwork;
mod browser;
mod deck;
pub mod disk_cache;
mod export_view;
mod jobs;
mod mixer;
mod paint;
mod preview;
pub mod theme;
mod waveform;

pub use artwork::ArtworkCache;
pub use browser::{Browser, BrowserAction, ListView, Source};
pub use deck::{DeckView, LoadedTrack};
pub use export_view::{
    DIVIDER, ExportView, FOOTER, Mode, PLAYER, SideTab, TITLE_BAR, TOP_BAR, footer, title_bar,
    top_bar,
};
pub use mixer::MixerView;
pub use preview::{PREVIEW_BINS, Preview, PreviewCache, PreviewWorker, preview_from_bands};
pub use waveform::{BandWaveform, detail_waveform};
