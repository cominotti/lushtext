// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable document identity helpers for bookmark, note, and history sidecars.
//!
//! The UI and services reason about notes as "belonging to a saved file path",
//! but persistence needs a filesystem-safe filename that survives restarts.
//! This module keeps that identity shaping explicit and deterministic without
//! pulling GTK or blocking I/O into the model layer.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable identity for one file-backed note sidecar document.
///
/// `display_path` preserves the human-readable path shown in UI and export
/// flows, while `canonical_path` is the deduplicated filesystem identity used
/// to derive the sidecar filename.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentSidecarIdentity {
    /// Original absolute path shown back to the user.
    pub display_path: PathBuf,
    /// Canonical absolute path used for identity and rename migration.
    pub canonical_path: PathBuf,
    /// Deterministic hash of `canonical_path`, used as the sidecar filename stem.
    pub sidecar_id: String,
}

impl DocumentSidecarIdentity {
    /// Build a stable identity from a display path and its already-resolved
    /// canonical filesystem path.
    #[must_use]
    pub fn from_paths(display_path: PathBuf, canonical_path: PathBuf) -> Self {
        Self {
            sidecar_id: stable_path_hash(&canonical_path),
            display_path,
            canonical_path,
        }
    }
}

/// Generate a deterministic sidecar hash from a canonical path.
///
/// A small explicit FNV-1a hash is used instead of `DefaultHasher` so sidecar
/// filenames remain stable across Rust versions and process launches.
#[must_use]
pub fn stable_path_hash(path: &Path) -> String {
    stable_bytes_hash(path.to_string_lossy().as_bytes())
}

/// Generate a deterministic hash from arbitrary bytes.
///
/// Local history reuses the same explicit hash for snapshot deduplication so
/// persistence never relies on process-randomized hash seeds.
#[must_use]
pub fn stable_bytes_hash(bytes: &[u8]) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{hash:016x}")
}

/// Generate a stable-enough record ID for bookmark and note entries.
///
/// The timestamp keeps IDs roughly time ordered for debugging, while the
/// monotonic counter avoids collisions when several records are created in the
/// same system-time tick.
#[must_use]
pub fn next_record_id(prefix: &str) -> String {
    static NEXT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = NEXT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{prefix}-{now_nanos:032x}-{counter:016x}")
}

/// Current UNIX timestamp in seconds for persisted note metadata.
#[must_use]
pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Current UNIX timestamp in milliseconds for persisted snapshot ordering.
#[must_use]
pub fn now_epoch_millis() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_hash_is_deterministic() {
        let path = Path::new("/tmp/project/src/main.rs");
        assert_eq!(stable_path_hash(path), stable_path_hash(path));
    }

    #[test]
    fn stable_hash_changes_with_path() {
        assert_ne!(
            stable_path_hash(Path::new("/tmp/a.rs")),
            stable_path_hash(Path::new("/tmp/b.rs"))
        );
    }

    #[test]
    fn stable_bytes_hash_changes_with_content() {
        assert_ne!(stable_bytes_hash(b"alpha"), stable_bytes_hash(b"beta"));
    }

    #[test]
    fn next_record_id_uses_requested_prefix() {
        assert!(next_record_id("bookmark").starts_with("bookmark-"));
        assert!(next_record_id("document-note").starts_with("document-note-"));
    }

    #[test]
    fn identity_uses_canonical_hash() {
        let identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/tmp/link.rs"),
            PathBuf::from("/tmp/real.rs"),
        );

        assert_eq!(
            identity.sidecar_id,
            stable_path_hash(Path::new("/tmp/real.rs"))
        );
    }

    #[test]
    fn epoch_helpers_return_current_nonzero_time_units() {
        let secs = now_epoch_secs();
        let millis = now_epoch_millis();

        assert!(secs > 1_700_000_000);
        assert!(millis >= secs * 1000);
    }
}
