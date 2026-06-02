// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for sidecar path rebasing.
//!
//! Rename migration must only rewrite paths that are component-wise descendants
//! of the old root; similar-looking sibling prefixes must stay untouched.

use std::path::{Path, PathBuf};

use lushtext_core::model::sidecar_identity::DocumentSidecarIdentity;
use lushtext_core::services::property_testing::rebase_document_identity_paths;
use proptest::prelude::*;

use crate::support;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn rebases_display_paths_under_old_root(suffix in support::path_suffix()) {
        let old_root = PathBuf::from("/workspace/old");
        let new_root = PathBuf::from("/workspace/new");
        let display_path = append_suffix(&old_root, &suffix);
        let expected = append_suffix(&new_root, &suffix);
        let identity = DocumentSidecarIdentity::from_paths(
            display_path,
            PathBuf::from("/canonical/outside/file.txt"),
        );

        let rebased = rebase_document_identity_paths(&identity, &old_root, &new_root);

        prop_assert_eq!(rebased, Some((expected.clone(), expected)));
    }

    #[test]
    fn rebases_canonical_paths_under_old_root(suffix in support::path_suffix()) {
        let old_root = PathBuf::from("/workspace/old");
        let new_root = PathBuf::from("/workspace/new");
        let canonical_path = append_suffix(&old_root, &suffix);
        let expected = append_suffix(&new_root, &suffix);
        let identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/display/outside/file.txt"),
            canonical_path,
        );

        let rebased = rebase_document_identity_paths(&identity, &old_root, &new_root);

        prop_assert_eq!(rebased, Some((expected.clone(), expected)));
    }

    #[test]
    fn leaves_component_prefix_siblings_untouched(suffix in support::path_suffix()) {
        let old_root = PathBuf::from("/workspace/old");
        let new_root = PathBuf::from("/workspace/new");
        let outside_display = append_suffix(Path::new("/workspace/oldish"), &suffix);
        let outside_canonical = append_suffix(Path::new("/workspace/older"), &suffix);
        let identity = DocumentSidecarIdentity::from_paths(outside_display, outside_canonical);

        let rebased = rebase_document_identity_paths(&identity, &old_root, &new_root);

        prop_assert_eq!(rebased, None);
    }
}

/// Append a generated relative suffix to a stable absolute root.
fn append_suffix(root: &Path, suffix: &Path) -> PathBuf {
    if suffix.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(suffix)
    }
}
