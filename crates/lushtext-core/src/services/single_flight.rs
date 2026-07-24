// SPDX-License-Identifier: GPL-3.0-or-later

//! Workflow-neutral one-active/one-latest single-flight coordination.
//!
//! Several workflows keep at most one active background request and at most one
//! latest superseding request: command-palette search, notes browsing, bookmark
//! excerpt previews, local-history preview selection, and workspace content
//! search. They all share this GTK-free primitive rather than re-implementing
//! submit/finish/supersede generation semantics per workflow. Palette search
//! keeps its palette-named aliases in `services::palette::runtime`; non-palette
//! consumers depend on these neutral names directly.

use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative cancellation token scoped to one active generation.
#[derive(Clone, Debug, Default)]
pub struct FlightCancellation {
    cancelled: Arc<AtomicBool>,
    #[cfg(test)]
    cancel_after_checks: Arc<AtomicUsize>,
    #[cfg(test)]
    checks: Arc<AtomicUsize>,
}

impl FlightCancellation {
    /// Request cancellation and report whether this is the first request.
    #[must_use]
    pub fn cancel(&self) -> bool {
        !self.cancelled.swap(true, Ordering::Relaxed)
    }

    /// Return whether the owning generation has been superseded.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        #[cfg(test)]
        {
            let cancel_after = self.cancel_after_checks.load(Ordering::Relaxed);
            if cancel_after > 0
                && self
                    .checks
                    .fetch_add(1, Ordering::Relaxed)
                    .saturating_add(1)
                    >= cancel_after
            {
                self.cancelled.store(true, Ordering::Relaxed);
            }
        }
        self.cancelled.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn cancel_after_checks_for_test(&self, checks: usize) {
        self.checks.store(0, Ordering::Relaxed);
        self.cancel_after_checks
            .store(checks.max(1), Ordering::Relaxed);
    }
}

/// One request admitted as the coordinator's sole active generation.
#[derive(Debug)]
pub struct SingleFlightStart<R> {
    pub generation: u64,
    pub request: R,
    pub cancellation: FlightCancellation,
}

#[derive(Debug)]
struct ActiveFlight {
    generation: u64,
    cancellation: FlightCancellation,
}

#[derive(Debug)]
struct PendingFlight<R> {
    generation: u64,
    request: R,
}

/// Scalar ownership snapshot used by tests and benchmark evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SingleFlightSnapshot {
    pub active: usize,
    pub pending: usize,
    pub active_high_water: usize,
    pub pending_high_water: usize,
    pub started: usize,
    pub cancellation_requests: usize,
}

/// Retain at most one active request and one latest superseding request.
#[derive(Debug)]
pub struct SingleFlightCoordinator<R> {
    current_generation: u64,
    active: Option<ActiveFlight>,
    pending: Option<PendingFlight<R>>,
    snapshot: SingleFlightSnapshot,
}

impl<R> Default for SingleFlightCoordinator<R> {
    fn default() -> Self {
        Self {
            current_generation: 0,
            active: None,
            pending: None,
            snapshot: SingleFlightSnapshot::default(),
        }
    }
}

impl<R> SingleFlightCoordinator<R> {
    /// Submit a request, starting it only when no older generation owns the worker.
    pub fn submit(&mut self, request: R) -> Option<SingleFlightStart<R>> {
        let generation = self.advance_generation();
        if let Some(active) = self.active.as_ref() {
            if active.cancellation.cancel() {
                self.snapshot.cancellation_requests =
                    self.snapshot.cancellation_requests.saturating_add(1);
            }
            self.pending = Some(PendingFlight {
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
    pub fn finish(&mut self, generation: u64) -> Option<SingleFlightStart<R>> {
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

    /// Drop only the latest pending request, leaving the active generation to
    /// drain on its own. Unlike [`invalidate`](Self::invalidate) this does not
    /// advance the generation or cancel the active token.
    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    /// Return whether active or latest work still belongs to the coordinator.
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }

    /// Return the currently active generation, if one owns the worker.
    #[must_use]
    pub fn active_generation(&self) -> Option<u64> {
        self.active.as_ref().map(|active| active.generation)
    }

    /// Return whether a completion still belongs to the latest requested generation.
    #[must_use]
    pub fn is_current(&self, generation: u64) -> bool {
        self.current_generation == generation
    }

    /// Return scalar ownership and high-water evidence without exposing requests.
    #[must_use]
    pub fn snapshot(&self) -> SingleFlightSnapshot {
        SingleFlightSnapshot {
            active: usize::from(self.active.is_some()),
            pending: usize::from(self.pending.is_some()),
            ..self.snapshot
        }
    }

    fn advance_generation(&mut self) -> u64 {
        self.current_generation = self.current_generation.wrapping_add(1);
        self.current_generation
    }

    fn start(&mut self, generation: u64, request: R) -> SingleFlightStart<R> {
        let cancellation = FlightCancellation::default();
        self.active = Some(ActiveFlight {
            generation,
            cancellation: cancellation.clone(),
        });
        self.snapshot.started = self.snapshot.started.saturating_add(1);
        self.snapshot.active_high_water = self.snapshot.active_high_water.max(1);
        SingleFlightStart {
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
        let mut coordinator = SingleFlightCoordinator::default();
        let first = coordinator.submit("first").expect("first request starts");
        assert!(coordinator.submit("middle").is_none());
        assert!(coordinator.submit("latest").is_none());
        assert!(first.cancellation.is_cancelled());
        assert_eq!(coordinator.active_generation(), Some(first.generation));
        assert_eq!(
            coordinator.snapshot(),
            SingleFlightSnapshot {
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
    }

    #[test]
    fn clear_pending_drops_latest_without_cancelling_active() {
        let mut coordinator = SingleFlightCoordinator::default();
        let first = coordinator.submit("first").expect("first request starts");
        assert!(coordinator.submit("latest").is_none());
        coordinator.clear_pending();
        assert_eq!(coordinator.snapshot().pending, 0);
        assert_eq!(coordinator.active_generation(), Some(first.generation));
        assert!(coordinator.finish(first.generation).is_none());
    }
}
