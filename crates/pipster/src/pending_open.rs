//! Paths queued from Finder Open With (Apple Events) and window file drops
//! (winit `DroppedFile`), drained on the Slint timer.

use parking_lot::Mutex;
use std::path::PathBuf;

static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub fn queue(paths: impl IntoIterator<Item = PathBuf>) {
    PENDING.lock().extend(paths);
}

pub fn take() -> Vec<PathBuf> {
    std::mem::take(&mut *PENDING.lock())
}
