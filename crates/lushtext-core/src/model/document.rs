// SPDX-License-Identifier: GPL-3.0-or-later

//! Document model — runtime state for an open file in a tab.

use std::path::PathBuf;

/// Runtime identity for an open document (derived from canonical path).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentId(pub PathBuf);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_equality_same_path() {
        let a = DocumentId(PathBuf::from("/tmp/file.rs"));
        let b = DocumentId(PathBuf::from("/tmp/file.rs"));
        assert_eq!(a, b);
    }

    #[test]
    fn test_inequality_different_path() {
        let a = DocumentId(PathBuf::from("/tmp/file.rs"));
        let b = DocumentId(PathBuf::from("/tmp/other.rs"));
        assert_ne!(a, b);
    }

    #[test]
    fn test_hash_consistent_with_equality() {
        let a = DocumentId(PathBuf::from("/tmp/file.rs"));
        let b = DocumentId(PathBuf::from("/tmp/file.rs"));
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn test_hash_distinguishes_different_paths() {
        let a = DocumentId(PathBuf::from("/tmp/file.rs"));
        let b = DocumentId(PathBuf::from("/tmp/other.rs"));
        let mut set = HashSet::new();
        set.insert(a);
        assert!(!set.contains(&b));
    }

    #[test]
    fn test_clone_preserves_equality() {
        let a = DocumentId(PathBuf::from("/tmp/file.rs"));
        let b = a.clone();
        assert_eq!(a, b);
    }
}
