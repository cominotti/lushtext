// SPDX-License-Identifier: GPL-3.0-or-later

//! Property test for durable-write identity-metadata preservation.
//!
//! Over randomized permission bits and byte payloads, an atomic overwrite must
//! keep the destination's prior mode while still replacing its content. The
//! fixture is a single tempdir-backed file per case, so the property stays
//! deterministic and cheap and never touches GTK or a compositor.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use lushtext_core::services::durable_write;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use crate::support;

/// Read a file's standard permission bits (the guaranteed `0o777` rwx bits).
///
/// The setuid/setgid/sticky special bits are intentionally excluded: the kernel
/// clears setuid/setgid when an unprivileged process rewrites file contents, so
/// only the standard permission bits are part of the durable-write guarantee.
fn file_mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path)
        .expect("stat generated file")
        .permissions()
        .mode()
        & 0o777
}

proptest! {
    #![proptest_config(support::property_config())]

    /// An atomic overwrite preserves the destination's standard permission bits
    /// exactly while replacing its bytes, for any owner-readable/writable mode.
    #[test]
    fn atomic_overwrite_preserves_mode(
        // Keep the owner read+write bits so the test can always re-read the file,
        // and vary the remaining standard rwx bits freely.
        extra_bits in 0u32..0o177,
        old_bytes in proptest::collection::vec(any::<u8>(), 0..support::MAX_BYTE_VECTOR_LEN),
        new_bytes in proptest::collection::vec(any::<u8>(), 0..support::MAX_BYTE_VECTOR_LEN),
    ) {
        let mode = 0o600 | extra_bits;

        let dir = tempfile::tempdir().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let path = dir.path().join("payload.bin");
        std::fs::write(&path, &old_bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        let before = file_mode(&path);

        durable_write::atomic_write_bytes(&path, "prop", &new_bytes)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        // Content is replaced ...
        let written = std::fs::read(&path).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(written, new_bytes);
        // ... and the mode is preserved across the overwrite.
        prop_assert_eq!(file_mode(&path), before);
    }

    /// Durable copy fallback carries the source's standard mode over any
    /// existing destination mode while replacing bytes and removing the source.
    #[test]
    fn copy_fallback_preserves_source_mode(
        source_extra_bits in 0u32..0o177,
        dest_extra_bits in 0u32..0o177,
        source_bytes in proptest::collection::vec(any::<u8>(), 0..support::MAX_BYTE_VECTOR_LEN),
        dest_bytes in proptest::collection::vec(any::<u8>(), 0..support::MAX_BYTE_VECTOR_LEN),
    ) {
        let source_mode = 0o600 | source_extra_bits;
        let dest_mode = 0o600 | dest_extra_bits;

        let dir = tempfile::tempdir().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let from = dir.path().join("from.bin");
        let to = dir.path().join("to.bin");
        std::fs::write(&from, &source_bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
        std::fs::write(&to, &dest_bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
        std::fs::set_permissions(&from, std::fs::Permissions::from_mode(source_mode))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        std::fs::set_permissions(&to, std::fs::Permissions::from_mode(dest_mode))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        let expected_mode = file_mode(&from);

        durable_write::copy_file_durable(&from, &to, "prop-copy")
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert!(!from.exists());
        let written = std::fs::read(&to).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(written, source_bytes);
        prop_assert_eq!(file_mode(&to), expected_mode);
    }
}
