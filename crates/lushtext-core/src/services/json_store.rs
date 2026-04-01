// SPDX-License-Identifier: GPL-3.0-or-later

//! Generic JSON file persistence: load/save any serde type to a JSON file.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;

/// Returns the application data directory (`$XDG_DATA_HOME/lushtext`).
pub fn data_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("lushtext")
}

/// Load a JSON file from `data_dir/filename`. Returns `None` if the file doesn't exist.
pub fn load<T: DeserializeOwned + Default>(data_dir: &Path, filename: &str) -> Result<T> {
    let path = data_dir.join(filename);
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(anyhow::anyhow!("failed to read {}: {}", path.display(), e)),
    }
}

/// Save a value as pretty-printed JSON to `data_dir/filename`.
pub fn save<T: Serialize>(data_dir: &Path, filename: &str, value: &T) -> Result<()> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("failed to create {}", data_dir.display()))?;
    let path = data_dir.join(filename);
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))
}
