//! Album artwork from audio files' tags, decoded off the UI thread and kept
//! as small textures.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{ColorImage, TextureHandle, TextureOptions};

/// Thumbnails are this many pixels square: the 79 pt artwork column at 2x.
const THUMBNAIL: u32 = 160;

enum Entry {
    Pending,
    Missing,
    Ready(TextureHandle),
}

pub struct ArtworkCache {
    requests: Sender<PathBuf>,
    results: Receiver<(PathBuf, Option<ColorImage>)>,
    entries: HashMap<PathBuf, Entry>,
}

impl Default for ArtworkCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtworkCache {
    pub fn new() -> Self {
        let (requests, jobs) = channel::<PathBuf>();
        let (done, results) = channel();
        std::thread::Builder::new()
            .name("cuepoint-artwork".into())
            .spawn(move || {
                for path in jobs {
                    let image = thumbnail(&path);
                    if done.send((path, image)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn artwork thread");
        Self {
            requests,
            results,
            entries: HashMap::new(),
        }
    }

    /// Collects finished thumbnails. Call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((path, image)) = self.results.try_recv() {
            let entry = match image {
                Some(image) => Entry::Ready(ctx.load_texture(
                    format!("artwork:{}", path.display()),
                    image,
                    TextureOptions::LINEAR,
                )),
                None => Entry::Missing,
            };
            self.entries.insert(path, entry);
        }
    }

    /// The artwork for `path` if it is ready; otherwise starts loading it.
    pub fn get(&mut self, path: &Path) -> Option<&TextureHandle> {
        if !self.entries.contains_key(path) {
            self.entries.insert(path.to_owned(), Entry::Pending);
            let _ = self.requests.send(path.to_owned());
        }
        match self.entries.get(path) {
            Some(Entry::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// True once `path` is known to have no artwork.
    pub fn is_missing(&self, path: &Path) -> bool {
        matches!(self.entries.get(path), Some(Entry::Missing))
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
