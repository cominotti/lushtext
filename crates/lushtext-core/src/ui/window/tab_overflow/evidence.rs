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
//! the workflow's own cells. The All Tabs popover's list is a lazy model whose
//! `item()` creates a row object: the surface reads its lengths, its
//! selection's position and the popover's own cells, never an item, so no
//! read brings a row object into being.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::super::LushtextWindow;
use super::all_tabs::{
    SET_ALL_TABS_QUERY_ACTION, SET_ALL_TABS_VISIBLE_ACTION, TOGGLE_ALL_TABS_ACTION,
};
use super::all_tabs_popover::LushtextAllTabsPopover;
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
    /// Whether a reconcile found the tab model in a projection batch and waits
    /// for the batch's terminal (part of `reconcile_pending`).
    pub reconcile_waits_for_terminal: bool,
    /// Steps and holds the strip's own input cancelled.
    pub guard_cancellations: u32,
    /// Whether the reveal of the selected tab after a restore still runs.
    pub selection_reveal_pending: bool,
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
    /// Whether the All Tabs button shows.
    pub all_tabs_button_visible: bool,
    /// The count the All Tabs button shows.
    pub all_tabs_count: u32,
    /// Whether `win.toggle-all-tabs` and `win.set-all-tabs-visible` are
    /// enabled, and `win.set-all-tabs-query`.
    pub all_tabs_actions_enabled: [bool; 3],
    /// Whether the All Tabs popover shows.
    pub popover_visible: bool,
    /// Rows the popover's query lists, listed or still growing.
    pub popover_rows: u32,
    /// Rows the popover's list is told about.
    pub popover_listed_rows: u32,
    /// The popover query's length in characters.
    pub popover_query_len: u32,
    /// The highlighted row's position.
    pub popover_highlighted_position: Option<u32>,
    /// Whether the popover's staged first publish still grows.
    pub row_growth_pending: bool,
    /// Whether the popover's realized rows are still being retired.
    pub row_retirement_pending: bool,
    /// Whether the popover shows its no-match state.
    pub popover_empty_state_visible: bool,
    /// Match-count announcements the popover made.
    pub popover_announcements: u32,
    /// Whether a match-count announcement waits for the query to settle.
    pub popover_announcement_pending: bool,
    /// Row objects the popover's list model handed out and still alive.
    pub popover_row_objects: u32,
    /// How many times the All Tabs count changed.
    pub all_tabs_count_publications: u32,
    /// The All Tabs button and its popover, for a test that drives them.
    pub all_tabs: Option<(gtk4::MenuButton, LushtextAllTabsPopover)>,
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
    let popover = controls.as_ref().map(|controls| controls.popover.facts());
    let popover_visible = controls
        .as_ref()
        .is_some_and(|controls| controls.popover.is_visible());
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
        reconcile_waits_for_terminal: state.waiting_for_terminal.get(),
        guard_cancellations: state.guard_cancellations.get(),
        selection_reveal_pending: state.selection_reveal.borrow().is_some(),
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
        group: controls.as_ref().map(|controls| controls.group.clone()),
        all_tabs_button_visible: state.all_tabs_visible.get(),
        all_tabs_count: state.all_tabs_count.get(),
        all_tabs_actions_enabled: [
            action_enabled(TOGGLE_ALL_TABS_ACTION),
            action_enabled(SET_ALL_TABS_VISIBLE_ACTION),
            action_enabled(SET_ALL_TABS_QUERY_ACTION),
        ],
        popover_visible,
        popover_rows: popover.map_or(0, |facts| facts.rows),
        popover_listed_rows: popover.map_or(0, |facts| facts.listed_rows),
        popover_query_len: popover.map_or(0, |facts| facts.query_len),
        popover_highlighted_position: popover.and_then(|facts| facts.highlighted_position),
        row_growth_pending: popover.is_some_and(|facts| facts.growth_pending),
        row_retirement_pending: popover.is_some_and(|facts| facts.retirement_pending),
        popover_empty_state_visible: popover.is_some_and(|facts| facts.empty_state_visible),
        popover_announcements: popover.map_or(0, |facts| facts.announcements),
        popover_announcement_pending: popover.is_some_and(|facts| facts.announcement_pending),
        popover_row_objects: popover.map_or(0, |facts| facts.row_objects),
        all_tabs_count_publications: state.all_tabs_count_publications.get(),
        all_tabs: controls.map(|controls| (controls.all_tabs, controls.popover)),
    }
}
