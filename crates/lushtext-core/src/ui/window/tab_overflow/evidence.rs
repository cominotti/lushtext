// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — `WFR-TAB-OVERFLOW-NAVIGATION`'s single observable
//! state, a `test-utils` view of the production `TabOverflowState` that the
//! readiness blocker and the automation snapshot read too (design D9), so the
//! three cannot drift.
//!
//! * **Reading must not mutate.** Every field is a `Cell` read, a slot's
//!   presence, an action's enabled state or an adjustment read. No field
//!   schedules a reconcile, starts or stops a step, or moves the strip.
//! * **No field may be read from inside a mutable borrow.** Each `Ref` taken
//!   here is dropped before the struct literal is built, so the accessor is
//!   callable between any two stages, including from a cancellation observer.
//! * **A disposed widget is a stage.** `tab_bar` and `tab_view` are template
//!   children, reached through `try_get()`.
//!
//! **No materialization.** The strip's tabs live in an `AdwTabBox` that holds
//! every page eagerly; this surface reads only the scroller's adjustment and
//! the workflow's own cells, so no read brings anything into being.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::super::LushtextWindow;
use super::controls::{SCROLL_BACK_ACTION, SCROLL_FORWARD_ACTION};
use super::policy::StripScrollExtent;
use super::scroll_execution::{extent_at, is_rtl};

/// Everything a test may observe about the tab strip's overflow controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabOverflowEvidence {
    /// Whether the lookup found the unpinned tab scroller.
    pub scroller_found: bool,
    /// Whether the scroll buttons show.
    pub controls_visible: bool,
    /// How many times the scroll buttons appeared.
    pub controls_shown_count: u32,
    /// Whether the end group is the bar's end action widget.
    pub group_attached: bool,
    /// Whether Back may move.
    pub can_scroll_back: bool,
    /// Whether Forward may move.
    pub can_scroll_forward: bool,
    /// `win.scroll-tabs-back`'s and `win.scroll-tabs-forward`'s enabled
    /// states.
    pub scroll_actions_enabled: [bool; 2],
    /// Whether our step animation is playing.
    pub animation_active: bool,
    /// Whether a hold is repeating.
    pub repeat_active: bool,
    /// Whether a reconcile is scheduled or waits for a projection batch.
    pub reconcile_pending: bool,
    /// Steps and holds the strip's own input cancelled.
    pub guard_cancellations: u32,
    /// The unpinned adjustment in whole pixels, when the scroller exists.
    pub extent: Option<StripScrollExtent>,
    /// Whether the bar lays out right to left.
    pub rtl: bool,
    /// The bar's `is-overflowing`.
    pub bar_overflowing: bool,
    /// The bar's allocated height, 0 once it is gone.
    pub bar_height: i32,
    /// The Back and Forward buttons, for a test that drives their gestures.
    pub buttons: Option<(gtk4::Button, gtk4::Button)>,
    /// The end group, when built.
    pub group: Option<gtk4::Box>,
}

/// Read the whole surface.
#[must_use]
pub fn tab_overflow_evidence(window: &LushtextWindow) -> TabOverflowEvidence {
    let imp = window.imp();
    let state = &imp.tab_overflow;
    let unpinned = state.unpinned.borrow().clone();
    let controls = state.controls.borrow().clone();
    let animation_active = state.animation.borrow().as_ref().is_some_and(|animation| {
        libadwaita::prelude::AnimationExt::state(animation) == libadwaita::AnimationState::Playing
    });
    let repeat_active = state.repeat.borrow().is_some();
    let bar = imp.tab_bar.try_get();
    let action_enabled = |name: &str| {
        window
            .lookup_action(name)
            .is_some_and(|action| action.is_enabled())
    };
    TabOverflowEvidence {
        scroller_found: unpinned.is_some(),
        controls_visible: state.controls_visible.get(),
        controls_shown_count: state.controls_shown_count.get(),
        group_attached: state.group_attached.get(),
        can_scroll_back: state.can_scroll_back.get(),
        can_scroll_forward: state.can_scroll_forward.get(),
        scroll_actions_enabled: [
            action_enabled(SCROLL_BACK_ACTION),
            action_enabled(SCROLL_FORWARD_ACTION),
        ],
        animation_active,
        repeat_active,
        reconcile_pending: state.reconcile.pending() || state.waiting_for_terminal.get(),
        guard_cancellations: state.guard_cancellations.get(),
        extent: unpinned.map(|scroller| {
            let adjustment = scroller.hadjustment();
            extent_at(&adjustment, adjustment.value())
        }),
        rtl: bar.as_ref().is_some_and(is_rtl),
        bar_overflowing: bar.as_ref().is_some_and(libadwaita::TabBar::is_overflowing),
        bar_height: bar.as_ref().map_or(0, WidgetExt::height),
        buttons: controls
            .as_ref()
            .map(|controls| (controls.back.clone(), controls.forward.clone())),
        group: controls.map(|controls| controls.group),
    }
}
