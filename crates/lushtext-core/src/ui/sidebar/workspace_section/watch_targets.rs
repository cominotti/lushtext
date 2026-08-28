// SPDX-License-Identifier: GPL-3.0-or-later

//! Incremental materialized watch-target bookkeeping for a flattened tree.
//!
//! # Role: none — a data structure owned by the `watch` role
//!
//! Deliberately classified rather than left for a reader to infer. This module is a
//! plain incremental mirror owned by `watch.rs`: no GTK import, no widget, no stage of
//! its own, and no ordered side effect. It is therefore **not** one of the five roles
//! (giving `watch` a second role module would split one coordination job in two), and
//! **not** a called presentation surface (it projects nothing onto widgets). It is not
//! `policy.rs` either: it is stateful bookkeeping rather than a pure decision, and this
//! workflow owns exactly one `policy.rs`, at its canonical role home in `ui/sidebar/`.
//!
//! The workflow's matrix row records the same classification.

use std::collections::{BTreeMap, BTreeSet};

use crate::services::workspace_watch::WorkspaceWatchTarget;
// The two generation newtypes moved to the workflow's `seams.rs`: they are seam
// values, captured at dispatch and compared at completion.
use crate::ui::sidebar::seams::WatchTargetGeneration;

/// Owned target snapshot passed from GTK orchestration to watcher workers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WatchTargetSnapshot {
    pub(super) generation: WatchTargetGeneration,
    pub(super) targets: Vec<WorkspaceWatchTarget>,
}

/// Target contribution of one row in the flattened `GtkTreeListModel`.
pub(super) type RowWatchContribution = Option<WorkspaceWatchTarget>;

/// Plain incremental mirror of the flattened model's watch contributions.
///
/// `rows` stays index-aligned with the flattened model while it is mounted.
/// Counts preserve overlapping paths without asking GTK to rewalk unrelated
/// rows. Before a model is mounted, configured top-level folders supply the
/// same effective fallback used by the previous implementation.
#[derive(Debug, Default)]
pub(super) struct MaterializedWatchTargets {
    rows: Vec<RowWatchContribution>,
    counts: BTreeMap<WorkspaceWatchTarget, usize>,
    fallback: BTreeMap<WorkspaceWatchTarget, usize>,
    model_mounted: bool,
    generation: WatchTargetGeneration,
    /// Gated identically to every writer and reader below: a field only the
    /// `test-utils` build ever touches is `dead_code` in a default-feature build,
    /// and `make check` runs `--all-features`, so nothing else would report it.
    #[cfg(feature = "test-utils")]
    touched_rows: usize,
}

impl MaterializedWatchTargets {
    /// Replace the pre-model configured-folder fallback.
    pub(super) fn set_fallback(&mut self, targets: Vec<WorkspaceWatchTarget>) -> bool {
        let before = self.effective_targets();
        self.fallback = count_targets(targets.into_iter().map(Some));
        self.finish_full_mutation(&before, 0)
    }

    /// Mark the flattened model absent while retaining configured roots.
    pub(super) fn unmount(&mut self) -> bool {
        let before = self.effective_targets();
        self.rows.clear();
        self.counts.clear();
        self.model_mounted = false;
        self.finish_full_mutation(&before, 0)
    }

    /// Install a complete mirror once when a flattened model is replaced.
    pub(super) fn mount(&mut self, rows: Vec<RowWatchContribution>) -> bool {
        let before = self.effective_targets();
        let touched = rows.len();
        self.counts = count_targets(rows.iter().cloned());
        self.rows = rows;
        self.model_mounted = true;
        self.finish_full_mutation(&before, touched)
    }

    /// Apply one `items-changed` splice without visiting unaffected rows.
    pub(super) fn splice(
        &mut self,
        position: usize,
        removed: usize,
        added: &[RowWatchContribution],
    ) -> bool {
        assert!(self.model_mounted, "row splice requires a mounted model");
        assert!(position <= self.rows.len(), "splice position is in bounds");
        assert!(
            position.saturating_add(removed) <= self.rows.len(),
            "removed rows are in bounds"
        );

        let removed_rows = self
            .rows
            .splice(position..position + removed, added.iter().cloned())
            .collect::<Vec<_>>();
        let affected = removed_rows
            .iter()
            .chain(added)
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        let present_before = affected
            .iter()
            .map(|target| (target.clone(), self.counts.contains_key(target)))
            .collect::<BTreeMap<_, _>>();
        for target in removed_rows.iter().flatten() {
            decrement_count(&mut self.counts, target);
        }
        for target in added.iter().flatten() {
            *self.counts.entry(target.clone()).or_insert(0) += 1;
        }
        let changed = affected
            .iter()
            .any(|target| present_before[target] != self.counts.contains_key(target));
        self.finish_incremental_mutation(changed, removed + added.len())
    }

    /// Refresh one row after its expanded state changes.
    pub(super) fn update_row(
        &mut self,
        position: usize,
        contribution: RowWatchContribution,
    ) -> bool {
        assert!(self.model_mounted, "row update requires a mounted model");
        let Some(previous) = self.rows.get_mut(position) else {
            return false;
        };
        if *previous == contribution {
            #[cfg(feature = "test-utils")]
            self.record_touched_rows(1);
            return false;
        }

        let previous_target = previous.clone();
        let previous_was_present = previous_target
            .as_ref()
            .is_some_and(|target| self.counts.contains_key(target));
        let contribution_was_present = contribution
            .as_ref()
            .is_some_and(|target| self.counts.contains_key(target));
        if let Some(target) = previous.as_ref() {
            decrement_count(&mut self.counts, target);
        }
        if let Some(target) = contribution.as_ref() {
            *self.counts.entry(target.clone()).or_insert(0) += 1;
        }
        *previous = contribution;
        let previous_is_present = previous_target
            .as_ref()
            .is_some_and(|target| self.counts.contains_key(target));
        let contribution_is_present = previous
            .as_ref()
            .is_some_and(|target| self.counts.contains_key(target));
        self.finish_incremental_mutation(
            previous_was_present != previous_is_present
                || contribution_was_present != contribution_is_present,
            1,
        )
    }

    #[must_use]
    pub(super) fn snapshot(&self) -> WatchTargetSnapshot {
        WatchTargetSnapshot {
            generation: self.generation,
            targets: self.effective_targets(),
        }
    }

    /// Return the current generation without cloning target paths.
    #[must_use]
    pub(super) const fn generation(&self) -> WatchTargetGeneration {
        self.generation
    }

    /// Return whether the effective deduplicated set is empty without allocation.
    #[must_use]
    pub(super) fn is_empty(&self) -> bool {
        if self.model_mounted {
            self.counts.is_empty()
        } else {
            self.fallback.is_empty()
        }
    }

    pub(super) fn effective_targets(&self) -> Vec<WorkspaceWatchTarget> {
        let counts = if self.model_mounted {
            &self.counts
        } else {
            &self.fallback
        };
        counts.keys().cloned().collect()
    }

    fn finish_full_mutation(&mut self, before: &[WorkspaceWatchTarget], touched: usize) -> bool {
        #[cfg(feature = "test-utils")]
        self.record_touched_rows(touched);
        #[cfg(not(feature = "test-utils"))]
        let _ = touched;
        let changed = before != self.effective_targets();
        if changed {
            self.generation = self.generation.next();
        }
        changed
    }

    fn finish_incremental_mutation(&mut self, changed: bool, touched: usize) -> bool {
        #[cfg(feature = "test-utils")]
        self.record_touched_rows(touched);
        #[cfg(not(feature = "test-utils"))]
        let _ = touched;
        if changed {
            self.generation = self.generation.next();
        }
        changed
    }

    /// Accumulate the row-touch probe counter.
    ///
    /// Gated with its call sites rather than kept as a production no-op: a
    /// `&mut self` receiver that only the test build reads is an unused receiver
    /// in a default-feature build, which `clippy::unused_self` denies.
    #[cfg(feature = "test-utils")]
    fn record_touched_rows(&mut self, touched: usize) {
        self.touched_rows = self.touched_rows.saturating_add(touched);
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    /// Rows touched since the last reset, **without** resetting.
    ///
    /// The pre-convention seam was a `take`, so counting mutated. The evidence surface
    /// must not change the metric it reports, so observation and reset are separate.
    pub(super) const fn touched_rows_for_evidence(&self) -> usize {
        self.touched_rows
    }

    /// Reset the touched-row counter. A drive, not an observation.
    #[cfg(feature = "test-utils")]
    pub(super) const fn reset_touched_rows(&mut self) {
        self.touched_rows = 0;
    }
}

fn count_targets(
    contributions: impl IntoIterator<Item = RowWatchContribution>,
) -> BTreeMap<WorkspaceWatchTarget, usize> {
    let mut counts = BTreeMap::new();
    for target in contributions.into_iter().flatten() {
        *counts.entry(target).or_insert(0) += 1;
    }
    counts
}

fn decrement_count(
    counts: &mut BTreeMap<WorkspaceWatchTarget, usize>,
    target: &WorkspaceWatchTarget,
) {
    let count = counts
        .get_mut(target)
        .expect("removed row contribution must have a reference count");
    *count -= 1;
    if *count == 0 {
        counts.remove(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::path::PathBuf;

    fn target(id: u8) -> WorkspaceWatchTarget {
        WorkspaceWatchTarget::directory(PathBuf::from(format!("/workspace/{id}")))
    }

    fn oracle(rows: &[RowWatchContribution]) -> Vec<WorkspaceWatchTarget> {
        count_targets(rows.iter().cloned()).into_keys().collect()
    }

    #[test]
    fn overlapping_rows_release_only_the_removed_reference() {
        let mut state = MaterializedWatchTargets::default();
        assert!(state.mount(vec![Some(target(1)), Some(target(1)), Some(target(2))]));
        let generation = state.snapshot().generation;

        assert!(!state.splice(0, 1, &[]));
        assert_eq!(state.snapshot().generation, generation);
        assert_eq!(state.snapshot().targets, vec![target(1), target(2)]);

        assert!(state.splice(0, 1, &[]));
        assert_eq!(state.snapshot().targets, vec![target(2)]);
    }

    #[test]
    fn fallback_applies_only_before_a_model_is_mounted() {
        let mut state = MaterializedWatchTargets::default();
        assert!(state.set_fallback(vec![target(2), target(1), target(1)]));
        assert_eq!(state.snapshot().targets, vec![target(1), target(2)]);

        assert!(state.mount(Vec::new()));
        assert!(state.snapshot().targets.is_empty());

        assert!(state.unmount());
        assert_eq!(state.snapshot().targets, vec![target(1), target(2)]);
    }

    #[test]
    fn unchanged_row_state_does_not_advance_generation() {
        let mut state = MaterializedWatchTargets::default();
        state.mount(vec![Some(target(1))]);
        let generation = state.snapshot().generation;
        assert!(!state.update_row(0, Some(target(1))));
        assert_eq!(state.snapshot().generation, generation);
    }

    proptest! {
        #[test]
        fn generated_splices_match_full_derivation_oracle(
            initial in prop::collection::vec(prop::option::of(0u8..8), 0..64),
            operations in prop::collection::vec((any::<u8>(), any::<u8>(), prop::collection::vec(prop::option::of(0u8..8), 0..8)), 0..128),
        ) {
            let mut rows = initial.into_iter().map(|id| id.map(target)).collect::<Vec<_>>();
            let mut state = MaterializedWatchTargets::default();
            state.mount(rows.clone());
            prop_assert_eq!(state.snapshot().targets, oracle(&rows));

            for (raw_position, raw_removed, added) in operations {
                let position = usize::from(raw_position) % (rows.len() + 1);
                let removable = rows.len() - position;
                let removed = usize::from(raw_removed).min(removable);
                let added = added.into_iter().map(|id| id.map(target)).collect::<Vec<_>>();
                rows.splice(position..position + removed, added.iter().cloned());
                state.splice(position, removed, &added);
                prop_assert_eq!(state.snapshot().targets, oracle(&rows));
            }
        }
    }
}
