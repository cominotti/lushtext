// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem mutation helpers for rename, creation, and removal workflows.
//!
//! Callers use these commands to make destructive or namespace-changing intent
//! explicit without importing raw filesystem APIs.

use std::path::Path;

use super::sys;
use super::types::MutationOutcome;
/// Create one directory.
///
/// # Errors
///
/// Returns an error when the directory cannot be created.
pub fn create_dir(path: &Path) -> std::io::Result<()> {
    sys::create_dir(path)
}

/// Create a directory tree without adding durability guarantees.
///
/// # Errors
///
/// Returns an error when the directory tree cannot be created.
pub fn create_dir_all(path: &Path) -> std::io::Result<()> {
    sys::create_dir_all(path)
}

/// Remove a file, treating an already-absent path as a narrow outcome.
///
/// # Errors
///
/// Returns an error when removal fails for a reason other than not found.
pub fn remove_file_if_exists(path: &Path) -> std::io::Result<MutationOutcome> {
    match sys::remove_file(path) {
        Ok(()) => Ok(MutationOutcome::Changed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(MutationOutcome::AlreadyAbsent)
        }
        Err(error) => Err(error),
    }
}

/// Remove an empty directory, treating an already-absent path as a narrow outcome.
///
/// # Errors
///
/// Returns an error when removal fails for a reason other than not found.
pub fn remove_dir_if_exists(path: &Path) -> std::io::Result<MutationOutcome> {
    match sys::remove_dir(path) {
        Ok(()) => Ok(MutationOutcome::Changed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(MutationOutcome::AlreadyAbsent)
        }
        Err(error) => Err(error),
    }
}

/// Remove a directory tree, treating an already-absent path as a narrow outcome.
///
/// # Errors
///
/// Returns an error when removal fails for a reason other than not found.
pub fn remove_dir_all_if_exists(path: &Path) -> std::io::Result<MutationOutcome> {
    match sys::remove_dir_all(path) {
        Ok(()) => Ok(MutationOutcome::Changed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(MutationOutcome::AlreadyAbsent)
        }
        Err(error) => Err(error),
    }
}
