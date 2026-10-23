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
/// Longest string `export.pdb` stores, in UTF-16 units; a device path must
/// fit whole or the track row points at a file that does not exist.
const MAX_PATH_UNITS: usize = 255;
/// Artist and album folder names are capped so the file name keeps room.
const MAX_FOLDER_UNITS: usize = 64;

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
    /// Audio files skipped because an identical file was already there.
    pub already_present: usize,
}

/// Writes `tracks` and `playlists` (whose entries refer to positions in
/// `tracks`, 1-based) under `root`. Existing identical audio files are
/// kept, so re-exporting is fast. `progress` may return false to cancel.
///
/// Every file is written under a temporary name first and moved into place
/// only once the whole export is ready, so a cancelled or failed export
/// leaves the previous `export.pdb` and the files it refers to untouched.
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

    let mut staging = Staging::default();
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
        // Names depend on export order, so a same-named file may be another
        // track: only keep it when the bytes match.
        let same = same_contents(&track.source, &audio_dest).unwrap_or(false);
        if same {
            already_present += 1;
        } else {
            staging
                .copy(&track.source, &audio_dest)
                .map_err(io_error(&audio_dest))?;
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
        staging
            .write(&dat, &anlz::write_dat(&input))
            .map_err(io_error(&dat))?;
        staging
            .write(&ext, &anlz::write_ext(&input))
            .map_err(io_error(&ext))?;
    }

    if !progress(Progress::WritingLibrary) {
        return Err(ExportError::Cancelled);
    }
    let library = DeviceLibrary {
        tracks: tracks.into_iter().map(|t| t.row).collect(),
        playlists,
    };
    let pdb_path = library_dir.join("export.pdb");
    staging
        .write(&pdb_path, &pdb::write_pdb(&library))
        .map_err(io_error(&pdb_path))?;
    staging
        .commit()
        .map_err(|(path, source)| ExportError::Io { path, source })?;
    Ok(ExportReport {
        tracks: library.tracks.len(),
        playlists: library.playlists.len(),
        already_present,
    })
}

/// `/Contents/<artist>/<album>/<file>` with names made safe for FAT and
/// exFAT, de-duplicated against paths already used in this export.
/// The whole path stays within `MAX_PATH_UNITS` UTF-16 units.
fn unique_device_path(row: &ExportTrack, source: &Path, used: &mut HashSet<String>) -> String {
    let folder = |name: &str, fallback: &str| {
        let name = sanitise(name).unwrap_or_else(|| fallback.into());
        let short = truncate_units(&name, MAX_FOLDER_UNITS).trim_end_matches(['.', ' ']);
        if short.is_empty() {
            fallback.to_owned()
        } else {
            short.to_owned()
        }
    };
    let artist = folder(&row.artist, "UnknownArtist");
    let album = folder(&row.album, "UnknownAlbum");
    let dir = format!("/Contents/{artist}/{album}/");
    let name_units = MAX_PATH_UNITS - units(&dir);
    let file = source
        .file_name()
        .and_then(|f| sanitise(&f.to_string_lossy()))
        .unwrap_or_else(|| "track".into());
    let (stem, ext) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (file.clone(), String::new()),
    };
    for n in 1.. {
        let suffix = if n == 1 {
            ext.clone()
        } else {
            format!(" ({n}){ext}")
        };
        let room = name_units.saturating_sub(units(&suffix));
        let stem = truncate(truncate_units(&stem, room), MAX_NAME_BYTES - suffix.len());
        let path = format!("{dir}{stem}{suffix}");
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

fn units(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The longest prefix of `s` within `max` UTF-16 units.
fn truncate_units(s: &str, max: usize) -> &str {
    let mut count = 0;
    for (index, c) in s.char_indices() {
        count += c.len_utf16();
        if count > max {
            return &s[..index];
        }
    }
    s
}

/// Whether two files hold the same bytes.
fn same_contents(a: &Path, b: &Path) -> io::Result<bool> {
    use std::io::Read;
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
    let (mut buf_a, mut buf_b) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let n = a.read(&mut buf_a)?;
        if n == 0 {
            return Ok(true);
        }
        b.read_exact(&mut buf_b[..n])?;
        if buf_a[..n] != buf_b[..n] {
            return Ok(false);
        }
    }
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

/// Files written under temporary names beside their destinations, waiting
/// to be moved into place together. Dropping it without committing deletes
/// the temporary files.
#[derive(Default)]
struct Staging {
    /// (temporary, destination) pairs, in the order they were written.
    staged: Vec<(PathBuf, PathBuf)>,
}

impl Staging {
    /// A short temporary name beside `dest`, so it fits wherever `dest` does.
    fn temp_for(&mut self, dest: &Path) -> PathBuf {
        let temp = dest.with_file_name(format!(".cuepoint-{}.tmp", self.staged.len() + 1));
        self.staged.push((temp.clone(), dest.to_owned()));
        temp
    }

    fn write(&mut self, dest: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(self.temp_for(dest), bytes)
    }

    fn copy(&mut self, from: &Path, dest: &Path) -> io::Result<()> {
        fs::copy(from, self.temp_for(dest)).map(drop)
    }

    /// Moves every staged file into place, in order. A file being replaced
    /// is set aside first; if any move fails, everything moved so far is
    /// put back as it was.
    fn commit(mut self) -> Result<(), (PathBuf, io::Error)> {
        let staged = std::mem::take(&mut self.staged);
        // (destination, where its previous file was set aside)
        let mut moved: Vec<(&Path, Option<PathBuf>)> = Vec::new();
        let mut result = Ok(());
        for (temp, dest) in &staged {
            let set_aside = match fs::symlink_metadata(dest) {
                Ok(_) => {
                    let old = temp.with_extension("old");
                    match fs::rename(dest, &old) {
                        Ok(()) => Some(old),
                        Err(e) => {
                            result = Err((dest.clone(), e));
                            break;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                Err(e) => {
                    result = Err((dest.clone(), e));
                    break;
                }
            };
            moved.push((dest, set_aside));
            if let Err(e) = fs::rename(temp, dest) {
                result = Err((dest.clone(), e));
                break;
            }
        }
        if result.is_err() {
            for (dest, set_aside) in moved.iter().rev() {
                let _ = match set_aside {
                    Some(old) => fs::rename(old, dest),
                    None => fs::remove_file(dest),
                };
            }
        } else {
            for old in moved.into_iter().filter_map(|(_, old)| old) {
                let _ = fs::remove_file(old);
            }
        }
        // Whatever was not moved into place is removed.
        self.staged = staged;
        result
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        for (temp, _) in &self.staged {
            let _ = fs::remove_file(temp);
        }
    }
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

    #[test]
    fn device_paths_fit_the_pdb_string_limit() {
        let mut used = HashSet::new();
        let row = ExportTrack {
            artist: "A".repeat(100),
            album: "Ä".repeat(100),
            ..ExportTrack::default()
        };
        let source = PathBuf::from(format!("/m/{}.flac", "t".repeat(84)));
        let first = unique_device_path(&row, &source, &mut used);
        let second = unique_device_path(&row, &source, &mut used);
        for path in [&first, &second] {
            assert!(units(path) <= MAX_PATH_UNITS, "{} units", units(path));
            assert!(path.ends_with(".flac"));
        }
        assert!(second.ends_with(" (2).flac"));
        assert_ne!(first.to_lowercase(), second.to_lowercase());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cuepoint-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn staged_names_fit_beside_the_longest_destination() {
        let dir = temp_dir("staging-long");
        let source = dir.join("source.mp3");
        fs::write(&source, b"audio").unwrap();
        // 253 bytes: valid, but too long once given a longer extension.
        let dest = dir.join(format!("{}.mp3", "曲".repeat(83)));
        let mut staging = Staging::default();
        staging.copy(&source, &dest).unwrap();
        staging.commit().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"audio");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2, "no leftovers");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_commit_puts_replaced_files_back() {
        let dir = temp_dir("staging-rollback");
        let (kept, added) = (dir.join("kept"), dir.join("sub").join("added"));
        fs::write(&kept, b"old").unwrap();
        fs::create_dir_all(added.parent().unwrap()).unwrap();
        let mut staging = Staging::default();
        staging.write(&kept, b"new").unwrap();
        staging.write(&added, b"new").unwrap();
        // The second move fails because its folder has gone.
        fs::remove_dir_all(added.parent().unwrap()).unwrap();
        assert!(staging.commit().is_err());
        assert_eq!(fs::read(&kept).unwrap(), b"old");
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["kept"], "temporary and set-aside files removed");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn same_size_different_audio_is_not_kept() {
        let dir = std::env::temp_dir().join(format!("cuepoint-same-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (a, b, c) = (dir.join("a"), dir.join("b"), dir.join("c"));
        fs::write(&a, [1u8; 70_000]).unwrap();
        fs::write(&b, [1u8; 70_000]).unwrap();
        let mut other = vec![1u8; 70_000];
        other[69_999] = 2;
        fs::write(&c, other).unwrap();
        assert!(same_contents(&a, &b).unwrap());
        assert!(!same_contents(&a, &c).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
