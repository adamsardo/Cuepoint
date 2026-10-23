//! Track list previews: a small 3-band overview per track, made off the UI
//! thread by a worker the app supplies and kept on disk between runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// Bins per preview: one per device pixel of rekordbox's 165 pt preview.
pub const PREVIEW_BINS: usize = 330;
const CACHE_VERSION: u8 = 1;

/// Mean low, mid and high level per bin, 0..=255.
pub type Preview = Vec<[u8; 3]>;

/// Computes a preview for an audio file; `None` when it cannot be read.
pub type PreviewWorker = dyn Fn(&Path) -> Option<Preview> + Send + Sync;

enum Entry {
    Pending,
    Missing,
    Ready(Preview),
}

pub struct PreviewCache {
    requests: Sender<PathBuf>,
    results: Receiver<(PathBuf, Option<Preview>)>,
    entries: HashMap<PathBuf, Entry>,
}

impl PreviewCache {
    /// Starts `threads` workers running `worker`, caching under `cache_dir`.
    pub fn new(worker: Arc<PreviewWorker>, cache_dir: Option<PathBuf>, threads: usize) -> Self {
        let (requests, jobs) = channel::<PathBuf>();
        let (done, results) = channel();
        let jobs = Arc::new(Mutex::new(jobs));
        for i in 0..threads.max(1) {
            let (jobs, done, worker, cache_dir) = (
                jobs.clone(),
                done.clone(),
                worker.clone(),
                cache_dir.clone(),
            );
            std::thread::Builder::new()
                .name(format!("cuepoint-preview-{i}"))
                .spawn(move || {
                    // Hold the lock only while waiting for the next job.
                    let next = || jobs.lock().ok()?.recv().ok();
                    while let Some(path) = next() {
                        let file = cache_dir.as_deref().and_then(|dir| cache_file(dir, &path));
                        let preview = file.as_deref().and_then(read_cached).or_else(|| {
                            // A panic counts as no preview and keeps the
                            // worker alive for the rest of the queue.
                            let preview =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    worker(&path)
                                }))
                                .ok()
                                .flatten()?;
                            if let Some(file) = &file {
                                write_cached(file, &preview);
                            }
                            Some(preview)
                        });
                        if done.send((path, preview)).is_err() {
                            break;
                        }
                    }
                })
                .expect("spawn preview thread");
        }
        Self {
            requests,
            results,
            entries: HashMap::new(),
        }
    }

    /// Collects finished previews. Call once per frame.
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        while let Ok((path, preview)) = self.results.try_recv() {
            let entry = preview.map_or(Entry::Missing, Entry::Ready);
            self.entries.insert(path, entry);
            any = true;
        }
        any
    }

    /// The preview for `path` if ready; otherwise starts making it.
    pub fn get(&mut self, path: &Path) -> Option<&Preview> {
        if !self.entries.contains_key(path) {
            self.entries.insert(path.to_owned(), Entry::Pending);
            let _ = self.requests.send(path.to_owned());
        }
        match self.entries.get(path) {
            Some(Entry::Ready(preview)) => Some(preview),
            _ => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.entries.values().any(|e| matches!(e, Entry::Pending))
    }
}

/// Reduces 3-band levels to [`PREVIEW_BINS`] mean levels.
pub fn preview_from_bands(low: &[u8], mid: &[u8], high: &[u8]) -> Preview {
    let points = low.len().min(mid.len()).min(high.len());
    if points == 0 {
        return Vec::new();
    }
    (0..PREVIEW_BINS)
        .map(|bin| {
            let start = bin * points / PREVIEW_BINS;
            let end = ((bin + 1) * points / PREVIEW_BINS)
                .max(start + 1)
                .min(points);
            let mean = |band: &[u8]| {
                let sum: u32 = band[start..end].iter().map(|&v| u32::from(v)).sum();
                (sum / (end - start) as u32) as u8
            };
            [mean(low), mean(mid), mean(high)]
        })
        .collect()
}

/// The cache file for `audio`, named by a hash of its path, size and
/// modification time so an edited file is analysed again.
fn cache_file(dir: &Path, audio: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(audio).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    audio.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    meta.modified().ok()?.hash(&mut hasher);
    Some(dir.join(format!("{:016x}.preview", hasher.finish())))
}

fn read_cached(file: &Path) -> Option<Preview> {
    let bytes = std::fs::read(file).ok()?;
    let (&version, body) = bytes.split_first()?;
    if version != CACHE_VERSION || body.len() % 3 != 0 {
        return None;
    }
    Some(body.as_chunks::<3>().0.to_vec())
}

fn write_cached(file: &Path, preview: &Preview) {
    let mut bytes = Vec::with_capacity(1 + preview.len() * 3);
    bytes.push(CACHE_VERSION);
    bytes.extend(preview.iter().flatten());
    if let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        let temp = file.with_extension("tmp");
        if std::fs::write(&temp, bytes).is_ok() {
            let _ = std::fs::rename(temp, file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_bins_are_means() {
        let low: Vec<u8> = (0..660).map(|i| if i % 2 == 0 { 10 } else { 30 }).collect();
        let preview = preview_from_bands(&low, &low, &low);
        assert_eq!(preview.len(), PREVIEW_BINS);
        assert!(preview.iter().all(|bin| *bin == [20, 20, 20]));
        assert!(preview_from_bands(&[], &[], &[]).is_empty());
    }

    #[test]
    fn cache_round_trips() {
        let dir = std::env::temp_dir().join(format!("cuepoint-preview-{}", std::process::id()));
        let file = dir.join("x.preview");
        let preview = vec![[1, 2, 3], [4, 5, 6]];
        write_cached(&file, &preview);
        assert_eq!(read_cached(&file), Some(preview));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
