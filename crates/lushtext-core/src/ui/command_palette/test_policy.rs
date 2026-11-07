// SPDX-License-Identifier: GPL-3.0-or-later

//! One test policy value for the whole command-palette workflow.
//!
//! Two row-count overrides (`results_per_source`, `category_row_limit`) let a
//! test force a "More" row and a category budget cut on small fixtures
//! (`drill-down-palette-categories`); both are read once per flight in
//! `query_execution`.
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

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

static SEARCH_WORKER_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static INDEX_UPDATE_WORKER_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static RESULTS_PER_SOURCE: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_ROW_LIMIT: AtomicUsize = AtomicUsize::new(0);

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
    /// Per-group row cap of the grouped view in place of
    /// `MAX_RESULTS_PER_SOURCE` (0: the production cap), so a test can force
    /// a "More" row on any group.
    pub results_per_source: usize,
    /// Row limit of a category view in place of the retained-byte budget's
    /// (0: the production limit), so a test can force a budget cut.
    pub category_row_limit: usize,
}

impl CommandPaletteTestPolicy {
    /// Read the overrides currently installed for this process.
    #[must_use]
    pub fn current() -> Self {
        Self {
            search_worker_delay: load_delay(&SEARCH_WORKER_DELAY_MS),
            index_update_worker_delay: load_delay(&INDEX_UPDATE_WORKER_DELAY_MS),
            results_per_source: RESULTS_PER_SOURCE.load(Ordering::Acquire),
            category_row_limit: CATEGORY_ROW_LIMIT.load(Ordering::Acquire),
        }
    }

    /// Install this policy for the current process.
    pub fn install(self) {
        store_delay(&SEARCH_WORKER_DELAY_MS, self.search_worker_delay);
        store_delay(
            &INDEX_UPDATE_WORKER_DELAY_MS,
            self.index_update_worker_delay,
        );
        RESULTS_PER_SOURCE.store(self.results_per_source, Ordering::Release);
        CATEGORY_ROW_LIMIT.store(self.category_row_limit, Ordering::Release);
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

    /// Cap each grouped group at `rows` rows.
    #[must_use]
    pub const fn with_results_per_source(mut self, rows: usize) -> Self {
        self.results_per_source = rows;
        self
    }

    /// Limit a category view to `rows` rows.
    #[must_use]
    pub const fn with_category_row_limit(mut self, rows: usize) -> Self {
        self.category_row_limit = rows;
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

/// The grouped view's per-group cap override, when one is installed.
pub(super) fn results_per_source_override() -> Option<usize> {
    let rows = RESULTS_PER_SOURCE.load(Ordering::Acquire);
    (rows != 0).then_some(rows)
}

/// The category view's row-limit override, when one is installed.
pub(super) fn category_row_limit_override() -> Option<usize> {
    let rows = CATEGORY_ROW_LIMIT.load(Ordering::Acquire);
    (rows != 0).then_some(rows)
}
