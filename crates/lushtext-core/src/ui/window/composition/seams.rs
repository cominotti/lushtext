// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: seam value objects of the window-composition workflow.
//!
//! [`PendingWindowRequest`] crosses four boundaries — the application's `open`
//! or `activate` handler, stage W, stage C, and stage B1 — so it is one value
//! constructed once, in the handler, rather than files, a hold and a start time
//! passed separately through every idle closure.

use gtk4::gio;

/// One window the application was asked for and has not constructed yet.
///
/// `open` and `activate` record it and return without window work, so the
/// main loop runs before any of it (design D1). A later request that arrives
/// before stage C merges into it instead of asking for a second window.
pub struct PendingWindowRequest {
    /// Files the activation asked to open, in order, merged across requests.
    pub(super) files: Vec<gio::File>,
    /// Keeps the application running until the window exists: a
    /// `GApplication` exits a main loop that holds no use count, and a
    /// `GtkApplication`'s use count comes from its windows (finding 1).
    /// Dropped in stage C once the window is added, or when the process quits
    /// first.
    pub(super) hold: gio::ApplicationHoldGuard,
    /// When the first request arrived (`g_get_monotonic_time()`), the origin of
    /// the window's first-frame and composition-complete spans.
    pub(super) requested_us: i64,
}

impl PendingWindowRequest {
    /// A request for `files` (empty for `activate`), holding the application.
    pub(super) fn new(
        files: Vec<gio::File>,
        hold: gio::ApplicationHoldGuard,
        requested_us: i64,
    ) -> Self {
        Self {
            files,
            hold,
            requested_us,
        }
    }

    /// Merge a later request's files: one window opens all of them, in the
    /// order the requests arrived.
    pub(super) fn merge(&mut self, files: impl IntoIterator<Item = gio::File>) {
        self.files.extend(files);
    }

    /// How many files the request carries.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}
