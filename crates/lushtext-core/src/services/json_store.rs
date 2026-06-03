// SPDX-License-Identifier: GPL-3.0-or-later

//! Generic JSON file persistence: load/save any serde type to a JSON file.

use crate::services::durable_write;
use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::Path;

/// Returns the application data directory (`$XDG_DATA_HOME/lushtext`).
///
/// Respects `LUSHTEXT_DATA_DIR` env var for test isolation — widget tests
/// set this to a temp directory so session/draft I/O doesn't touch the
/// user's real data.
#[must_use]
pub fn data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("LUSHTEXT_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("lushtext")
}

/// Load a JSON file from `data_dir/filename`. Returns `None` if the file doesn't exist.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be read or parsed as JSON.
pub fn load<T: DeserializeOwned + Default>(data_dir: &Path, filename: &str) -> Result<T> {
    let path = data_dir.join(filename);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(anyhow::anyhow!("failed to read {}: {}", path.display(), e)),
    }
}

/// Save a value as pretty-printed JSON to `data_dir/filename`.
/// Uses atomic write (write-to-temp + rename) to prevent corruption
/// if the process exits mid-write.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be created, the value cannot
/// be serialized, or the temp file cannot be flushed, synced, or renamed.
pub fn save<T: Serialize>(data_dir: &Path, filename: &str, value: &T) -> Result<()> {
    durable_write::create_dir_all_durable(data_dir)
        .with_context(|| format!("failed to create {}", data_dir.display()))?;
    let path = data_dir.join(filename);
    // The shared helper owns the temp-file-then-rename ordering, the full fsync
    // contract, and identity-metadata preservation. Streaming JSON avoids a
    // second full-sized allocation for large state files.
    durable_write::atomic_write_stream_classified(&path, "json", |writer| {
        serde_json::to_writer_pretty(writer, value).map_err(std::io::Error::other)
    })
    .map_err(durable_write::DurableWriteError::into_io_error)
    .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let result: TestData =
            load(dir.path(), "missing.json").expect("expected operation to succeed");
        assert_eq!(result, TestData::default());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let data = TestData {
            name: "test".into(),
            value: 42,
        };
        save(dir.path(), "data.json", &data).expect("expected operation to succeed");
        let loaded: TestData =
            load(dir.path(), "data.json").expect("expected operation to succeed");
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_load_malformed_json_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("bad.json"), "not valid json {{{")
            .expect("expected operation to succeed");
        let result: Result<TestData> = load(dir.path(), "bad.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_non_file_path_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("data.json")).expect("expected operation to succeed");

        let error: Result<TestData> = load(dir.path(), "data.json");

        let error = error.expect_err("directory JSON path should fail");
        assert!(
            error.to_string().contains("failed to read"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_save_creates_nested_directories() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let nested = dir.path().join("deeply/nested/dir");
        let data = TestData {
            name: "nested".into(),
            value: 1,
        };
        save(&nested, "data.json", &data).expect("expected operation to succeed");
        let loaded: TestData = load(&nested, "data.json").expect("expected operation to succeed");
        assert_eq!(loaded, data);
    }

    #[test]
    fn test_save_overwrites_existing_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let data1 = TestData {
            name: "first".into(),
            value: 1,
        };
        save(dir.path(), "data.json", &data1).expect("expected operation to succeed");

        let data2 = TestData {
            name: "second".into(),
            value: 2,
        };
        save(dir.path(), "data.json", &data2).expect("expected operation to succeed");

        let loaded: TestData =
            load(dir.path(), "data.json").expect("expected operation to succeed");
        assert_eq!(loaded, data2);
    }

    #[test]
    fn test_save_produces_pretty_printed_json() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let data = TestData {
            name: "pretty".into(),
            value: 99,
        };
        save(dir.path(), "data.json", &data).expect("expected operation to succeed");
        let content = std::fs::read_to_string(dir.path().join("data.json"))
            .expect("expected operation to succeed");
        assert!(content.contains('\n'));
    }

    #[test]
    fn test_data_dir_ends_with_lushtext() {
        let dir = data_dir();
        assert_eq!(
            dir.file_name().expect("expected operation to succeed"),
            "lushtext"
        );
    }
}
