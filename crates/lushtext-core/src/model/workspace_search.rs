// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain workspace-search traversal planning and fallback identity admission.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Maximum fallback identities retained when root alias coverage is ambiguous.
pub const WORKSPACE_SEARCH_FALLBACK_ENTRY_LIMIT: usize = 0x0001_0000;
/// Maximum conservative path bytes retained by the ambiguous-alias fallback ledger.
pub const WORKSPACE_SEARCH_FALLBACK_PATH_BYTE_LIMIT: u64 = 8 * 1024 * 1024;

/// One ordered configured root used to preserve display ownership after normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSearchDisplayRoot {
    configured_path: PathBuf,
    canonical_path: Option<PathBuf>,
}

impl WorkspaceSearchDisplayRoot {
    /// Return the path captured from the ordered workspace scope.
    #[must_use]
    pub fn configured_path(&self) -> &Path {
        &self.configured_path
    }

    /// Return the one-time canonical identity, when resolution succeeded.
    #[must_use]
    pub fn canonical_path(&self) -> Option<&Path> {
        self.canonical_path.as_deref()
    }
}

/// One ordered, non-overlapping engine partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSearchTraversalRoot {
    scan_path: PathBuf,
    canonical_path: Option<PathBuf>,
    display_precedence: usize,
    excluded_paths: Vec<PathBuf>,
}

impl WorkspaceSearchTraversalRoot {
    /// Return the configured path the traversal engine should scan.
    #[must_use]
    pub fn scan_path(&self) -> &Path {
        &self.scan_path
    }

    /// Return the one-time canonical identity, when resolution succeeded.
    #[must_use]
    pub fn canonical_path(&self) -> Option<&Path> {
        self.canonical_path.as_deref()
    }

    /// Return the first configured-folder position covered by this engine root.
    #[must_use]
    pub fn display_precedence(&self) -> usize {
        self.display_precedence
    }

    /// Return descendant roots already covered by earlier configured partitions.
    #[must_use]
    pub fn excluded_paths(&self) -> &[PathBuf] {
        &self.excluded_paths
    }
}

/// Immutable ordered workspace-search plan shared by one search generation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSearchTraversalPlan {
    traversal_roots: Vec<WorkspaceSearchTraversalRoot>,
    display_roots: Vec<WorkspaceSearchDisplayRoot>,
    fallback_identity_required: bool,
}

impl WorkspaceSearchTraversalPlan {
    /// Resolve each distinct configured root once and collapse proven coverage.
    ///
    /// The canonicalizer is supplied by the service boundary so this policy
    /// stays deterministic and filesystem-free in unit tests.
    #[must_use]
    pub fn build<I, P, F, E>(roots: I, mut canonicalize: F) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
        F: FnMut(&Path) -> Result<PathBuf, E>,
    {
        let mut display_roots: Vec<WorkspaceSearchDisplayRoot> = Vec::new();
        let mut traversal_roots: Vec<WorkspaceSearchTraversalRoot> = Vec::new();

        for configured_path in roots.into_iter().map(Into::into) {
            if display_roots
                .iter()
                .any(|root| root.configured_path == configured_path)
            {
                continue;
            }

            let canonical_path = canonicalize(&configured_path).ok();
            if canonical_path.as_ref().is_some_and(|canonical| {
                display_roots
                    .iter()
                    .any(|root| root.canonical_path.as_ref() == Some(canonical))
            }) {
                continue;
            }

            let display_index = display_roots.len();
            display_roots.push(WorkspaceSearchDisplayRoot {
                configured_path: configured_path.clone(),
                canonical_path: canonical_path.clone(),
            });

            let Some(canonical_path) = canonical_path else {
                traversal_roots.push(WorkspaceSearchTraversalRoot {
                    scan_path: configured_path,
                    canonical_path: None,
                    display_precedence: display_index,
                    excluded_paths: Vec::new(),
                });
                continue;
            };

            if traversal_roots.iter().any(|root| {
                root.canonical_path
                    .as_ref()
                    .is_some_and(|ancestor| canonical_path.starts_with(ancestor))
            }) {
                continue;
            }

            // A child configured before its parent must remain the first engine
            // partition. The later parent excludes that already-covered subtree,
            // preserving sequential folder precedence without a per-file ledger.
            let excluded_paths = traversal_roots
                .iter()
                .filter_map(|root| {
                    let descendant = root.canonical_path.as_ref()?;
                    let relative = descendant.strip_prefix(&canonical_path).ok()?;
                    (!relative.as_os_str().is_empty()).then(|| configured_path.join(relative))
                })
                .collect();
            traversal_roots.push(WorkspaceSearchTraversalRoot {
                scan_path: configured_path,
                canonical_path: Some(canonical_path),
                display_precedence: display_index,
                excluded_paths,
            });
        }

        let unresolved_roots = traversal_roots
            .iter()
            .filter(|root| root.canonical_path.is_none())
            .count();
        let fallback_identity_required = unresolved_roots > 0 && traversal_roots.len() > 1;

        Self {
            traversal_roots,
            display_roots,
            fallback_identity_required,
        }
    }

    /// Return ordered non-overlapping partitions over the minimal physical coverage.
    #[must_use]
    pub fn traversal_roots(&self) -> &[WorkspaceSearchTraversalRoot] {
        &self.traversal_roots
    }

    /// Return the first-owner map in configured folder order.
    #[must_use]
    pub fn display_roots(&self) -> &[WorkspaceSearchDisplayRoot] {
        &self.display_roots
    }

    /// Return whether unresolved coverage needs bounded per-file identity tracking.
    #[must_use]
    pub fn fallback_identity_required(&self) -> bool {
        self.fallback_identity_required
    }

    /// Resolve the first configured display owner for one walked result path.
    #[must_use]
    pub fn display_owner_for_walked_path(
        &self,
        traversal_index: usize,
        walked_path: &Path,
    ) -> Option<&WorkspaceSearchDisplayRoot> {
        let traversal = self.traversal_roots.get(traversal_index)?;
        let relative = walked_path.strip_prefix(&traversal.scan_path).ok()?;
        let canonical_candidate = traversal
            .canonical_path
            .as_ref()
            .map(|root| root.join(relative));

        self.display_roots.iter().find(|display| {
            match (
                display.canonical_path.as_ref(),
                canonical_candidate.as_ref(),
            ) {
                (Some(root), Some(candidate)) => candidate.starts_with(root),
                _ => walked_path.starts_with(&display.configured_path),
            }
        })
    }

    /// Return a display-relative path under the first configured owner.
    #[must_use]
    pub fn display_relative_path(
        &self,
        traversal_index: usize,
        walked_path: &Path,
    ) -> Option<PathBuf> {
        let traversal = self.traversal_roots.get(traversal_index)?;
        let relative = walked_path.strip_prefix(&traversal.scan_path).ok()?;
        let canonical_candidate = traversal
            .canonical_path
            .as_ref()
            .map(|root| root.join(relative));
        let display = self.display_owner_for_walked_path(traversal_index, walked_path)?;
        match (display.canonical_path.as_ref(), canonical_candidate) {
            (Some(root), Some(candidate)) => {
                candidate.strip_prefix(root).ok().map(Path::to_path_buf)
            }
            _ => walked_path
                .strip_prefix(&display.configured_path)
                .ok()
                .map(Path::to_path_buf),
        }
    }
}

/// Entry and byte ceilings for ambiguous-alias fallback identity retention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceSearchFallbackLimits {
    /// Maximum distinct file identities retained.
    pub entries: usize,
    /// Maximum conservative path ownership retained.
    pub path_bytes: u64,
}

impl Default for WorkspaceSearchFallbackLimits {
    fn default() -> Self {
        Self {
            entries: WORKSPACE_SEARCH_FALLBACK_ENTRY_LIMIT,
            path_bytes: WORKSPACE_SEARCH_FALLBACK_PATH_BYTE_LIMIT,
        }
    }
}

/// Typed incomplete-search terminal emitted before fallback ownership exceeds policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceSearchIncompleteReason {
    /// The next distinct identity would exceed the entry ceiling.
    FallbackEntryLimit,
    /// The next distinct identity would exceed the conservative path-byte ceiling.
    FallbackPathByteLimit,
}

/// Result of claiming one file identity in the fallback ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceSearchFallbackClaim {
    /// The identity was admitted and should be searched.
    Admitted,
    /// The identity was already visited by another ambiguous traversal root.
    Duplicate,
    /// Admission stopped before the next charge crossed a declared limit.
    Incomplete(WorkspaceSearchIncompleteReason),
}

/// Compact high-water evidence for fallback identity retention.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSearchFallbackMetrics {
    /// Current and peak admitted identities; identities live for the generation.
    pub entries: usize,
    /// Current and peak conservative path ownership; paths live for the generation.
    pub path_bytes: u64,
}

/// Saturating per-generation identity ledger used only for ambiguous root aliases.
#[derive(Debug)]
pub struct WorkspaceSearchFallbackLedger {
    limits: WorkspaceSearchFallbackLimits,
    identities: HashSet<PathBuf>,
    metrics: WorkspaceSearchFallbackMetrics,
}

impl WorkspaceSearchFallbackLedger {
    /// Start an empty ledger under explicit entry and byte ceilings.
    #[must_use]
    pub fn new(limits: WorkspaceSearchFallbackLimits) -> Self {
        Self {
            limits,
            identities: HashSet::new(),
            metrics: WorkspaceSearchFallbackMetrics::default(),
        }
    }

    /// Claim a complete identity without ever crossing either configured limit.
    pub fn try_claim(&mut self, identity: PathBuf) -> WorkspaceSearchFallbackClaim {
        if self.identities.contains(&identity) {
            return WorkspaceSearchFallbackClaim::Duplicate;
        }

        let next_entries = self.metrics.entries.saturating_add(1);
        if next_entries > self.limits.entries {
            return WorkspaceSearchFallbackClaim::Incomplete(
                WorkspaceSearchIncompleteReason::FallbackEntryLimit,
            );
        }
        let path_bytes = conservative_path_bytes(&identity);
        let next_path_bytes = self.metrics.path_bytes.saturating_add(path_bytes);
        if next_path_bytes > self.limits.path_bytes {
            return WorkspaceSearchFallbackClaim::Incomplete(
                WorkspaceSearchIncompleteReason::FallbackPathByteLimit,
            );
        }

        let inserted = self.identities.insert(identity);
        debug_assert!(inserted);
        self.metrics.entries = next_entries;
        self.metrics.path_bytes = next_path_bytes;
        WorkspaceSearchFallbackClaim::Admitted
    }

    /// Return direct container and byte high-water evidence.
    #[must_use]
    pub fn metrics(&self) -> WorkspaceSearchFallbackMetrics {
        self.metrics
    }
}

fn conservative_path_bytes(path: &Path) -> u64 {
    u64::try_from(std::mem::size_of::<PathBuf>())
        .unwrap_or(u64::MAX)
        .saturating_add(
            u64::try_from(path.as_os_str().as_encoded_bytes().len()).unwrap_or(u64::MAX),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::convert::Infallible;

    fn identity(path: &Path) -> Result<PathBuf, Infallible> {
        Ok(path.to_path_buf())
    }

    #[test]
    fn exact_duplicates_are_removed_before_canonicalization() {
        let calls = Cell::new(0usize);
        let plan = WorkspaceSearchTraversalPlan::build(
            [PathBuf::from("/repo"), PathBuf::from("/repo")],
            |path| {
                calls.set(calls.get() + 1);
                identity(path)
            },
        );

        assert_eq!(calls.get(), 1);
        assert_eq!(plan.traversal_roots().len(), 1);
        assert_eq!(plan.display_roots().len(), 1);
        assert!(!plan.fallback_identity_required());
    }

    #[test]
    fn parent_before_child_collapses_to_the_parent_partition() {
        let plan = WorkspaceSearchTraversalPlan::build(
            [PathBuf::from("/repo"), PathBuf::from("/repo/src")],
            identity,
        );
        assert_eq!(plan.traversal_roots().len(), 1);
        assert_eq!(plan.traversal_roots()[0].scan_path(), Path::new("/repo"));
        assert!(plan.traversal_roots()[0].excluded_paths().is_empty());
        assert!(!plan.fallback_identity_required());
    }

    #[test]
    fn child_before_parent_becomes_ordered_non_overlapping_partitions() {
        let plan = WorkspaceSearchTraversalPlan::build(
            [PathBuf::from("/repo/src"), PathBuf::from("/repo")],
            identity,
        );
        assert_eq!(plan.traversal_roots().len(), 2);
        assert_eq!(
            plan.traversal_roots()[0].scan_path(),
            Path::new("/repo/src")
        );
        assert_eq!(plan.traversal_roots()[1].scan_path(), Path::new("/repo"));
        assert_eq!(
            plan.traversal_roots()[1].excluded_paths(),
            &[PathBuf::from("/repo/src")]
        );
        assert!(!plan.fallback_identity_required());
    }

    #[test]
    fn canonical_aliases_keep_the_first_configured_display_owner() {
        let plan = WorkspaceSearchTraversalPlan::build(
            [PathBuf::from("/alias"), PathBuf::from("/real")],
            |_| Ok::<_, Infallible>(PathBuf::from("/real")),
        );

        assert_eq!(plan.traversal_roots().len(), 1);
        assert_eq!(plan.traversal_roots()[0].scan_path(), Path::new("/alias"));
        assert_eq!(plan.display_roots().len(), 1);
        assert_eq!(
            plan.display_owner_for_walked_path(0, Path::new("/alias/src/main.rs"))
                .map(WorkspaceSearchDisplayRoot::configured_path),
            Some(Path::new("/alias"))
        );
    }

    #[test]
    fn unresolved_single_root_needs_no_ledger_but_multiple_roots_do() {
        let single = WorkspaceSearchTraversalPlan::build([PathBuf::from("/missing")], |_| {
            Err::<PathBuf, _>(())
        });
        assert!(!single.fallback_identity_required());

        let multiple = WorkspaceSearchTraversalPlan::build(
            [PathBuf::from("/missing"), PathBuf::from("/repo")],
            |path| {
                if path == Path::new("/missing") {
                    Err(())
                } else {
                    Ok(path.to_path_buf())
                }
            },
        );
        assert!(multiple.fallback_identity_required());
        assert_eq!(multiple.traversal_roots().len(), 2);
    }

    #[test]
    fn fallback_ledger_stops_at_exact_entry_boundary() {
        let mut ledger = WorkspaceSearchFallbackLedger::new(WorkspaceSearchFallbackLimits {
            entries: 1,
            path_bytes: u64::MAX,
        });
        assert_eq!(
            ledger.try_claim(PathBuf::from("/repo/a")),
            WorkspaceSearchFallbackClaim::Admitted
        );
        assert_eq!(
            ledger.try_claim(PathBuf::from("/repo/a")),
            WorkspaceSearchFallbackClaim::Duplicate
        );
        assert_eq!(
            ledger.try_claim(PathBuf::from("/repo/b")),
            WorkspaceSearchFallbackClaim::Incomplete(
                WorkspaceSearchIncompleteReason::FallbackEntryLimit
            )
        );
        assert_eq!(ledger.metrics().entries, 1);
    }

    #[test]
    fn fallback_ledger_stops_before_one_over_path_boundary() {
        let path = PathBuf::from("/repo/é.rs");
        let exact = conservative_path_bytes(&path);
        let mut exact_ledger = WorkspaceSearchFallbackLedger::new(WorkspaceSearchFallbackLimits {
            entries: usize::MAX,
            path_bytes: exact,
        });
        assert_eq!(
            exact_ledger.try_claim(path.clone()),
            WorkspaceSearchFallbackClaim::Admitted
        );
        assert_eq!(exact_ledger.metrics().path_bytes, exact);

        let mut short_ledger = WorkspaceSearchFallbackLedger::new(WorkspaceSearchFallbackLimits {
            entries: usize::MAX,
            path_bytes: exact - 1,
        });
        assert_eq!(
            short_ledger.try_claim(path),
            WorkspaceSearchFallbackClaim::Incomplete(
                WorkspaceSearchIncompleteReason::FallbackPathByteLimit
            )
        );
        assert_eq!(
            short_ledger.metrics(),
            WorkspaceSearchFallbackMetrics::default()
        );
    }
}
