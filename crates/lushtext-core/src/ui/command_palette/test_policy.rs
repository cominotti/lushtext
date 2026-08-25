// SPDX-License-Identifier: GPL-3.0-or-later

//! One test policy value for the whole command-palette workflow.
//!
//! Widget tests need to slow one of the palette's two workers down so a
//! supersession or a generation race becomes observable. Each stage order owns
//! one such delay, and each is read at exactly one point:
//! [`delay_search_worker`] on the grouped-search worker in `query_execution`,
//! and [`delay_index_update_worker`] on the index-mutation worker in
//! `index_execution`. Both are fields of [`CommandPaletteTestPolicy`], so the
//! workflow has one test policy value rather than one static per delay.
//!
//! The whole module is gated behind the `test-utils` feature, so a build without
//! that feature compiles no override storage and no override selection: the
//! workflow reads its ordinary policy values with nothing in front of them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SEARCH_WORKER_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static INDEX_UPDATE_WORKER_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Test-only timing overrides for the command-palette workflow.
///
/// A zero delay means "do not delay", so [`CommandPaletteTestPolicy::default`]
/// is the production posture and [`CommandPaletteTestPolicy::reset`] restores it
/// between tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommandPaletteTestPolicy {
    /// Artificial delay before the grouped palette search worker starts scoring.
    pub search_worker_delay: Duration,
    /// Artificial delay before the incremental index-mutation worker starts.
    pub index_update_worker_delay: Duration,
}

impl CommandPaletteTestPolicy {
    /// Read the overrides currently installed for this process.
    #[must_use]
    pub fn current() -> Self {
        Self {
            search_worker_delay: load_delay(&SEARCH_WORKER_DELAY_MS),
            index_update_worker_delay: load_delay(&INDEX_UPDATE_WORKER_DELAY_MS),
        }
    }

    /// Install this policy for the current process.
    pub fn install(self) {
        store_delay(&SEARCH_WORKER_DELAY_MS, self.search_worker_delay);
        store_delay(
            &INDEX_UPDATE_WORKER_DELAY_MS,
            self.index_update_worker_delay,
        );
    }

    /// Restore the production posture: no delays.
    pub fn reset() {
        Self::default().install();
    }

    /// Delay the grouped palette search worker.
    #[must_use]
    pub const fn with_search_worker_delay(mut self, delay: Duration) -> Self {
        self.search_worker_delay = delay;
        self
    }

    /// Delay the incremental index-mutation worker.
    #[must_use]
    pub const fn with_index_update_worker_delay(mut self, delay: Duration) -> Self {
        self.index_update_worker_delay = delay;
        self
    }
}

fn load_delay(cell: &AtomicU64) -> Duration {
    Duration::from_millis(cell.load(Ordering::Acquire))
}

fn store_delay(cell: &AtomicU64, delay: Duration) {
    cell.store(
        u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
        Ordering::Release,
    );
}

fn sleep(delay: Duration) {
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}

/// Delay grouped search worker entry for deterministic supersession tests.
pub(super) fn delay_search_worker() {
    sleep(load_delay(&SEARCH_WORKER_DELAY_MS));
}

/// Delay index-mutation worker entry for deterministic replacement-race tests.
pub(super) fn delay_index_update_worker() {
    sleep(load_delay(&INDEX_UPDATE_WORKER_DELAY_MS));
}
