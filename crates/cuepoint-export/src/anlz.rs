//! ANLZ analysis files: the `.DAT` read by every player and the `.EXT` read
//! by nexus 2 and later (spec 02, section 3.4).
//!
//! Format sources: Deep Symmetry's "Analysis Files" page (djl-analysis) and
//! crate-digger's `rekordbox_anlz.ksy`. Every integer is big-endian. A file
//! is a `PMAI` header followed by tagged sections. Each section starts with
//! a twelve-byte envelope: its four-character tag, `len_header` (bytes of
//! section header, counting the envelope) and `len_tag` (bytes of the whole
//! section). The section header holds counts and constants; the content
//! (path text, beats, waveform entries) follows it.
//!
//! Where the documents call a value unknown, we write the value seen in
//! real rekordbox exports (the sample USB export in rekordcrate's test data,
//! inspected as data only), noted at each use, so that players meet nothing
//! unfamiliar.

mod waveform;

use crate::AnalysisInput;
use cuepoint_core::{BEATS_PER_BAR, BeatGrid};
use waveform::{
    Peaks, colour_detail_entry, colour_preview_entry, columns, detail_len, detail_peaks,
    mono_entry, tiny_entry,
};

/// `PWAV` columns.
const PREVIEW_COLUMNS: usize = 400;
/// `PWV2` columns.
const TINY_PREVIEW_COLUMNS: usize = 100;
/// `PWV4` columns.
const COLOUR_PREVIEW_COLUMNS: usize = 1200;

/// `PCOB`/`PCO2` list type: memory points and loops.
const MEMORY_CUES: u32 = 0;
/// `PCOB`/`PCO2` list type: hot cues and hot loops.
const HOT_CUES: u32 = 1;

/// `.DAT` file: `PPTH`, `PVBR`, `PQTZ`, `PWAV`, `PWV2`, then `PCOB` hot cues
/// and `PCOB` memory cues (both empty), in the order rekordbox writes them.
pub fn write_dat(input: &AnalysisInput) -> Vec<u8> {
    let detail = detail_peaks(input.waveform, detail_len(input.duration_secs));
    let mut file = AnlzWriter::new();
    file.path(input.device_path);
    file.vbr();
    file.beat_grid(input.grid, input.duration_secs);
    file.preview(*b"PWAV", &columns(&detail, PREVIEW_COLUMNS), mono_entry);
    file.preview(
        *b"PWV2",
        &columns(&detail, TINY_PREVIEW_COLUMNS),
        tiny_entry,
    );
    file.cue_list(HOT_CUES);
    file.cue_list(MEMORY_CUES);
    file.finish()
}

/// `.EXT` file: `PPTH`, `PWV3`, empty `PCOB` and `PCO2` lists (hot cues then
/// memory cues), `PWV5`, `PWV4`, in the order rekordbox writes them.
/// rekordbox repeats the legacy `PCOB` lists here, so we do too. It also
/// writes `PQT2` (extended beat grid, undocumented) and `PSSI` (phrases,
/// which need phrase analysis); we write neither.
pub fn write_ext(input: &AnalysisInput) -> Vec<u8> {
    let detail = detail_peaks(input.waveform, detail_len(input.duration_secs));
    let mut file = AnlzWriter::new();
    file.path(input.device_path);
    file.monochrome_detail(&detail);
    file.cue_list(HOT_CUES);
    file.cue_list(MEMORY_CUES);
    file.extended_cue_list(HOT_CUES);
    file.extended_cue_list(MEMORY_CUES);
    file.colour_detail(&detail);
    file.colour_preview(&columns(&detail, COLOUR_PREVIEW_COLUMNS));
    file.finish()
}

/// Device path for a track's analysis files,
/// `/PIONEER/USBANLZ/Pxxx/xxxxxxxx/ANLZ0000.DAT` (the `.EXT` sits beside it
/// with the other extension).
///
/// rekordbox derives the two folder names from a hash it does not document.
/// Players never recompute it: they read the path from the track's PDB row.
/// So any stable scheme works. Ours hashes the UTF-8 audio path with 64-bit
/// FNV-1a, mixes the result with MurmurHash3's 64-bit finaliser so every
/// output bit depends on every input bit, and spends 12 bits on `Pxxx` and
/// 32 bits on the second folder, both in upper-case hex like rekordbox's.
/// With 44 bits, two tracks in a 10,000-track export share a folder with
/// probability about 3 in a million; the exporter should still check for a
/// clash before writing.
pub fn analysis_path(device_audio_path: &str) -> String {
    let hash = mix64(fnv1a64(device_audio_path.as_bytes()));
    let outer = hash >> 52;
    let inner = (hash >> 20) & 0xFFFF_FFFF;
    format!("/PIONEER/USBANLZ/P{outer:03X}/{inner:08X}/ANLZ0000.DAT")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// MurmurHash3's `fmix64` avalanche step.
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    x ^ (x >> 33)
}

/// Byte count as a `u32` length field.
fn len32(len: usize) -> u32 {
    u32::try_from(len).expect("ANLZ sections are far smaller than 4 GiB")
}

/// Builds one ANLZ file section by section.
struct AnlzWriter {
    bytes: Vec<u8>,
}

impl AnlzWriter {
    /// `PMAI` file header: tag, `len_header` (always `1c`), `len_file`
    /// (patched in `finish`), then 16 bytes the documents leave unexplained.
    /// rekordbox's exports hold `00000001 00010000 00010000 00000000` there.
    const HEADER_LEN: u32 = 0x1c;
    const HEADER_TAIL: [u32; 4] = [1, 0x0001_0000, 0x0001_0000, 0];

    fn new() -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PMAI");
        bytes.extend_from_slice(&Self::HEADER_LEN.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        for word in Self::HEADER_TAIL {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        Self { bytes }
    }

    fn finish(mut self) -> Vec<u8> {
        let len = len32(self.bytes.len()).to_be_bytes();
        self.bytes[8..12].copy_from_slice(&len);
        self.bytes
    }

    /// Appends a section. `header` is the section header after the twelve
    /// byte envelope, as big-endian words; `content` follows it.
    fn section(&mut self, tag: [u8; 4], header: &[u32], content: &[u8]) {
        let len_header = 12 + 4 * header.len();
        let len_tag = len_header + content.len();
        self.bytes.reserve(len_tag);
        self.bytes.extend_from_slice(&tag);
        self.bytes
            .extend_from_slice(&len32(len_header).to_be_bytes());
        self.bytes.extend_from_slice(&len32(len_tag).to_be_bytes());
        for word in header {
            self.bytes.extend_from_slice(&word.to_be_bytes());
        }
        self.bytes.extend_from_slice(content);
    }

    /// `PPTH`: `len_path`, then the path as UTF-16BE with a trailing NUL;
    /// `len_path` counts the NUL.
    fn path(&mut self, path: &str) {
        let text: Vec<u8> = path
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_be_bytes)
            .collect();
        self.section(*b"PPTH", &[len32(text.len())], &text);
    }

    /// `PVBR`: one unknown word in the header, then the seek index. The
    /// documents do not explain the index. All zeros is what rekordbox
    /// writes for constant-bit-rate files, and is acceptable for any file:
    /// players then seek without the index. We write rekordbox's size,
    /// 401 words; the `.ksy` says 400, but rekordbox's sections are 1,620
    /// bytes long, which is 401 after the 16 header bytes.
    fn vbr(&mut self) {
        const INDEX_WORDS: usize = 401;
        self.section(*b"PVBR", &[0], &[0; 4 * INDEX_WORDS]);
    }

    /// `PQTZ`: header words unknown (0), unknown (`00080000`, as in the
    /// `.ksy` and rekordbox's files; the prose says `00800000`) and the beat
    /// count; then eight bytes per beat: beat in bar (1 to 4, `u16`), tempo
    /// in hundredths of a BPM (`u16`) and time in milliseconds (`u32`).
    fn beat_grid(&mut self, grid: &BeatGrid, duration_secs: f64) {
        let beats = if duration_secs.is_finite() && duration_secs > 0.0 {
            grid.beats_between(0.0, duration_secs)
        } else {
            Vec::new()
        };
        let mut content = Vec::with_capacity(8 * beats.len());
        for &(time, _) in &beats {
            let index = grid.beat_at(time).unwrap_or(0.0).round() as i64;
            let tempo = grid.bpm_at(time).unwrap_or(0.0) * 100.0;
            content.extend_from_slice(&beat_in_bar(grid, index).to_be_bytes());
            content.extend_from_slice(&(tempo.round() as u16).to_be_bytes());
            content.extend_from_slice(&((time * 1000.0).round() as u32).to_be_bytes());
        }
        self.section(*b"PQTZ", &[0, 0x0008_0000, len32(beats.len())], &content);
    }

    /// `PWAV` (400 columns) and `PWV2` (100 columns): `len_preview`, then a
    /// word the documents give as `00100000`; rekordbox's files hold
    /// `00010000`, which we follow.
    fn preview(&mut self, tag: [u8; 4], columns: &[Peaks], encode: fn(Peaks) -> u8) {
        let data: Vec<u8> = columns.iter().copied().map(encode).collect();
        self.section(tag, &[len32(data.len()), 0x0001_0000], &data);
    }

    /// `PCOB` with no cues: list type, a `u16` of 0 and a `u16` cue count
    /// (one word here), then `memory_count`, which rekordbox writes as
    /// `ffffffff` for an empty list.
    fn cue_list(&mut self, list_type: u32) {
        self.section(*b"PCOB", &[list_type, 0, 0xFFFF_FFFF], &[]);
    }

    /// `PCO2` with no cues: list type, then a `u16` cue count and a `u16`
    /// of 0 (one word here).
    fn extended_cue_list(&mut self, list_type: u32) {
        self.section(*b"PCO2", &[list_type, 0], &[]);
    }

    /// `PWV3`: bytes per entry (1), entry count and `00960000`, then one
    /// `PWAV`-style byte per 1/150 s.
    fn monochrome_detail(&mut self, detail: &[Peaks]) {
        let data: Vec<u8> = detail.iter().copied().map(mono_entry).collect();
        self.section(*b"PWV3", &[1, len32(data.len()), 0x0096_0000], &data);
    }

    /// `PWV5`: bytes per entry (2), entry count and `00960305`, then one
    /// packed colour entry per 1/150 s.
    fn colour_detail(&mut self, detail: &[Peaks]) {
        let data: Vec<u8> = detail
            .iter()
            .copied()
            .flat_map(colour_detail_entry)
            .collect();
        self.section(*b"PWV5", &[2, len32(detail.len()), 0x0096_0305], &data);
    }

    /// `PWV4`: bytes per entry (6), entry count (1,200) and an unknown word
    /// that rekordbox writes as 0, then six channel bytes per column.
    fn colour_preview(&mut self, columns: &[Peaks]) {
        let data: Vec<u8> = columns
            .iter()
            .copied()
            .flat_map(colour_preview_entry)
            .collect();
        self.section(*b"PWV4", &[6, len32(columns.len()), 0], &data);
    }
}

/// Position of whole beat `index` within its bar, 1 to 4: one more than the
/// number of beats since the latest downbeat.
fn beat_in_bar(grid: &BeatGrid, index: i64) -> u16 {
    (0..i64::from(BEATS_PER_BAR))
        .find(|&back| grid.is_downbeat(index - back))
        .map_or(1, |back| back as u16 + 1)
}

#[cfg(test)]
mod tests;
