// SPDX-License-Identifier: GPL-3.0-or-later

//! Generic JSON file persistence: load/save any serde type to a JSON file.

use crate::services::filesystem::{WriteLabel, read, write};
use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::Path;

/// Returns the application data directory (`$XDG_DATA_HOME/lushtext`).
///
/// Respects the `LUSHTEXT_DATA_DIR` env var for test isolation: the widget-test
/// harness, smoke lanes, and manual drills point it at a private directory so
/// session/draft I/O doesn't touch the user's real data.
///
/// # Panics
///
/// Only in test builds (`cfg(test)` or the `test-utils` feature), when the
/// resolved directory lies in the developer's real home application data; see
/// `services::test_isolation`. Production builds never panic here.
#[must_use]
pub fn data_dir() -> std::path::PathBuf {
    checked_data_dir(
        std::env::var_os("LUSHTEXT_DATA_DIR").map(std::path::PathBuf::from),
        dirs::data_dir(),
        std::env::var_os("HOME").as_deref().map(Path::new),
    )
}

/// [`data_dir`] over explicit inputs: resolve, then, in test builds only,
/// refuse a directory inside `home`'s application data trees.
#[cfg_attr(
    not(any(test, feature = "test-utils")),
    expect(
        unused_variables,
        reason = "the home is read only by the test-build guard"
    )
)]
fn checked_data_dir(
    override_dir: Option<std::path::PathBuf>,
    platform_data_home: Option<std::path::PathBuf>,
    home: Option<&Path>,
) -> std::path::PathBuf {
    let dir = data_dir_from(override_dir, platform_data_home);
    #[cfg(any(test, feature = "test-utils"))]
    crate::services::test_isolation::assert_app_data_dir_is_isolated(&dir, home);
    dir
}

/// Resolve the application data directory from an explicit override and the
/// platform data home, in that order of precedence.
fn data_dir_from(
    override_dir: Option<std::path::PathBuf>,
    platform_data_home: Option<std::path::PathBuf>,
) -> std::path::PathBuf {
    if let Some(dir) = override_dir {
        return dir;
    }
    platform_data_home
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("lushtext")
}

/// Directory of the derived GtkSourceView style schemes LushText writes:
/// `{data_dir}/style-schemes/`.
#[must_use]
pub fn style_schemes_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join(STYLE_SCHEMES_DIR)
}

/// Name of the derived style-scheme directory under the app data home.
const STYLE_SCHEMES_DIR: &str = "style-schemes";

/// Load a JSON file from `data_dir/filename`. Returns `None` if the file doesn't exist.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be read or parsed as JSON.
pub fn load<T: DeserializeOwned + Default>(data_dir: &Path, filename: &str) -> Result<T> {
    let path = data_dir.join(filename);
    match read::bytes(&path) {
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
    write::create_dir_all_durable(data_dir)
        .with_context(|| format!("failed to create {}", data_dir.display()))?;
    let path = data_dir.join(filename);
    // The shared helper owns the temp-file-then-rename ordering, the full fsync
    // contract, and identity-metadata preservation. Streaming JSON avoids a
    // second full-sized allocation for large state files.
    write::atomic_replace_stream(&path, WriteLabel::JSON, |writer| {
        serde_json::to_writer_pretty(writer, value).map_err(std::io::Error::other)
    })
    .map_err(write::DurableWriteError::into_io_error)
    .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
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
        fixture::write_text(&dir.path().join("bad.json"), "not valid json {{{");
        let result: Result<TestData> = load(dir.path(), "bad.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_non_file_path_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join("data.json"));

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
        let content = fixture::read_text(&dir.path().join("data.json"));
        assert!(content.contains('\n'));
    }

    #[test]
    fn test_data_dir_ends_with_lushtext() {
        // Resolves from a fake data home: reading the process's real
        // `dirs::data_dir()` here resolved the developer's own app data.
        let home = TempDir::new().expect("fake data home");
        let dir = data_dir_from(None, Some(home.path().to_path_buf()));
        assert_eq!(dir, home.path().join("lushtext"));
    }

    #[test]
    fn test_data_dir_override_wins() {
        let override_dir = TempDir::new().expect("override dir");
        let home = TempDir::new().expect("fake data home");
        let dir = data_dir_from(
            Some(override_dir.path().to_path_buf()),
            Some(home.path().to_path_buf()),
        );
        assert_eq!(dir, override_dir.path());
    }

    #[test]
    #[should_panic(expected = "test isolation violated")]
    fn test_data_dir_refuses_the_real_home_app_data() {
        // A fake HOME stands in for the developer's: with no override and the
        // default data home, resolution lands in `$HOME/.local/share/lushtext`,
        // and a test build must refuse it before any caller can write.
        let home = TempDir::new().expect("fake home");
        let _ = checked_data_dir(
            None,
            Some(home.path().join(".local/share")),
            Some(home.path()),
        );
    }

    #[test]
    #[should_panic(expected = "test isolation violated")]
    fn test_data_dir_refuses_an_override_into_the_real_home() {
        let home = TempDir::new().expect("fake home");
        let _ = checked_data_dir(
            Some(
                home.path()
                    .join(".var/app/dev.cominotti.lushtext/data/lushtext"),
            ),
            None,
            Some(home.path()),
        );
    }

    #[test]
    fn test_data_dir_accepts_a_private_override() {
        let home = TempDir::new().expect("fake home");
        let private = TempDir::new().expect("private data dir");
        let dir = checked_data_dir(
            Some(private.path().to_path_buf()),
            Some(home.path().join(".local/share")),
            Some(home.path()),
        );
        assert_eq!(dir, private.path());
    }
}
