// SPDX-License-Identifier: GPL-3.0-or-later

//! One test policy value for the whole search-panel workflow.
//!
//! Widget tests need to slow a worker down or shrink a budget so a race becomes
//! observable. Before this module those overrides were five module-level
//! statics with five public setters spread across the workflow's coordination
//! files, ahead of the workflow logic a reader came for. They are now fields of
//! [`SearchPanelTestPolicy`].
//!
//! The whole module is gated behind the `test-utils` feature, so a build without
//! that feature compiles no override storage and no override selection: the
//! workflow reads its ordinary policy values with nothing in front of them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::model::content_search::ReplacePreviewBudget;

static SEARCH_WORKER_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static REPLACE_PREVIEW_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static PREVIEW_SELECTION_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static UNDO_BACKUP_DISK_DELAY_MS: AtomicU64 = AtomicU64::new(0);
static REPLACE_PREVIEW_MAX_ROWS: AtomicU64 = AtomicU64::new(0);
static REPLACE_PREVIEW_MAX_BYTES: AtomicU64 = AtomicU64::new(0);

/// Test-only timing and limit overrides for the workspace search workflow.
///
/// A zero delay means "do not delay" and a zero limit means "use the production
/// budget", so [`SearchPanelTestPolicy::default`] is the production posture and
/// [`SearchPanelTestPolicy::reset`] restores it between tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchPanelTestPolicy {
    /// Artificial delay before the workspace-search worker starts walking.
    pub search_worker_delay: Duration,
    /// Artificial delay inside Replace All preview generation.
    pub replace_preview_delay: Duration,
    /// Artificial delay inside the checked-row selection partition.
    pub preview_selection_delay: Duration,
    /// Artificial delay before undo-journal persistence touches the disk.
    pub undo_backup_disk_delay: Duration,
    /// Replace All preview row ceiling; zero keeps the production budget.
    pub replace_preview_max_rows: u64,
    /// Replace All preview byte ceiling; zero keeps the production budget.
    pub replace_preview_max_bytes: u64,
}

impl SearchPanelTestPolicy {
    /// Read the overrides currently installed for this process.
    #[must_use]
    pub fn current() -> Self {
        Self {
            search_worker_delay: load_delay(&SEARCH_WORKER_DELAY_MS),
            replace_preview_delay: load_delay(&REPLACE_PREVIEW_DELAY_MS),
            preview_selection_delay: load_delay(&PREVIEW_SELECTION_DELAY_MS),
            undo_backup_disk_delay: load_delay(&UNDO_BACKUP_DISK_DELAY_MS),
            replace_preview_max_rows: REPLACE_PREVIEW_MAX_ROWS.load(Ordering::Acquire),
            replace_preview_max_bytes: REPLACE_PREVIEW_MAX_BYTES.load(Ordering::Acquire),
        }
    }

    /// Install this policy for the current process.
    pub fn install(self) {
        store_delay(&SEARCH_WORKER_DELAY_MS, self.search_worker_delay);
        store_delay(&REPLACE_PREVIEW_DELAY_MS, self.replace_preview_delay);
        store_delay(&PREVIEW_SELECTION_DELAY_MS, self.preview_selection_delay);
        store_delay(&UNDO_BACKUP_DISK_DELAY_MS, self.undo_backup_disk_delay);
        REPLACE_PREVIEW_MAX_ROWS.store(self.replace_preview_max_rows, Ordering::Release);
        REPLACE_PREVIEW_MAX_BYTES.store(self.replace_preview_max_bytes, Ordering::Release);
    }

    /// Restore the production posture: no delays and no budget overrides.
    pub fn reset() {
        Self::default().install();
    }

    #[must_use]
    pub const fn with_search_worker_delay(mut self, delay: Duration) -> Self {
        self.search_worker_delay = delay;
        self
    }

    #[must_use]
    pub const fn with_replace_preview_delay(mut self, delay: Duration) -> Self {
        self.replace_preview_delay = delay;
        self
    }

    #[must_use]
    pub const fn with_preview_selection_delay(mut self, delay: Duration) -> Self {
        self.preview_selection_delay = delay;
        self
    }

    #[must_use]
    pub const fn with_undo_backup_disk_delay(mut self, delay: Duration) -> Self {
        self.undo_backup_disk_delay = delay;
        self
    }

    /// Override the Replace All preview budget; zero on both restores production.
    #[must_use]
    pub const fn with_replace_preview_budget(mut self, max_rows: u64, max_bytes: u64) -> Self {
        self.replace_preview_max_rows = max_rows;
        self.replace_preview_max_bytes = max_bytes;
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

/// Delay worker entry for deterministic cancellation/disconnection tests.
pub(super) fn delay_search_worker() {
    sleep(load_delay(&SEARCH_WORKER_DELAY_MS));
}

/// Delay Replace All preview generation for freshness tests.
pub(super) fn delay_replace_preview() {
    sleep(load_delay(&REPLACE_PREVIEW_DELAY_MS));
}

/// Delay the checked-row selection partition for freshness tests.
pub(super) fn delay_preview_selection() {
    sleep(load_delay(&PREVIEW_SELECTION_DELAY_MS));
}

/// Delay undo-journal persistence for ordering tests.
pub(super) fn delay_undo_backup_disk() {
    sleep(load_delay(&UNDO_BACKUP_DISK_DELAY_MS));
}

/// The overridden Replace All preview budget, when a test installed one.
pub(super) fn replace_preview_budget_override() -> Option<ReplacePreviewBudget> {
    let max_rows = REPLACE_PREVIEW_MAX_ROWS.load(Ordering::Acquire);
    let max_bytes = REPLACE_PREVIEW_MAX_BYTES.load(Ordering::Acquire);
    (max_rows > 0 || max_bytes > 0).then(|| ReplacePreviewBudget {
        max_rows: usize::try_from(max_rows).unwrap_or(usize::MAX),
        max_bytes: usize::try_from(max_bytes).unwrap_or(usize::MAX),
    })
}
