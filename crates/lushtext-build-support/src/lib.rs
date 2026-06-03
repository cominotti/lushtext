// SPDX-License-Identifier: GPL-3.0-or-later

//! Build-time helper boundary for LushText Cargo build scripts.
//!
//! Runtime code cannot be used from a crate's own `build.rs`, so build scripts
//! get their own tiny filesystem adapter instead of preserving raw `std::fs`
//! calls in generated-code setup.

pub mod filesystem {
    //! Filesystem helpers for Cargo build scripts.
    //!
    //! This module is the build-script-only counterpart to
    //! `services::filesystem`. It intentionally exposes only the operations
    //! needed while generating resources and widget-test registries.

    use std::path::{Path, PathBuf};

    /// Return whether a build input exists.
    #[must_use]
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    /// Read a directory into paths for build-script discovery.
    ///
    /// # Errors
    ///
    /// Returns an error when Cargo's build-script process cannot read the
    /// directory or one of its entries.
    pub fn read_dir_paths(path: &Path) -> std::io::Result<Vec<PathBuf>> {
        std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect()
    }

    /// Read a UTF-8 source file used by a build script.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read as a string.
    pub fn read_to_string(path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    /// Write generated build output.
    ///
    /// # Errors
    ///
    /// Returns an error when Cargo's output directory cannot be written.
    pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        std::fs::write(path, contents)
    }
}
