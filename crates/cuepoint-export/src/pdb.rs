//! `export.pdb` writer: the DeviceSQL device library every Pioneer CDJ/XDJ
//! reads from USB and SD media.
//!
//! Spec: `docs/spec/subsystems/02-device-export-prodjlink.md` section 3.2.
//! The format is documented publicly by Deep Symmetry's "Rekordbox Export
//! Structure Analysis" and crate-digger's `rekordbox_pdb.ksy`; section and
//! field names in comments are theirs. Where the documents leave a value
//! open, we write what rekordbox itself writes in a fresh export.
//!
//! The file holds 20 tables in page type order, all present even when empty,
//! as rekordbox writes them. Types 0 to 8 are the library proper (tracks,
//! genres, artists, albums, labels, keys, colours, playlist tree, playlist
//! entries), 11 and 12 the player-recorded history playlists and entries,
//! 13 artwork, 16 the browse categories ("columns"), 17 and 18 two
//! undocumented tables that appear to configure the player's browse menu,
//! and 19 rekordbox's history sync data. Types 9, 10, 14 and 15 have no
//! known rows in device exports.

mod page;
mod row;
mod string;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::hash::Hash;

use crate::model::DeviceLibrary;
use page::Table;
use row::{Row, TrackLinks};

/// Page type of each table.
mod kind {
    pub const TRACKS: u32 = 0;
    pub const GENRES: u32 = 1;
    pub const ARTISTS: u32 = 2;
    pub const ALBUMS: u32 = 3;
    pub const LABELS: u32 = 4;
    pub const KEYS: u32 = 5;
    pub const COLOURS: u32 = 6;
    pub const PLAYLIST_TREE: u32 = 7;
    pub const PLAYLIST_ENTRIES: u32 = 8;
    pub const HISTORY_PLAYLISTS: u32 = 11;
    pub const HISTORY_ENTRIES: u32 = 12;
    pub const ARTWORK: u32 = 13;
    pub const COLUMNS: u32 = 16;
    pub const MENU: u32 = 17;
    pub const MENU_SORT: u32 = 18;
    pub const HISTORY: u32 = 19;
    /// Every table rekordbox writes, in its order.
    pub const ALL: std::ops::RangeInclusive<u32> = 0..=19;
}

/// The fixed colour labels: ids 1 to 8 select the colour players show,
/// whatever the name.
const COLOURS: [&str; 8] = [
    "Pink", "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple",
];

/// Browse categories in the columns table: id, an unknown code and the name,
/// as rekordbox writes them in every export.
const COLUMNS: [(u16, u16, &str); 27] = [
    (1, 0x80, "GENRE"),
    (2, 0x81, "ARTIST"),
    (3, 0x82, "ALBUM"),
    (4, 0x83, "TRACK"),
    (5, 0x85, "BPM"),
    (6, 0x86, "RATING"),
    (7, 0x87, "YEAR"),
    (8, 0x88, "REMIXER"),
    (9, 0x89, "LABEL"),
    (10, 0x8a, "ORIGINAL ARTIST"),
    (11, 0x8b, "KEY"),
    (12, 0x8d, "CUE"),
    (13, 0x8e, "COLOR"),
    (14, 0x92, "TIME"),
    (15, 0x93, "BITRATE"),
    (16, 0x94, "FILE NAME"),
    (17, 0x84, "PLAYLIST"),
    (18, 0x98, "HOT CUE BANK"),
    (19, 0x95, "HISTORY"),
    (20, 0x91, "SEARCH"),
    (21, 0x96, "COMMENTS"),
    (22, 0x8c, "DATE ADDED"),
    (23, 0x97, "DJ PLAY COUNT"),
    (24, 0x90, "FOLDER"),
    (25, 0xa1, "DEFAULT"),
    (26, 0xa2, "ALPHABET"),
    (27, 0xaa, "MATCHING"),
];

/// Table 17 as rekordbox writes it in a fresh export. Not publicly
/// documented. The first value is a columns table id; the last appears to
/// be the position in the player's top browse menu (1 = ARTIST to
/// 10 = DATE ADDED, 0 = hidden), so players may build that menu from here.
const MENU: [[u16; 4]; 22] = [
    [1, 1, 0x163, 0],
    [5, 6, 0x105, 0],
    [6, 7, 0x163, 0],
    [7, 8, 0x163, 0],
    [8, 9, 0x163, 0],
    [9, 10, 0x163, 0],
    [10, 11, 0x163, 0],
    [13, 15, 0x163, 0],
    [14, 19, 0x104, 0],
    [15, 20, 0x106, 0],
    [16, 21, 0x163, 0],
    [18, 23, 0x163, 0],
    [2, 2, 0x2, 1],
    [3, 3, 0x3, 2],
    [4, 4, 0x1, 3],
    [11, 12, 0x63, 4],
    [17, 5, 0x63, 5],
    [19, 22, 0x63, 6],
    [20, 18, 0x63, 7],
    [27, 26, 0x263, 8],
    [24, 17, 0x63, 9],
    [22, 27, 0x63, 10],
];

/// Table 18 as rekordbox writes it in a fresh export. Not publicly
/// documented; its first values are columns table ids, so it is likely the
/// sort options offered for track lists.
const MENU_SORT: [[u16; 4]; 17] = [
    [1, 6, 1, 0],
    [21, 7, 1, 0],
    [14, 8, 1, 0],
    [8, 9, 1, 0],
    [9, 10, 1, 0],
    [10, 11, 1, 0],
    [15, 13, 1, 0],
    [13, 15, 1, 0],
    [23, 16, 1, 0],
    [22, 17, 1, 0],
    [25, 0, 0x100, 0],
    [26, 1, 0x200, 0],
    [2, 2, 0x300, 0],
    [3, 3, 0x400, 0],
    [5, 4, 0x500, 0],
    [6, 5, 0x600, 0],
    [11, 12, 0x700, 0],
];

/// Serialises a complete export.pdb.
///
/// Genres, artists (performers and remixers), albums (keyed by name and
/// artist, the track artist standing in for the album artist), labels and
/// keys are derived from the tracks, de-duplicated, with 1-based ids in
/// first-seen order; an empty name means no row and id 0. Colours are the 8
/// fixed rekordbox colour rows. Output depends only on `library`.
pub fn write_pdb(library: &DeviceLibrary) -> Vec<u8> {
    let mut genres = Interner::default();
    let mut artists = Interner::default();
    let mut albums = Interner::default();
    let mut labels = Interner::default();
    let mut keys = Interner::default();

    let track_rows: Vec<Row> = library
        .tracks
        .iter()
        .map(|track| {
            let genre = genres.name(&track.genre);
            let artist = artists.name(&track.artist);
            let remixer = artists.name(&track.remixer);
            let album = if track.album.is_empty() {
                0
            } else {
                albums.intern((track.album.clone(), artist))
            };
            let links = TrackLinks {
                artist,
                remixer,
                album,
                genre,
                label: labels.name(&track.label),
                key: track.key.map_or(0, |key| keys.name(&key.name())),
                colour: if (1..=8).contains(&track.colour) {
                    track.colour
                } else {
                    0
                },
            };
            row::track(track, &links)
        })
        .collect();

    let playlist_rows = library.playlists.iter().map(row::playlist_tree).collect();
    let entry_rows = library
        .playlists
        .iter()
        .filter(|playlist| !playlist.is_folder)
        .flat_map(|playlist| {
            (1..)
                .zip(&playlist.entries)
                .map(|(index, &track)| row::playlist_entry(index, track, playlist.id))
        })
        .collect();

    let mut rows_by_kind: HashMap<u32, Vec<Row>> = HashMap::from([
        (kind::TRACKS, track_rows),
        (kind::GENRES, genres.rows(|id, name| row::genre(id, name))),
        (
            kind::ARTISTS,
            artists.rows(|id, name| row::artist(id, name)),
        ),
        (
            kind::ALBUMS,
            albums.rows(|id, (name, artist)| row::album(id, *artist, name)),
        ),
        (kind::LABELS, labels.rows(|id, name| row::label(id, name))),
        (kind::KEYS, keys.rows(|id, name| row::key(id, name))),
        (
            kind::COLOURS,
            (1..)
                .zip(COLOURS)
                .map(|(id, name)| row::colour(id, name))
                .collect(),
        ),
        (kind::PLAYLIST_TREE, playlist_rows),
        (kind::PLAYLIST_ENTRIES, entry_rows),
        (
            kind::COLUMNS,
            COLUMNS
                .iter()
                .map(|&(id, code, name)| row::column(id, code, name))
                .collect(),
        ),
        (kind::MENU, MENU.into_iter().map(row::four_u16).collect()),
        (
            kind::MENU_SORT,
            MENU_SORT.into_iter().map(row::four_u16).collect(),
        ),
        // Players record history playlists and entries themselves.
        (kind::HISTORY_PLAYLISTS, Vec::new()),
        (kind::HISTORY_ENTRIES, Vec::new()),
        // Artwork rows need image paths, which the model does not carry.
        (kind::ARTWORK, Vec::new()),
        // rekordbox's own history sync rows are undocumented.
        (kind::HISTORY, Vec::new()),
    ]);

    let tables: Vec<Table> = kind::ALL
        .map(|kind| Table {
            kind,
            rows: rows_by_kind.remove(&kind).unwrap_or_default(),
        })
        .collect();
    page::write_file(&tables)
}

/// De-duplicates values, handing out 1-based ids in first-seen order.
struct Interner<K> {
    ids: HashMap<K, u32>,
    order: Vec<K>,
}

impl<K> Default for Interner<K> {
    fn default() -> Self {
        Self {
            ids: HashMap::new(),
            order: Vec::new(),
        }
    }
}

impl<K: Eq + Hash + Clone> Interner<K> {
    fn intern(&mut self, value: K) -> u32 {
        if let Some(&id) = self.ids.get(&value) {
            return id;
        }
        let id = u32::try_from(self.order.len() + 1).expect("fewer than 2^32 rows");
        self.ids.insert(value.clone(), id);
        self.order.push(value);
        id
    }

    /// One row per value, in id order.
    fn rows(&self, encode: impl Fn(u32, &K) -> Row) -> Vec<Row> {
        (1..)
            .zip(&self.order)
            .map(|(id, value)| encode(id, value))
            .collect()
    }
}

impl Interner<String> {
    /// The id for a name, or 0 (no row) when it is empty.
    fn name(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        match self.ids.get(name) {
            Some(&id) => id,
            None => self.intern(name.to_owned()),
        }
    }
}
