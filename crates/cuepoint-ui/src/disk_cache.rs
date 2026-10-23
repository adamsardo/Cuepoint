//! Size limits for the on-disk caches (track analysis and list previews).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Deletes the least recently used files with `extension` in `dir` until
/// they total at most `max_bytes`. Use is the modification time, which
/// [`touch`] refreshes on each cache hit.
pub fn trim_cache_dir(dir: &Path, extension: &str, max_bytes: u64) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = read
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != extension {
                return None;
            }
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), path))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_unstable_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// Marks a cache file as just used, so trimming keeps it.
pub fn touch(file: &Path) {
    if let Ok(f) = std::fs::File::options().write(true).open(file) {
        let _ = f.set_modified(SystemTime::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_oldest_first() {
        let dir = std::env::temp_dir().join(format!("cuepoint-trim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let file = dir.join(format!("{name}.analysis"));
            std::fs::write(&file, [0u8; 10]).unwrap();
            let f = std::fs::File::options().write(true).open(&file).unwrap();
            f.set_modified(old + std::time::Duration::from_secs(i as u64))
                .unwrap();
        }
        std::fs::write(dir.join("other.preview"), [0u8; 100]).unwrap();
        trim_cache_dir(&dir, "analysis", 20);
        assert!(!dir.join("a.analysis").exists());
        assert!(dir.join("b.analysis").exists() && dir.join("c.analysis").exists());
        assert!(dir.join("other.preview").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
