// SPDX-License-Identifier: GPL-3.0-or-later

//! The tab-strip overflow workflow (`WFR-TAB-OVERFLOW-NAVIGATION`) — narrative
//! facade.
//!
//! When the open tabs do not fit the strip, the user gets visible controls to
//! move it. The stages:
//!
//! 1. **Install** (`watch.rs`, once per window): find the unpinned tab
//!    scroller inside `AdwTabBar` (ledger A132; fail closed when the
//!    template has another shape), build the trailing control group
//!    (`controls.rs`), and connect the observers.
//! 2. **Observe overflow → reconcile the controls** (`watch.rs`): the
//!    adjustments and `is-overflowing` notify inside the bar's allocation
//!    (A133), so the observer only schedules a `SettleBurst`. **Inversion:**
//!    control resumes in the reconcile, from a zero-delay timeout; during a
//!    tab-model projection batch it resumes at the batch's terminal,
//!    `refresh_tab_model_projections`. The reconcile decides visibility,
//!    enablement, attachment and the focus hand-off from `policy.rs`.
//! 3. **Scroll** (`scroll_execution.rs`): `win.scroll-tabs-back` /
//!    `win.scroll-tabs-forward` and a pointer hold step the unpinned
//!    adjustment by its page increment in a 200 ms animation. **Inversion:**
//!    a hold resumes from its repeat timeout; every step resumes the
//!    reconcile through the adjustment's `value-changed`.
//! 4. **Yield to the strip** (`scroll_execution.rs`, driven by `watch.rs`):
//!    a selection, page attach or detach, reorder, transfer, direction
//!    change, wheel or press on the strip pauses our step and drops the
//!    repeat, so the strip's own scroll wins.
//!
//! # Role home
//!
//! **Per-workflow subdirectory** `ui/window/tab_overflow/`: `ui/window/`
//! hosts many workflows and its flat `policy.rs` / `evidence.rs` names are
//! taken.
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy.rs` | pure policy (whole pixels) |
//! | `watch.rs` | coordination (`watch`): observers, the reconcile, the projection deferral |
//! | `scroll_execution.rs` | coordination (stage-qualified `execution`): lookup, step animation, hold repeat, cancellation |
//! | `evidence.rs` | evidence surface (`test-utils`) |
//! | `test_policy.rs` | test-only timing (`test-utils`) |
//! | `controls.rs` | **called presentation surface**: the end group, buttons and focus moves |

pub mod policy;

mod controls;
mod scroll_execution;
mod watch;

#[cfg(feature = "test-utils")]
pub mod evidence;
#[cfg(feature = "test-utils")]
pub mod test_policy;

pub use scroll_execution::find_tab_scroller;
pub(crate) use watch::TabOverflowState;

use super::LushtextWindow;
use policy::ScrollDirection;

/// The strip's overflow flags, read from the workflow's production state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TabStripOverflowFacts {
    /// Whether the unpinned tabs overflow.
    pub(crate) overflowing: bool,
    /// Whether Back may move.
    pub(crate) scroll_back_enabled: bool,
    /// Whether Forward may move.
    pub(crate) scroll_forward_enabled: bool,
}

impl LushtextWindow {
    /// Entry point of `win.scroll-tabs-back` and `win.scroll-tabs-forward`:
    /// one step, no repeat (keyboard activation and automation).
    pub(super) fn scroll_tab_strip(&self, direction: ScrollDirection) {
        let _ = self.step_tab_strip(direction);
    }

    /// The strip's overflow flags the automation snapshot reports.
    pub(crate) fn tab_strip_overflow_facts(&self) -> TabStripOverflowFacts {
        let state = &glib::subclass::prelude::ObjectSubclassIsExt::imp(self).tab_overflow;
        TabStripOverflowFacts {
            overflowing: state.unpinned_overflowing.get(),
            scroll_back_enabled: state.can_scroll_back.get(),
            scroll_forward_enabled: state.can_scroll_forward.get(),
        }
    }

    /// The end group and the Back and Forward buttons while the scroll
    /// buttons show, for the visual-geometry surfaces.
    pub(crate) fn visible_tab_overflow_controls(
        &self,
    ) -> Option<(gtk4::Box, gtk4::Button, gtk4::Button)> {
        let state = &glib::subclass::prelude::ObjectSubclassIsExt::imp(self).tab_overflow;
        if !state.controls_visible.get() {
            return None;
        }
        state.controls.borrow().as_ref().map(|controls| {
            (
                controls.group.clone(),
                controls.back.clone(),
                controls.forward.clone(),
            )
        })
    }

    /// Whether the readiness blocker `tab-overflow` holds.
    pub(crate) fn tab_overflow_blocks_readiness(&self) -> bool {
        glib::subclass::prelude::ObjectSubclassIsExt::imp(self)
            .tab_overflow
            .blocks_readiness()
    }
}
