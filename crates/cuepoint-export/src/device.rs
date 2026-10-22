//! Writes a complete export onto a device root: audio under `Contents`,
//! analysis files under `PIONEER/USBANLZ`, and `PIONEER/rekordbox/export.pdb`
//! (spec 02, section 3.1).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cuepoint_analysis::ColourWaveform;
use cuepoint_core::BeatGrid;

use crate::{AnalysisInput, DeviceLibrary, ExportPlaylist, ExportTrack, anlz, pdb};

/// Longest file or folder name most removable file systems accept.
const MAX_NAME_BYTES: usize = 255;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("export cancelled")]
    Cancelled,
}

/// A track ready to export: its library row (ids and device paths are
/// filled in by [`write_device`]), its source file and its analysis.
#[derive(Debug, Clone)]
pub struct PreparedTrack {
    pub row: ExportTrack,
    pub source: PathBuf,
    pub grid: BeatGrid,
    pub waveform: ColourWaveform,
    pub duration_secs: f64,
}

/// Progress through an export, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Copying audio and writing analysis for track `done + 1` of `total`.
    Track {
        done: usize,
        total: usize,
    },
    WritingLibrary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub tracks: usize,
    pub playlists: usize,
    /// Audio files skipped because an identical-size file was already there.
    pub already_present: usize,
}

/// Writes `tracks` and `playlists` (whose entries refer to positions in
/// `tracks`, 1-based) under `root`. Existing audio files of the same size
/// are kept, so re-exporting is fast. `progress` may return false to cancel.
pub fn write_device(
    root: &Path,
    mut tracks: Vec<PreparedTrack>,
    playlists: Vec<ExportPlaylist>,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<ExportReport, ExportError> {
    let io_error = |path: &Path| {
        let path = path.to_owned();
        move |source| ExportError::Io { path, source }
    };
    let pioneer = root.join("PIONEER");
    let library_dir = pioneer.join("rekordbox");
    fs::create_dir_all(&library_dir).map_err(io_error(&library_dir))?;

    let mut used_paths = HashSet::new();
    let mut used_analysis = HashSet::new();
    let total = tracks.len();
    let mut already_present = 0;
    for (index, track) in tracks.iter_mut().enumerate() {
        if !progress(Progress::Track { done: index, total }) {
            return Err(ExportError::Cancelled);
        }
        track.row.id = u32::try_from(index + 1).unwrap_or(u32::MAX);
        track.row.device_path = unique_device_path(&track.row, &track.source, &mut used_paths);
        track.row.analysis_path = unique_analysis_path(&track.row.device_path, &mut used_analysis);

        let audio_dest = on_device(root, &track.row.device_path);
        if let Some(parent) = audio_dest.parent() {
            fs::create_dir_all(parent).map_err(io_error(parent))?;
        }
        let source_len = fs::metadata(&track.source)
            .map_err(io_error(&track.source))?
            .len();
        let same = fs::metadata(&audio_dest).is_ok_and(|m| m.len() == source_len);
        if same {
            already_present += 1;
        } else {
            copy_atomically(&track.source, &audio_dest).map_err(io_error(&audio_dest))?;
        }
        track.row.file_size = source_len;

        let input = AnalysisInput {
            device_path: &track.row.device_path,
            grid: &track.grid,
            duration_secs: track.duration_secs,
            waveform: &track.waveform,
        };
        let dat = on_device(root, &track.row.analysis_path);
        let ext = dat.with_extension("EXT");
        if let Some(parent) = dat.parent() {
            fs::create_dir_all(parent).map_err(io_error(parent))?;
        }
        write_atomically(&dat, &anlz::write_dat(&input)).map_err(io_error(&dat))?;
        write_atomically(&ext, &anlz::write_ext(&input)).map_err(io_error(&ext))?;
    }

    if !progress(Progress::WritingLibrary) {
        return Err(ExportError::Cancelled);
    }
    let library = DeviceLibrary {
        tracks: tracks.into_iter().map(|t| t.row).collect(),
        playlists,
    };
    let pdb_path = library_dir.join("export.pdb");
    write_atomically(&pdb_path, &pdb::write_pdb(&library)).map_err(io_error(&pdb_path))?;
    Ok(ExportReport {
        tracks: library.tracks.len(),
        playlists: library.playlists.len(),
        already_present,
    })
}

/// `/Contents/<artist>/<album>/<file>` with names made safe for FAT and
/// exFAT, de-duplicated against paths already used in this export.
fn unique_device_path(row: &ExportTrack, source: &Path, used: &mut HashSet<String>) -> String {
    let artist = sanitise(&row.artist).unwrap_or_else(|| "UnknownArtist".into());
    let album = sanitise(&row.album).unwrap_or_else(|| "UnknownAlbum".into());
    let file = source
        .file_name()
        .and_then(|f| sanitise(&f.to_string_lossy()))
        .unwrap_or_else(|| "track".into());
    let (stem, ext) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (file.clone(), String::new()),
    };
    for n in 1.. {
        let name = if n == 1 {
            file.clone()
        } else {
            let suffix = format!(" ({n}){ext}");
            format!("{}{suffix}", truncate(&stem, MAX_NAME_BYTES - suffix.len()))
        };
        let path = format!("/Contents/{artist}/{album}/{name}");
        // Case-insensitive file systems treat these as the same file.
        if used.insert(path.to_lowercase()) {
            return path;
        }
    }
    unreachable!("an unused name always exists")
}

/// The analysis folder is a hash of the audio path, so two tracks can very
/// rarely land in the same folder; re-hash with a counter until free.
fn unique_analysis_path(device_path: &str, used: &mut HashSet<String>) -> String {
    let mut path = anlz::analysis_path(device_path);
    let mut n = 1;
    while !used.insert(path.to_lowercase()) {
        n += 1;
        path = anlz::analysis_path(&format!("{device_path}#{n}"));
    }
    path
}

/// A name safe on FAT32/exFAT/HFS+/APFS: no path separators, reserved
/// characters or control characters, no trailing dots or spaces, at most
/// 255 bytes. `None` if nothing usable is left.
fn sanitise(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    let trimmed = truncate(trimmed, MAX_NAME_BYTES).trim_end_matches(['.', ' ']);
    (!trimmed.is_empty() && trimmed != "." && trimmed != "..").then(|| trimmed.to_owned())
}

/// The longest prefix of `s` within `max` bytes, on a character boundary.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn on_device(root: &Path, device_path: &str) -> PathBuf {
    device_path
        .trim_start_matches('/')
        .split('/')
        .fold(root.to_owned(), |path, part| path.join(part))
}

/// Writes via a temporary file and rename, so a pulled drive never holds a
/// half-written file under the real name.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("cuepoint-tmp");
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)
}

fn copy_atomically(from: &Path, to: &Path) -> io::Result<()> {
    let temp = to.with_extension("cuepoint-tmp");
    fs::copy(from, &temp)?;
    fs::rename(&temp, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitises_names_for_removable_media() {
        assert_eq!(sanitise("AC/DC: Live?").as_deref(), Some("AC_DC_ Live_"));
        assert_eq!(
            sanitise("  trailing dots... ").as_deref(),
            Some("trailing dots")
        );
        assert_eq!(sanitise("..."), None);
        assert_eq!(sanitise(""), None);
        let long = "é".repeat(200);
        let cut = sanitise(&long).unwrap();
        assert!(cut.len() <= MAX_NAME_BYTES && cut.chars().all(|c| c == 'é'));
    }

    #[test]
    fn device_paths_are_unique_ignoring_case() {
        let mut used = HashSet::new();
        let row = ExportTrack {
            artist: "Artist".into(),
            ..ExportTrack::default()
        };
        let a = unique_device_path(&row, Path::new("/m/Track.mp3"), &mut used);
        let b = unique_device_path(&row, Path::new("/other/track.MP3"), &mut used);
        let c = unique_device_path(&row, Path::new("/x/Track.mp3"), &mut used);
        assert_eq!(a, "/Contents/Artist/UnknownAlbum/Track.mp3");
        assert_eq!(b, "/Contents/Artist/UnknownAlbum/track (2).MP3");
        assert_eq!(c, "/Contents/Artist/UnknownAlbum/Track (3).mp3");
    }
}
