//! Row layouts ("Table Rows" in the Deep Symmetry export analysis). All
//! integers are little-endian. String offsets count from the start of the
//! row, and rekordbox places a row's strings straight after its fixed part.

use super::string;
use crate::model::{ExportPlaylist, ExportTrack, FileType};

/// One encoded row, ready to place in a page heap.
pub(super) struct Row {
    /// Padded to a multiple of 4: every row in a rekordbox export starts on
    /// a 4-byte boundary of the heap.
    pub bytes: Vec<u8>,
    /// Bytes 2..4 hold the field the analysis calls `index_shift`. In real
    /// exports it is always 0x20 times the row's position within its page,
    /// so the page writer fills it in when it places the row.
    pub has_index_shift: bool,
}

/// Builds one row.
#[derive(Default)]
struct RowWriter {
    buf: Vec<u8>,
}

impl RowWriter {
    fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn set_u16(&mut self, at: usize, value: u16) {
        self.buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// Appends a string and returns its offset from the row start.
    fn string(&mut self, text: &str) -> usize {
        if string::is_long(text) {
            self.buf.resize(self.buf.len().next_multiple_of(4), 0);
        }
        let at = self.buf.len();
        string::encode(text, &mut self.buf);
        at
    }

    fn finish(mut self, has_index_shift: bool) -> Row {
        self.buf.resize(self.buf.len().next_multiple_of(4), 0);
        Row {
            bytes: self.buf,
            has_index_shift,
        }
    }
}

/// Row ids a track links to, 0 for none.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct TrackLinks {
    pub artist: u32,
    pub remixer: u32,
    pub album: u32,
    pub genre: u32,
    pub label: u32,
    pub key: u32,
    pub colour: u8,
}

/// Fixed part of a track row: fields up to 0x5e, then 21 `u16` string
/// offsets.
const TRACK_STRINGS_AT: usize = 0x5e;
const TRACK_STRING_COUNT: usize = 21;

/// Track row ("Track Rows"). Unknown fields hold what rekordbox writes for a
/// freshly exported track.
pub(super) fn track(track: &ExportTrack, links: &TrackLinks) -> Row {
    let mut w = RowWriter::default();
    // Subtype 0x24: bit 0x04 means 16-bit string offsets, which track rows
    // always use.
    w.u16(0x24);
    w.u16(0); // index_shift
    // `bitmask`: meaning unknown, 0x000c0700 in every rekordbox export seen.
    w.u32(0x000c_0700);
    w.u32(track.sample_rate);
    w.u32(0); // composer_id
    w.u32(u32::try_from(track.file_size).unwrap_or(u32::MAX));
    // Three unknowns (`u2` to `u4`) that vary between rekordbox exports with
    // no known meaning; 0 is the neutral choice.
    w.u32(0);
    w.u16(0);
    w.u16(0);
    // artwork_id: the model has no artwork path to put in the artwork table,
    // and an id with no artwork row would dangle, so none.
    w.u32(0);
    w.u32(links.key);
    w.u32(0); // original_artist_id
    w.u32(links.label);
    w.u32(links.remixer);
    w.u32(track.bit_rate_kbps);
    w.u32(0); // track_number: not in the model
    w.u32(track.bpm_centi);
    w.u32(links.genre);
    w.u32(links.album);
    w.u32(links.artist);
    w.u32(track.id);
    w.u16(0); // disc_number
    w.u16(0); // play_count
    w.u16(track.year);
    w.u16(track.bit_depth);
    w.u16(u16::try_from(track.duration_secs).unwrap_or(u16::MAX));
    w.u16(0x29); // `u5`: always 0x29
    w.u8(links.colour);
    w.u8(track.rating.min(5));
    w.u16(file_type_code(track.file_type));
    // The 3 that precedes string offsets in every row type, two bytes wide
    // here because track rows use 16-bit offsets.
    w.u16(3);
    debug_assert_eq!(w.buf.len(), TRACK_STRINGS_AT);
    w.buf.resize(TRACK_STRINGS_AT + 2 * TRACK_STRING_COUNT, 0);

    let file_name = track.device_path.rsplit('/').next().unwrap_or_default();
    // Index order of the documented string table ("Track offset strings").
    let strings: [&str; TRACK_STRING_COUNT] = [
        "", // 0: ISRC
        "", // 1: lyricist
        // 2 and 3: undocumented ASCII numbers that rekordbox sets to "1" on
        // a track's first export (and seemingly bumps on re-export).
        "1",
        "1",
        "",   // 4: unknown, usually empty
        "",   // 5: message
        "",   // 6: publish track information ("ON" or empty)
        "ON", // 7: autoload hot cues, rekordbox's default
        "",   // 8: unknown
        "",   // 9: unknown, usually empty
        &track.date_added,
        "", // 11: release date: the model only has the year
        "", // 12: mix name
        "", // 13: unknown, usually empty
        &track.analysis_path,
        "", // 15: analysis date: not in the model
        &track.comment,
        &track.title,
        "", // 18: unknown, usually empty
        file_name,
        &track.device_path,
    ];
    for (index, text) in strings.into_iter().enumerate() {
        let at = w.string(text);
        let at = u16::try_from(at).expect("clamped strings keep a track row inside a page");
        w.set_u16(TRACK_STRINGS_AT + 2 * index, at);
    }
    w.finish(true)
}

/// The `file_type` codes players use.
fn file_type_code(file_type: FileType) -> u16 {
    match file_type {
        FileType::Unknown => 0x00,
        FileType::Mp3 => 0x01,
        FileType::M4a => 0x04,
        FileType::Flac => 0x05,
        FileType::Wav => 0x0b,
        FileType::Aiff => 0x0c,
    }
}

/// Appends the string-offset tail shared by artist and album rows: the
/// constant 3, then a one-byte offset to the name, which follows directly.
/// The near (8-bit offset) subtype always suffices because the name sits
/// right after the row's fixed part.
fn near_name(mut w: RowWriter, name: &str) -> Row {
    w.u8(3);
    let offset_at = w.buf.len();
    w.u8(0);
    let at = w.string(name);
    w.buf[offset_at] = u8::try_from(at).expect("the name follows the fixed part");
    w.finish(true)
}

/// Artist row, subtype 0x60 ("Artist row with nearby name").
pub(super) fn artist(id: u32, name: &str) -> Row {
    let mut w = RowWriter::default();
    w.u16(0x60);
    w.u16(0); // index_shift
    w.u32(id);
    near_name(w, name)
}

/// Album row, subtype 0x80 ("Album row with nearby name").
pub(super) fn album(id: u32, artist_id: u32, name: &str) -> Row {
    let mut w = RowWriter::default();
    w.u16(0x80);
    w.u16(0); // index_shift
    w.u32(0);
    w.u32(artist_id);
    w.u32(id);
    w.u32(0);
    near_name(w, name)
}

/// Genre row; label rows share the layout.
pub(super) fn genre(id: u32, name: &str) -> Row {
    let mut w = RowWriter::default();
    w.u32(id);
    w.string(name);
    w.finish(false)
}

pub(super) fn label(id: u32, name: &str) -> Row {
    genre(id, name)
}

/// Key row: the id twice, then the name.
pub(super) fn key(id: u32, name: &str) -> Row {
    let mut w = RowWriter::default();
    w.u32(id);
    w.u32(id);
    w.string(name);
    w.finish(false)
}

/// Colour row: four unknown zero bytes, then the id as one byte and again as
/// a `u16` at bytes 5..7 (rekordbox repeats it so), a zero byte and the name
/// at byte 8.
pub(super) fn colour(id: u8, name: &str) -> Row {
    let mut w = RowWriter::default();
    w.u32(0);
    w.u8(id);
    w.u16(u16::from(id));
    w.u8(0);
    w.string(name);
    w.finish(false)
}

/// Playlist tree row: parent, unknown zero, sort order, id, folder flag and
/// the name at byte 0x14.
pub(super) fn playlist_tree(playlist: &ExportPlaylist) -> Row {
    let mut w = RowWriter::default();
    w.u32(playlist.parent_id);
    w.u32(0);
    w.u32(playlist.sort_order);
    w.u32(playlist.id);
    w.u32(u32::from(playlist.is_folder));
    w.string(&playlist.name);
    w.finish(false)
}

/// Playlist entry row. `entry_index` counts from 1 in rekordbox exports.
pub(super) fn playlist_entry(entry_index: u32, track_id: u32, playlist_id: u32) -> Row {
    let mut w = RowWriter::default();
    w.u32(entry_index);
    w.u32(track_id);
    w.u32(playlist_id);
    w.finish(false)
}

/// Columns table row: id, a code whose meaning is unknown, and the
/// category name, which rekordbox stores as UTF-16 wrapped in the
/// interlinear annotation characters U+FFFA and U+FFFB.
pub(super) fn column(id: u16, code: u16, name: &str) -> Row {
    let mut w = RowWriter::default();
    w.u16(id);
    w.u16(code);
    w.string(&format!("\u{fffa}{name}\u{fffb}"));
    w.finish(false)
}

/// A row of four `u16` values, the layout of the undocumented tables 17
/// and 18.
pub(super) fn four_u16(values: [u16; 4]) -> Row {
    let mut w = RowWriter::default();
    for value in values {
        w.u16(value);
    }
    w.finish(false)
}
