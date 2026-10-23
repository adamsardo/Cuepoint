//! The work queue behind the artwork and preview caches. It serves the
//! newest request first, so rows on screen now come before rows scrolled
//! past, and lets a cache withdraw requests it no longer wants.

use std::path::PathBuf;
use std::sync::{Condvar, Mutex};

#[derive(Default)]
pub(crate) struct PathQueue {
    state: Mutex<State>,
    ready: Condvar,
}

#[derive(Default)]
struct State {
    paths: Vec<PathBuf>,
    closed: bool,
}

impl PathQueue {
    pub fn push(&self, path: PathBuf) {
        if let Ok(mut state) = self.state.lock() {
            state.paths.push(path);
            self.ready.notify_one();
        }
    }

    /// The newest waiting path, blocking until there is one; `None` once
    /// the queue is closed.
    pub fn next(&self) -> Option<PathBuf> {
        let mut state = self.state.lock().ok()?;
        loop {
            if state.closed {
                return None;
            }
            if let Some(path) = state.paths.pop() {
                return Some(path);
            }
            state = self.ready.wait(state).ok()?;
        }
    }

    /// Removes the waiting paths `keep` rejects and returns them.
    pub fn withdraw(&self, keep: impl Fn(&PathBuf) -> bool) -> Vec<PathBuf> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        let (kept, dropped) = std::mem::take(&mut state.paths)
            .into_iter()
            .partition(|path| keep(path));
        state.paths = kept;
        dropped
    }

    /// Stops the workers once they finish their current job.
    pub fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            self.ready.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_and_withdrawable() {
        let queue = PathQueue::default();
        for name in ["a", "b", "c"] {
            queue.push(PathBuf::from(name));
        }
        assert_eq!(
            queue.withdraw(|p| p != &PathBuf::from("b")),
            [PathBuf::from("b")]
        );
        assert_eq!(queue.next(), Some(PathBuf::from("c")));
        assert_eq!(queue.next(), Some(PathBuf::from("a")));
        queue.close();
        assert_eq!(queue.next(), None);
    }
}
