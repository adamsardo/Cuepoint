//! Collection database: tracks, playlists, smart playlists, history, MyTags, import/export.
//!
//! Sources supported so far, all read-only:
//! - rekordbox XML (`DJ_PLAYLISTS`), see [`xml`].
//! - rekordbox 6/7 `master.db`, see [`master_db`] (feature `master-db`).

#[cfg(feature = "master-db")]
pub mod master_db;
mod search;
pub mod xml;

use std::collections::HashMap;
use std::path::PathBuf;

use cuepoint_core::{PlaylistNode, Track, TrackId};

pub use search::{SortColumn, matches_query, sort_rows, sort_tracks};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid rekordbox XML: {0}")]
    Xml(String),
    #[cfg(feature = "master-db")]
    #[error("the database key is wrong, or the file is not a rekordbox master.db")]
    WrongKey,
    #[cfg(feature = "master-db")]
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// An in-memory snapshot of a DJ library: every track plus the playlist tree.
#[derive(Debug, Clone, Default)]
pub struct Library {
    tracks: Vec<Track>,
    by_id: HashMap<TrackId, usize>,
    playlists: Vec<PlaylistNode>,
}

impl Library {
    pub fn new(tracks: Vec<Track>, playlists: Vec<PlaylistNode>) -> Self {
        let by_id = tracks
            .iter()
            .enumerate()
            .map(|(index, track)| (track.id.clone(), index))
            .collect();
        Self {
            tracks,
            by_id,
            playlists,
        }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn track(&self, id: &TrackId) -> Option<&Track> {
        self.track_index(id).map(|index| &self.tracks[index])
    }

    /// Position of a track in [`Library::tracks`].
    pub fn track_index(&self, id: &TrackId) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// Top-level playlist nodes, in display order.
    pub fn playlists(&self) -> &[PlaylistNode] {
        &self.playlists
    }

    pub fn find_playlist(&self, id: &str) -> Option<&PlaylistNode> {
        self.playlists.iter().find_map(|node| node.find(id))
    }

    /// Tracks of a playlist in play order. References to tracks missing
    /// from the collection are skipped.
    pub fn playlist_tracks<'a>(
        &'a self,
        node: &'a PlaylistNode,
    ) -> impl Iterator<Item = &'a Track> {
        node.tracks.iter().filter_map(|id| self.track(id))
    }
}
