//! Album artwork from audio files' tags, decoded off the UI thread and kept
//! as small textures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::jobs::PathQueue;

/// Thumbnails are this many pixels square: the 79 pt artwork column at 2x.
const THUMBNAIL: u32 = 160;
/// Textures kept at most, about 50 MB; the least recently drawn go first
/// and load again if they come back into view.
const MAX_TEXTURES: usize = 512;

enum Entry {
    Pending,
    Missing,
    Ready(TextureHandle),
}

struct Slot {
    entry: Entry,
    /// The frame this path was last asked for.
    used: u64,
}

pub struct ArtworkCache {
    queue: Arc<PathQueue>,
    results: Receiver<(PathBuf, Option<ColorImage>)>,
    entries: HashMap<PathBuf, Slot>,
    frame: u64,
}

impl Default for ArtworkCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ArtworkCache {
    fn drop(&mut self) {
        self.queue.close();
    }
}

impl ArtworkCache {
    pub fn new() -> Self {
        let queue = Arc::new(PathQueue::default());
        let (done, results) = channel();
        let jobs = queue.clone();
        std::thread::Builder::new()
            .name("cuepoint-artwork".into())
            .spawn(move || {
                while let Some(path) = jobs.next() {
                    // One bad file must not stop artwork for the rest.
                    let image = std::panic::catch_unwind(|| thumbnail(&path)).ok().flatten();
                    if done.send((path, image)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn artwork thread");
        Self {
            queue,
            results,
            entries: HashMap::new(),
            frame: 0,
        }
    }

    /// Collects finished thumbnails, withdraws requests for rows no longer
    /// drawn and evicts old textures. Call once per frame, before `get`.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        while let Ok((path, image)) = self.results.try_recv() {
            let entry = match image {
                Some(image) => Entry::Ready(ctx.load_texture(
                    format!("artwork:{}", path.display()),
                    image,
                    TextureOptions::LINEAR,
                )),
                None => Entry::Missing,
            };
            let used = self.entries.get(&path).map_or(self.frame, |s| s.used);
            self.entries.insert(path, Slot { entry, used });
        }
        let (entries, frame) = (&self.entries, self.frame);
        for path in self
            .queue
            .withdraw(|path| entries.get(path).is_some_and(|s| s.used + 1 >= frame))
        {
            self.entries.remove(&path);
        }
        let mut ready: Vec<(u64, PathBuf)> = self
            .entries
            .iter()
            .filter(|(_, s)| matches!(s.entry, Entry::Ready(_)))
            .map(|(path, s)| (s.used, path.clone()))
            .collect();
        if ready.len() > MAX_TEXTURES {
            ready.sort_unstable_by_key(|(used, _)| *used);
            for (_, path) in &ready[..ready.len() - MAX_TEXTURES] {
                self.entries.remove(path);
            }
        }
    }

    /// The artwork for `path` if it is ready; otherwise starts loading it.
    pub fn get(&mut self, path: &Path) -> Option<&TextureHandle> {
        let frame = self.frame;
        let slot = self.entries.entry(path.to_owned()).or_insert_with(|| {
            self.queue.push(path.to_owned());
            Slot {
                entry: Entry::Pending,
                used: frame,
            }
        });
        slot.used = frame;
        match &slot.entry {
            Entry::Ready(texture) => Some(texture),
            _ => None,
        }
    }

    /// True while any requested artwork is still decoding.
    pub fn is_pending(&self) -> bool {
        self.entries
            .values()
            .any(|s| matches!(s.entry, Entry::Pending))
    }

    /// True once `path` is known to have no artwork.
    pub fn is_missing(&self, path: &Path) -> bool {
        self.entries
            .get(path)
            .is_some_and(|s| matches!(s.entry, Entry::Missing))
    }
}

fn thumbnail(path: &Path) -> Option<ColorImage> {
    let bytes = cuepoint_engine::read_artwork(path)?;
    let image = image::load_from_memory(&bytes).ok()?;
    let rgba = image
        .resize_to_fill(THUMBNAIL, THUMBNAIL, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}
