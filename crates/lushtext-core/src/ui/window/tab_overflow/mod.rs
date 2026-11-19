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
//!    repeat, so the strip's own scroll wins; a wheel or press also ends a
//!    selection reveal.
//! 5. **Reveal the selection** (`scroll_execution.rs`): the session
//!    restore's terminal, and an All Tabs pick of the already selected tab,
//!    ask the strip to bring the selected tab into view through its own
//!    selection scrolling (ledger A133).
//!    **Inversion:** it resumes from a tick callback on the bar, once per
//!    frame after the strip is allocated, until the strip rests; while the
//!    strip is hidden (Focus Mode) it first resumes from the bar's one-shot
//!    `map` handler.
//! 6. **List → filter → select** (`all_tabs.rs`, `all_tabs_popover/`): the
//!    All Tabs button opens a popover over a snapshot of the open tabs; a
//!    query ranks them (`policy::rank_all_tabs`); an activated row selects
//!    its tab and focuses its editor, and a close without one gives the
//!    focus back. **Inversion:** control leaves the popover through its
//!    row-activation callback and resumes in `select_tab_from_all_tabs`;
//!    an open popover's re-snapshot resumes from a zero-delay `Debounce`.
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
//! | `all_tabs.rs` | **called presentation surface**: the popover's snapshot, popup wiring, focus save and restore, and `select_tab_from_all_tabs` |
//! | `all_tabs_popover/` | **called presentation surfaces**: the popover widget, its row model and row object |

pub mod all_tabs_popover;
pub mod policy;

mod all_tabs;
mod controls;
mod scroll_execution;
mod watch;

#[cfg(feature = "test-utils")]
pub mod evidence;
#[cfg(feature = "test-utils")]
pub mod test_policy;

pub use scroll_execution::find_tab_scroller;
pub(crate) use watch::TabOverflowState;

use gtk4::prelude::WidgetExt;

use super::LushtextWindow;
use policy::ScrollDirection;

/// The strip's controls that show, for the visual-geometry surfaces.
#[derive(Clone, Debug, Default)]
pub(crate) struct VisibleTabOverflowControls {
    /// The end group, while it is attached.
    pub(crate) group: Option<gtk4::Box>,
    /// Back and Forward, while they show.
    pub(crate) scroll_buttons: Option<(gtk4::Button, gtk4::Button)>,
    /// The All Tabs button, while it shows.
    pub(crate) all_tabs: Option<gtk4::MenuButton>,
    /// The All Tabs popover, while it shows.
    pub(crate) popover: Option<all_tabs_popover::LushtextAllTabsPopover>,
}

/// The strip's overflow flags, read from the workflow's production state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TabStripOverflowFacts {
    /// Whether the unpinned tabs overflow.
    pub(crate) overflowing: bool,
    /// Whether Back may move.
    pub(crate) scroll_back_enabled: bool,
    /// Whether Forward may move.
    pub(crate) scroll_forward_enabled: bool,
    /// Whether the strip shows the whole selected tab, when that applies.
    pub(crate) selected_tab_in_view: Option<bool>,
}

impl LushtextWindow {
    /// Entry point of `win.scroll-tabs-back` and `win.scroll-tabs-forward`:
    /// one step, no repeat (keyboard activation and automation).
    pub(super) fn scroll_tab_strip(&self, direction: ScrollDirection) {
        let _ = self.step_tab_strip(direction);
    }

    /// Entry point of `win.toggle-all-tabs` (`Ctrl+Shift+O`).
    pub(super) fn toggle_all_tabs_popover(&self) {
        self.toggle_all_tabs();
    }

    /// Entry point of `win.set-all-tabs-visible`.
    pub(super) fn set_all_tabs_popover_visible(&self, visible: bool) {
        self.set_all_tabs_visible(visible);
    }

    /// Entry point of `win.set-all-tabs-query`.
    pub(super) fn set_all_tabs_popover_query(&self, query: &str) {
        self.set_all_tabs_query(query);
    }

    /// The strip's overflow flags the automation snapshot reports.
    pub(crate) fn tab_strip_overflow_facts(&self) -> TabStripOverflowFacts {
        let state = &glib::subclass::prelude::ObjectSubclassIsExt::imp(self).tab_overflow;
        TabStripOverflowFacts {
            overflowing: state.unpinned_overflowing.get(),
            scroll_back_enabled: state.can_scroll_back.get(),
            scroll_forward_enabled: state.can_scroll_forward.get(),
            selected_tab_in_view: self.selected_tab_in_view(),
        }
    }

    /// The strip's controls as they show, for the visual-geometry surfaces.
    pub(crate) fn visible_tab_overflow_controls(&self) -> VisibleTabOverflowControls {
        let state = &glib::subclass::prelude::ObjectSubclassIsExt::imp(self).tab_overflow;
        let controls = state.controls.borrow().clone();
        let Some(controls) = controls.filter(|_| state.group_attached.get()) else {
            return VisibleTabOverflowControls::default();
        };
        VisibleTabOverflowControls {
            scroll_buttons: state
                .controls_visible
                .get()
                .then(|| (controls.back.clone(), controls.forward.clone())),
            all_tabs: state
                .all_tabs_visible
                .get()
                .then(|| controls.all_tabs.clone()),
            popover: controls
                .popover
                .is_visible()
                .then(|| controls.popover.clone()),
            group: Some(controls.group),
        }
    }

    /// Whether the readiness blocker `tab-overflow` holds.
    pub(crate) fn tab_overflow_blocks_readiness(&self) -> bool {
        glib::subclass::prelude::ObjectSubclassIsExt::imp(self)
            .tab_overflow
            .blocks_readiness()
    }
}
