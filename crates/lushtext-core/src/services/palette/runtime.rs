// SPDX-License-Identifier: GPL-3.0-or-later

//! Palette search metrics/outcomes over the shared single-flight coordinator.
//!
//! The generic one-active/one-latest coordinator and its cancellation token now
//! live in [`crate::services::single_flight`]; the palette-named aliases below
//! keep palette call sites reading in palette vocabulary.

use crate::services::single_flight::{
    FlightCancellation, SingleFlightCoordinator, SingleFlightSnapshot, SingleFlightStart,
};

/// Cooperative cancellation token scoped to one active palette generation.
pub type PaletteSearchCancellation = FlightCancellation;

/// Bounded work evidence reported by one palette search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaletteSearchMetrics {
    /// Candidates visited across all source-local selectors.
    pub candidates_examined: usize,
    /// Candidates that matched before source-local top-result selection.
    pub matching_candidates: usize,
    /// Largest retained candidate count in any one source selector.
    pub peak_retained_per_source: usize,
    /// Included candidates whose non-empty query reached a scorer.
    pub candidates_scored: usize,
    /// Note bodies inspected after metadata eligibility work.
    pub note_bodies_examined: usize,
    /// Note bodies skipped because their maximum contribution could not improve the row score.
    pub note_bodies_safely_pruned: usize,
}

impl PaletteSearchMetrics {
    pub(super) fn merge(&mut self, other: Self) {
        self.candidates_examined = self
            .candidates_examined
            .saturating_add(other.candidates_examined);
        self.matching_candidates = self
            .matching_candidates
            .saturating_add(other.matching_candidates);
        self.peak_retained_per_source = self
            .peak_retained_per_source
            .max(other.peak_retained_per_source);
        self.candidates_scored = self
            .candidates_scored
            .saturating_add(other.candidates_scored);
        self.note_bodies_examined = self
            .note_bodies_examined
            .saturating_add(other.note_bodies_examined);
        self.note_bodies_safely_pruned = self
            .note_bodies_safely_pruned
            .saturating_add(other.note_bodies_safely_pruned);
    }
}

/// Typed completion from cancellable palette scoring.
#[derive(Debug)]
pub enum PaletteSearchOutcome<T> {
    Complete {
        value: T,
        metrics: PaletteSearchMetrics,
    },
    Cancelled {
        metrics: PaletteSearchMetrics,
    },
}

impl<T> PaletteSearchOutcome<T> {
    /// Return the bounded work evidence for either terminal state.
    #[must_use]
    pub fn metrics(&self) -> PaletteSearchMetrics {
        match self {
            Self::Complete { metrics, .. } | Self::Cancelled { metrics } => *metrics,
        }
    }
}

/// One request admitted as the coordinator's sole active palette generation.
pub type PaletteSearchStart<R> = SingleFlightStart<R>;

/// Scalar ownership snapshot used by tests and benchmark evidence.
pub type PaletteSearchCoordinatorSnapshot = SingleFlightSnapshot;

/// Retain at most one active palette search and one latest superseding request.
pub type PaletteSearchCoordinator<R> = SingleFlightCoordinator<R>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_search_metrics_merge_sums_and_peaks() {
        let mut metrics = PaletteSearchMetrics {
            candidates_examined: 3,
            matching_candidates: 2,
            peak_retained_per_source: 4,
            candidates_scored: 1,
            note_bodies_examined: 5,
            note_bodies_safely_pruned: 2,
        };
        metrics.merge(PaletteSearchMetrics {
            candidates_examined: 7,
            matching_candidates: 1,
            peak_retained_per_source: 9,
            candidates_scored: 6,
            note_bodies_examined: 4,
            note_bodies_safely_pruned: 3,
        });
        assert_eq!(metrics.candidates_examined, 10);
        assert_eq!(metrics.matching_candidates, 3);
        assert_eq!(metrics.peak_retained_per_source, 9);
        assert_eq!(metrics.candidates_scored, 7);
        assert_eq!(metrics.note_bodies_examined, 9);
        assert_eq!(metrics.note_bodies_safely_pruned, 5);
    }

    #[test]
    fn invalidation_cancels_active_and_discards_pending() {
        let mut coordinator = PaletteSearchCoordinator::default();
        let active = coordinator.submit(1).expect("first request starts");
        coordinator.submit(2);

        coordinator.invalidate();

        assert!(active.cancellation.is_cancelled());
        assert_eq!(coordinator.snapshot().pending, 0);
        assert!(coordinator.finish(active.generation).is_none());
        assert!(!coordinator.has_work());
    }
}
