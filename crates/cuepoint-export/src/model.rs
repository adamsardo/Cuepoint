//! What an export writes, independent of file format: one row per track and
//! playlist, with every value already in device form (device paths, ids,
//! hundredths of a BPM). The `pdb` and `anlz` writers only serialise these.

use cuepoint_analysis::ColourWaveform;
use cuepoint_core::{BeatGrid, MusicalKey};

/// Audio container, as players classify it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FileType {
    Mp3,
    M4a,
    Flac,
    Wav,
    Aiff,
    #[default]
    Unknown,
}

impl FileType {
    /// From a file extension, case-insensitive.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" => Self::Mp3,
            "m4a" | "mp4" | "aac" | "alac" => Self::M4a,
            "flac" => Self::Flac,
            "wav" | "wave" => Self::Wav,
            "aif" | "aiff" => Self::Aiff,
            _ => Self::Unknown,
        }
    }
}

/// One track's library row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportTrack {
    /// 1-based, unique within the export.
    pub id: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub comment: String,
    pub key: Option<MusicalKey>,
    /// Tempo in hundredths of a BPM, 0 when unknown.
    pub bpm_centi: u32,
    pub duration_secs: u32,
    /// 0 when unknown.
    pub year: u16,
    /// 0 to 5 stars.
    pub rating: u8,
    /// rekordbox colour label index 1 to 8 (pink, red, orange, yellow,
    /// green, aqua, blue, purple), or 0 for none.
    pub colour: u8,
    pub sample_rate: u32,
    pub bit_rate_kbps: u32,
    pub bit_depth: u16,
    pub file_size: u64,
    pub file_type: FileType,
    /// Audio file path on the device, from its root, e.g.
    /// `/Contents/Artist/Album/Track.mp3`.
    pub device_path: String,
    /// `.DAT` analysis file path on the device, e.g.
    /// `/PIONEER/USBANLZ/P016/0000875E/ANLZ0000.DAT`; the `.EXT` sits
    /// beside it.
    pub analysis_path: String,
    /// `YYYY-MM-DD`.
    pub date_added: String,
    /// 0 when the track has no artwork.
    pub artwork_id: u32,
}

/// A playlist or folder in the device's playlist tree.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportPlaylist {
    /// 1-based, unique within the export.
    pub id: u32,
    /// 0 for top level.
    pub parent_id: u32,
    pub name: String,
    pub is_folder: bool,
    /// Position among its siblings, from 0.
    pub sort_order: u32,
    /// Track ids in play order (empty for folders).
    pub entries: Vec<u32>,
}

/// Everything `export.pdb` holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceLibrary {
    pub tracks: Vec<ExportTrack>,
    pub playlists: Vec<ExportPlaylist>,
}

/// Inputs for one track's `.DAT` and `.EXT` analysis files.
#[derive(Debug, Clone, Copy)]
pub struct AnalysisInput<'a> {
    /// Same as `ExportTrack::device_path`.
    pub device_path: &'a str,
    pub grid: &'a BeatGrid,
    pub duration_secs: f64,
    /// 3-band peaks at 150 points per second, from `cuepoint-analysis`.
    pub waveform: &'a ColourWaveform,
}
