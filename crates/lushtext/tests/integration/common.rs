// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared test infrastructure for LushText integration tests.
//!
//! The primary primitive is [`TestContext`], which creates an isolated temporary
//! directory with helpers for writing fixture files.

use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Isolated filesystem context for a single integration test.
///
/// Each context gets its own `TempDir` (auto-removed on drop) with a
/// simulated XDG data directory for workspace/session persistence.
pub struct TestContext {
    dir: TempDir,
    data_dir: PathBuf,
}

impl TestContext {
    /// Create a new isolated test context.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        let data_dir = dir.path().join("data/lushtext");
        std::fs::create_dir_all(&data_dir).expect("failed to create data dir");
        Self { dir, data_dir }
    }

    /// Root path of this test context.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Simulated `$XDG_DATA_HOME/lushtext` directory.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Write a file relative to context root; creates parent dirs.
    pub fn write_file(&self, rel: &str, content: &str) -> PathBuf {
        let full = self.dir.path().join(rel);
        if let Some(p) = full.parent() {
            std::fs::create_dir_all(p).expect("expected operation to succeed");
        }
        std::fs::write(&full, content).expect("expected operation to succeed");
        full
    }

    /// Create a directory relative to context root.
    pub fn mkdir(&self, rel: &str) -> PathBuf {
        let full = self.dir.path().join(rel);
        std::fs::create_dir_all(&full).expect("expected operation to succeed");
        full
    }
}
