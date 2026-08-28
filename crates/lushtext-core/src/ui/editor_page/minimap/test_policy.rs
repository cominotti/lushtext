// SPDX-License-Identifier: GPL-3.0-or-later

//! The minimap workflow's single test policy value and its one actuation seam.
//!
//! Test-only timing and limit overrides belong in one place per workflow rather
//! than in several independent module-level statics, and no override storage may
//! compile without the test feature — the whole module is gated.
//!
//! ## Seam dispositions
//!
//! The workflow carries **two** pre-existing test-only actuation seams, and this
//! change spends its budget of new ones on neither. Both are recorded here
//! rather than carried silently past a consolidation that only names inspection
//! seams:
//!
//! - [`LushtextEditorPage::set_after_minimap_analysis_slice_hook_for_test`] —
//!   **kept and justified.** It injects one transition *between* two bounded
//!   analysis slices, which is a window no production drive can enter: the slice
//!   loop runs to its own yield point inside a single `idle_add_local` turn, so
//!   there is no main-loop moment at which a test could edit the buffer, change
//!   a preference, or dispose the page and have the running slice observe it.
//!   Without the seam the stale-generation rejection at the post-slice recheck
//!   is unreachable, and the mid-scan cancellation proof would assert against
//!   the pre-slice guard instead — a different branch. It falls under the
//!   deferred actuation-seam taxonomy in the `gtk-testing` skill; it cannot move
//!   onto `evidence.rs`, because an evidence surface must not mutate.
//! - `mark_minimap_refresh_pending_for_test` — **retired.**
//!   Its three readiness-blocker call sites now drive a real refresh through
//!   `arm_minimap_refresh`, which is the production path that leaves a
//!   refresh genuinely pending, and read the result from the evidence surface.
//!   Nothing needed a synthetic pending flag.

use glib::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;
use super::policy::MINIMAP_ANALYSIS_CHARS_PER_SLICE;

impl LushtextEditorPage {
    /// Configured live-buffer character ceiling for one GTK analysis turn.
    #[must_use]
    pub fn minimap_analysis_slice_limit_for_test() -> usize {
        MINIMAP_ANALYSIS_CHARS_PER_SLICE
    }

    /// Inject one transition after the next bounded analysis slice.
    ///
    /// The only actuation seam this workflow keeps; see the module docs for why
    /// no production drive reaches the state it creates.
    pub fn set_after_minimap_analysis_slice_hook_for_test<F: FnOnce() + 'static>(&self, hook: F) {
        self.imp()
            .minimap
            .analysis_after_slice_hook
            .replace(Some(Box::new(hook)));
    }
}
