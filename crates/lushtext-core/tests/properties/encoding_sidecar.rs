// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for encoding, line-ending, and sidecar identity helpers.
//!
//! These domain helpers are pure and cheap, making them a good fit for
//! generated round-trip and determinism checks.

use std::path::{Path, PathBuf};

use lushtext_core::model::encoding::{DocumentEncoding, LineEnding};
use lushtext_core::model::sidecar_identity::{stable_bytes_hash, stable_path_hash};
use proptest::prelude::*;

use crate::support;

/// All document encodings exposed by the current picker.
const DOCUMENT_ENCODINGS: [DocumentEncoding; 6] = DocumentEncoding::COMMON;
/// All line-ending states exposed by the domain model.
const LINE_ENDINGS: [LineEnding; 4] = [
    LineEnding::Lf,
    LineEnding::Crlf,
    LineEnding::Cr,
    LineEnding::Mixed,
];

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn document_encoding_ids_roundtrip(encoding in prop::sample::select(&DOCUMENT_ENCODINGS)) {
        prop_assert_eq!(DocumentEncoding::from_id(encoding.id()), Some(encoding));
        prop_assert!(!encoding.id().is_empty());
        prop_assert!(!encoding.label().is_empty());
        prop_assert_eq!(encoding.writes_bom(), matches!(
            encoding,
            DocumentEncoding::Utf8Bom | DocumentEncoding::Utf16Le | DocumentEncoding::Utf16Be
        ));
    }

    #[test]
    fn line_ending_ids_and_separators_are_stable(line_ending in prop::sample::select(&LINE_ENDINGS)) {
        prop_assert_eq!(LineEnding::from_id(line_ending.id()), Some(line_ending));
        prop_assert!(!line_ending.id().is_empty());
        prop_assert!(!line_ending.label().is_empty());
        match line_ending {
            LineEnding::Mixed => prop_assert_eq!(line_ending.separator(), None),
            LineEnding::Lf | LineEnding::Crlf | LineEnding::Cr => {
                prop_assert!(line_ending.separator().is_some_and(|separator| !separator.is_empty()));
            }
        }
    }

    #[test]
    fn sidecar_hashes_are_deterministic_and_content_sensitive(
        bytes in prop::collection::vec(any::<u8>(), 0..=support::MAX_BYTE_VECTOR_LEN),
        extra in any::<u8>(),
    ) {
        let mut changed = bytes.clone();
        changed.push(extra);

        prop_assert_eq!(stable_bytes_hash(&bytes), stable_bytes_hash(&bytes));
        prop_assert_ne!(stable_bytes_hash(&bytes), stable_bytes_hash(&changed));
    }

    #[test]
    fn path_hash_matches_lossy_path_bytes(suffix in support::path_suffix()) {
        let path = append_suffix(Path::new("/workspace/folder"), &suffix);

        prop_assert_eq!(
            stable_path_hash(&path),
            stable_bytes_hash(path.to_string_lossy().as_bytes())
        );
    }
}

/// Append a generated relative suffix to a stable absolute folder.
fn append_suffix(folder: &Path, suffix: &Path) -> PathBuf {
    if suffix.as_os_str().is_empty() {
        folder.to_path_buf()
    } else {
        folder.join(suffix)
    }
}
