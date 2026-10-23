//! On-disk cache of a track's analysis (grid, key and 3-band waveform), so
//! loading it again skips the analysis. Entries are keyed by the file's
//! path, size and modification time, like the list previews.

use std::path::{Path, PathBuf};

use cuepoint_analysis::ColourWaveform;
use cuepoint_core::{BeatGrid, MusicalKey, TempoMarker};
use cuepoint_ui::disk_cache;

const VERSION: u8 = 1;
/// The cache's size on disk, at most: a few thousand tracks' worth.
const MAX_BYTES: u64 = 1 << 30;

/// What a load needs from analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Cached {
    pub grid: BeatGrid,
    pub key: Option<MusicalKey>,
    pub waveform: ColourWaveform,
}

pub fn read(dir: &Path, audio: &Path) -> Option<Cached> {
    let file = file(dir, audio)?;
    let cached = decode(&std::fs::read(&file).ok()?)?;
    disk_cache::touch(&file);
    Some(cached)
}

/// Writes `entry`, then trims the cache to [`MAX_BYTES`], dropping the
/// least recently used entries.
pub fn write(dir: &Path, audio: &Path, entry: &Cached) {
    let Some(file) = file(dir, audio) else { return };
    if std::fs::create_dir_all(dir).is_ok() {
        let temp = file.with_extension("tmp");
        if std::fs::write(&temp, encode(entry)).is_ok() {
            let _ = std::fs::rename(temp, file);
        }
        disk_cache::trim_cache_dir(dir, "analysis", MAX_BYTES);
    }
}

fn file(dir: &Path, audio: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(audio).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    audio.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    meta.modified().ok()?.hash(&mut hasher);
    Some(dir.join(format!("{:016x}.analysis", hasher.finish())))
}

/// Layout: version, key (tonic, minor; 0xFF when unknown), marker count and
/// markers (time f64, bpm f64, beat in bar u8), points per second, point
/// count, then the low, mid and high bands. Little-endian.
fn encode(entry: &Cached) -> Vec<u8> {
    let mut out = vec![VERSION];
    match entry.key {
        Some(key) => out.extend([key.tonic(), u8::from(key.is_minor())]),
        None => out.extend([0xFF, 0]),
    }
    let markers = entry.grid.markers();
    out.extend((markers.len() as u32).to_le_bytes());
    for m in markers {
        out.extend(m.time.to_le_bytes());
        out.extend(m.bpm.to_le_bytes());
        out.push(m.beat_in_bar as u8);
    }
    let w = &entry.waveform;
    out.extend(w.points_per_second.to_le_bytes());
    out.extend((w.len() as u32).to_le_bytes());
    for band in [&w.low, &w.mid, &w.high] {
        out.extend(band.iter());
    }
    out
}

fn decode(bytes: &[u8]) -> Option<Cached> {
    let mut at = 0;
    let mut take = |n: usize| -> Option<&[u8]> {
        let slice = bytes.get(at..at + n)?;
        at += n;
        Some(slice)
    };
    if take(1)?[0] != VERSION {
        return None;
    }
    let key = match take(2)? {
        [0xFF, _] => None,
        [tonic, minor] => Some(MusicalKey::new(*tonic, *minor != 0)),
        _ => return None,
    };
    let u32_at = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap_or_default());
    let f64_at = |b: &[u8]| f64::from_le_bytes(b.try_into().unwrap_or_default());
    let count = u32_at(take(4)?) as usize;
    let mut markers = Vec::with_capacity(count.min(10_000));
    for _ in 0..count {
        markers.push(TempoMarker {
            time: f64_at(take(8)?),
            bpm: f64_at(take(8)?),
            beat_in_bar: u32::from(take(1)?[0]),
        });
    }
    let points_per_second = u32_at(take(4)?);
    let points = u32_at(take(4)?) as usize;
    let low = take(points)?.to_vec();
    let mid = take(points)?.to_vec();
    let high = take(points)?.to_vec();
    Some(Cached {
        grid: BeatGrid::from_markers(markers),
        key,
        waveform: ColourWaveform {
            points_per_second,
            low,
            mid,
            high,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        let entry = Cached {
            grid: BeatGrid::constant(126.0, 0.05, 1),
            key: Some(MusicalKey::new(9, true)),
            waveform: ColourWaveform {
                points_per_second: 150,
                low: vec![1, 2, 3],
                mid: vec![4, 5, 6],
                high: vec![7, 8, 9],
            },
        };
        assert_eq!(decode(&encode(&entry)), Some(entry.clone()));
        let unkeyed = Cached { key: None, ..entry };
        assert_eq!(decode(&encode(&unkeyed)), Some(unkeyed.clone()));
        let mut truncated = encode(&unkeyed);
        truncated.pop();
        assert_eq!(decode(&truncated), None);
    }
}
