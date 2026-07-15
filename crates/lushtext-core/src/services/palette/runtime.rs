// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free cancellation and one-active/one-latest palette search ownership.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative cancellation token scoped to one active palette generation.
#[derive(Clone, Debug, Default)]
pub struct PaletteSearchCancellation {
    cancelled: Arc<AtomicBool>,
}

impl PaletteSearchCancellation {
    /// Request cancellation and report whether this is the first request.
    #[must_use]
    pub fn cancel(&self) -> bool {
        !self.cancelled.swap(true, Ordering::Relaxed)
    }

    /// Return whether the owning generation has been superseded.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Bounded work evidence reported by one palette search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaletteSearchMetrics {
    /// Candidates visited across all source-local selectors.
    pub candidates_examined: usize,
    /// Candidates that matched before source-local top-result selection.
    pub matching_candidates: usize,
    /// Largest retained candidate count in any one source selector.
    pub peak_retained_per_source: usize,
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

/// One request admitted as the coordinator's sole active generation.
#[derive(Debug)]
pub struct PaletteSearchStart<R> {
    pub generation: u64,
    pub request: R,
    pub cancellation: PaletteSearchCancellation,
}

#[derive(Debug)]
struct ActivePaletteSearch {
    generation: u64,
    cancellation: PaletteSearchCancellation,
}

#[derive(Debug)]
struct PendingPaletteSearch<R> {
    generation: u64,
    request: R,
}

/// Scalar ownership snapshot used by tests and benchmark evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaletteSearchCoordinatorSnapshot {
    pub active: usize,
    pub pending: usize,
    pub active_high_water: usize,
    pub pending_high_water: usize,
    pub started: usize,
    pub cancellation_requests: usize,
}

/// Retain at most one active palette search and one latest superseding request.
#[derive(Debug)]
pub struct PaletteSearchCoordinator<R> {
    current_generation: u64,
    active: Option<ActivePaletteSearch>,
    pending: Option<PendingPaletteSearch<R>>,
    snapshot: PaletteSearchCoordinatorSnapshot,
}

impl<R> Default for PaletteSearchCoordinator<R> {
    fn default() -> Self {
        Self {
            current_generation: 0,
            active: None,
            pending: None,
            snapshot: PaletteSearchCoordinatorSnapshot::default(),
        }
    }
}

impl<R> PaletteSearchCoordinator<R> {
    /// Submit a request, starting it only when no older generation owns the worker.
    pub fn submit(&mut self, request: R) -> Option<PaletteSearchStart<R>> {
        let generation = self.advance_generation();
        if let Some(active) = self.active.as_ref() {
            if active.cancellation.cancel() {
                self.snapshot.cancellation_requests =
                    self.snapshot.cancellation_requests.saturating_add(1);
            }
            self.pending = Some(PendingPaletteSearch {
                generation,
                request,
            });
            self.snapshot.pending_high_water = self.snapshot.pending_high_water.max(1);
            None
        } else {
            Some(self.start(generation, request))
        }
    }

    /// Release one completed active generation and start only the latest request.
    pub fn finish(&mut self, generation: u64) -> Option<PaletteSearchStart<R>> {
        if self.active.as_ref().map(|active| active.generation) != Some(generation) {
            return None;
        }
        self.active = None;
        self.pending
            .take()
            .map(|pending| self.start(pending.generation, pending.request))
    }

    /// Invalidate visible work while retaining only the active token until completion.
    pub fn invalidate(&mut self) {
        self.advance_generation();
        if let Some(active) = self.active.as_ref()
            && active.cancellation.cancel()
        {
            self.snapshot.cancellation_requests =
                self.snapshot.cancellation_requests.saturating_add(1);
        }
        self.pending = None;
    }

    /// Return whether active or latest work still belongs to the coordinator.
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }

    /// Return whether a completion still belongs to the latest requested generation.
    #[must_use]
    pub fn is_current(&self, generation: u64) -> bool {
        self.current_generation == generation
    }

    /// Return scalar ownership and high-water evidence without exposing requests.
    #[must_use]
    pub fn snapshot(&self) -> PaletteSearchCoordinatorSnapshot {
        PaletteSearchCoordinatorSnapshot {
            active: usize::from(self.active.is_some()),
            pending: usize::from(self.pending.is_some()),
            ..self.snapshot
        }
    }

    fn advance_generation(&mut self) -> u64 {
        self.current_generation = self.current_generation.wrapping_add(1);
        self.current_generation
    }

    fn start(&mut self, generation: u64, request: R) -> PaletteSearchStart<R> {
        let cancellation = PaletteSearchCancellation::default();
        self.active = Some(ActivePaletteSearch {
            generation,
            cancellation: cancellation.clone(),
        });
        self.snapshot.started = self.snapshot.started.saturating_add(1);
        self.snapshot.active_high_water = self.snapshot.active_high_water.max(1);
        PaletteSearchStart {
            generation,
            request,
            cancellation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_retains_only_active_and_latest_request() {
        let mut coordinator = PaletteSearchCoordinator::default();
        let first = coordinator.submit("first").expect("first request starts");
        assert!(coordinator.submit("middle").is_none());
        assert!(coordinator.submit("latest").is_none());
        assert!(first.cancellation.is_cancelled());
        assert_eq!(
            coordinator.snapshot(),
            PaletteSearchCoordinatorSnapshot {
                active: 1,
                pending: 1,
                active_high_water: 1,
                pending_high_water: 1,
                started: 1,
                cancellation_requests: 1,
            }
        );

        let latest = coordinator
            .finish(first.generation)
            .expect("latest request starts after active finishes");
        assert_eq!(latest.request, "latest");
        assert_eq!(coordinator.snapshot().started, 2);
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
