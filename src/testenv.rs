//! Shared scaffolding for unit tests: a poison-recovering environment
//! lock and RAII temp directories (review item 16).
//!
//! Declared `#[cfg(test)]` in main.rs, so it exists only in test builds
//! and any module's tests can use it. The env lock is shared process-wide
//! on purpose: cargo runs test threads in parallel inside one process,
//! so every test that mutates the environment — here or in `storage/`,
//! `cli/`, anywhere — must hold the same lock.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static DIR_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Holds the process environment stable for the whole test body. A
/// panicking test poisons the mutex; the poison is recovered
/// deliberately — one failed test must not cascade into every other
/// env-touching test through `.lock().unwrap()`.
pub fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A temp directory that removes itself (recursively) when dropped —
/// tests were leaking `termocab-test-*` directories before.
pub struct TempDir(PathBuf);

impl TempDir {
    /// Creates a unique directory under the system temp dir. `tag` names
    /// the test in the path, which helps when one fails and the guard
    /// never ran.
    pub fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "termocab-test-{}-{tag}-{}",
            std::process::id(),
            DIR_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
