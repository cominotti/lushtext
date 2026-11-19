// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`watch`) of `WFR-TAB-OVERFLOW-NAVIGATION`.
//!
//! Observes the strip — both tab scrollers' adjustments, the bar's
//! `is-overflowing` and visibility, the tab model's selection, attach,
//! detach, reorder and transfer, and the bar's own scroll and press input —
//! and turns each observation into one of two things:
//!
//! * **a reconcile**, never run where it was observed: `is-overflowing` and
//!   the adjustments notify inside the bar's allocation (ledger A133), so the
//!   observer only schedules one `SettleBurst` reconcile, which is what
//!   readiness waits on. While the tab model is in a projection batch (a
//!   session restore, a bulk close) the reconcile waits for the batch's
//!   terminal, `refresh_tab_model_projections`, so the controls appear once.
//! * **a cancellation** of our step or hold (`scroll_execution.rs`), on every
//!   event that precedes a scroll of the strip's own. The bar's observers act
//!   only while a step is active, and ignore a press inside our own group.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::SettleBurst;
use gtk4::prelude::*;

use super::super::LushtextWindow;
use super::all_tabs::{SET_ALL_TABS_VISIBLE_ACTION, TOGGLE_ALL_TABS_ACTION};
use super::controls::{self, OverflowControls, SCROLL_BACK_ACTION, SCROLL_FORWARD_ACTION};
use super::policy::{
    ControlFocusFacts, OverflowFacts, all_tabs_button_visible, can_scroll_back, can_scroll_forward,
    controls_visible, focus_after_disable, group_attached, scroll_controls_available,
    unpinned_overflows,
};
use super::scroll_execution::{current_extent, find_tab_scroller, is_rtl};

/// A tab page and its title and pinned handlers.
pub(super) type PageWatch = (
    glib::WeakRef<libadwaita::TabPage>,
    [glib::SignalHandlerId; 2],
);

/// The workflow's one production state, held by the window. The readiness
/// blocker, the automation snapshot and the evidence surface all read it.
#[derive(Debug, Default)]
pub(crate) struct TabOverflowState {
    /// Whether the observers are installed.
    pub(super) installed: Cell<bool>,
    /// The end group, built once.
    pub(super) controls: RefCell<Option<OverflowControls>>,
    /// The unpinned tab scroller, when the lookup found it.
    pub(super) unpinned: RefCell<Option<gtk4::ScrolledWindow>>,
    /// The pinned tab scroller, when the lookup found it.
    pub(super) pinned: RefCell<Option<gtk4::ScrolledWindow>>,
    /// Whether the scroll buttons show.
    pub(super) controls_visible: Cell<bool>,
    /// How many times the scroll buttons appeared.
    pub(super) controls_shown_count: Cell<u32>,
    /// Whether the end group is the bar's end action widget.
    pub(super) group_attached: Cell<bool>,
    /// Whether the unpinned tabs overflowed at the last reconcile.
    pub(super) unpinned_overflowing: Cell<bool>,
    /// Whether Back may move, as last published.
    pub(super) can_scroll_back: Cell<bool>,
    /// Whether Forward may move, as last published.
    pub(super) can_scroll_forward: Cell<bool>,
    /// The scheduled reconcile.
    pub(super) reconcile: SettleBurst,
    /// A reconcile that found the tab model in a projection batch and waits
    /// for its terminal.
    pub(super) waiting_for_terminal: Cell<bool>,
    /// Our playing step, if any.
    pub(super) animation: RefCell<Option<libadwaita::TimedAnimation>>,
    /// Where the playing step lands.
    pub(super) animation_target: Cell<Option<f64>>,
    /// The hold-to-repeat source, if a hold is repeating.
    pub(super) repeat: RefCell<Option<glib::SourceId>>,
    /// Steps and holds the strip's own input cancelled.
    pub(super) guard_cancellations: Cell<u32>,
    /// The reveal of the selected tab after a session restore, while it runs.
    pub(super) selection_reveal: RefCell<Option<super::scroll_execution::SelectionReveal>>,
    /// Whether the reveal is re-announcing the unchanged selection right now.
    pub(super) revealing_selection: Cell<bool>,
    /// Whether the All Tabs button shows, as last published.
    pub(super) all_tabs_visible: Cell<bool>,
    /// The count the All Tabs button shows, as last published.
    pub(super) all_tabs_count: Cell<u32>,
    /// How many times the All Tabs count changed.
    pub(super) all_tabs_count_publications: Cell<u32>,
    /// The focus to give back when the popover closes without an activation.
    pub(super) saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// The re-snapshot of an open popover after a tab change.
    pub(super) resnapshot: gtk_lush_settle::Debounce,
    /// Each tab page's title and pinned handlers, by page address.
    pub(super) page_watches: RefCell<std::collections::HashMap<usize, PageWatch>>,
}

/// Disconnect a page's title and pinned handlers, if the page is still alive.
pub(super) fn release_page_watch((page, handlers): PageWatch) {
    if let Some(page) = page.upgrade() {
        for handler in handlers {
            page.disconnect(handler);
        }
    }
}

impl TabOverflowState {
    /// Whether our step animation is playing.
    pub(super) fn animation_playing(&self) -> bool {
        self.animation.borrow().as_ref().is_some_and(|animation| {
            libadwaita::prelude::AnimationExt::state(animation)
                == libadwaita::AnimationState::Playing
        })
    }

    /// Whether the readiness blocker `tab-overflow` holds: a step, a hold, a
    /// reconcile or a selection reveal is pending, or the All Tabs popover is
    /// still publishing or retiring rows.
    pub(crate) fn blocks_readiness(&self) -> bool {
        self.reconcile.pending()
            || self.waiting_for_terminal.get()
            || self.repeat.borrow().is_some()
            || self.selection_reveal.borrow().is_some()
            || self
                .controls
                .borrow()
                .as_ref()
                .is_some_and(|controls| controls.popover.publishing())
            || self.animation_playing()
    }
}

impl LushtextWindow {
    /// Stage 1: find the scrollers, build the end group, and connect the
    /// observers. Runs once, from window construction.
    pub(crate) fn install_tab_overflow(&self) {
        let imp = self.imp();
        let state = &imp.tab_overflow;
        if state.installed.replace(true) {
            return;
        }
        let bar = imp.tab_bar.get();
        let unpinned = find_tab_scroller(bar.upcast_ref(), false);
        let pinned = find_tab_scroller(bar.upcast_ref(), true);

        let hold_window = self.downgrade();
        let release_window = self.downgrade();
        let built = controls::build_controls(
            move |direction| {
                if let Some(window) = hold_window.upgrade() {
                    window.begin_tab_scroll_hold(direction);
                }
            },
            move || {
                if let Some(window) = release_window.upgrade() {
                    window.end_tab_scroll_hold();
                }
            },
        );
        state.controls.replace(Some(built));

        for scroller in [&unpinned, &pinned].into_iter().flatten() {
            let adjustment = scroller.hadjustment();
            let window_weak = self.downgrade();
            adjustment.connect_changed(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.schedule_tab_overflow_reconcile();
                }
            });
            let window_weak = self.downgrade();
            adjustment.connect_value_changed(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.schedule_tab_overflow_reconcile();
                }
            });
        }
        state.unpinned.replace(unpinned);
        state.pinned.replace(pinned);

        let window_weak = self.downgrade();
        bar.connect_is_overflowing_notify(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.schedule_tab_overflow_reconcile();
            }
        });
        let window_weak = self.downgrade();
        bar.connect_visible_notify(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.schedule_tab_overflow_reconcile();
            }
        });
        let window_weak = self.downgrade();
        bar.connect_direction_changed(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                window.cancel_tab_scroll_for_strip_input();
                window.schedule_tab_overflow_reconcile();
            }
        });
        self.watch_strip_input(&bar);
        self.watch_tab_model();
        self.install_all_tabs();
        self.reconcile_tab_overflow();
    }

    /// The bar's own wheel and press input: capture-phase observers that run
    /// before the tab box's bubble-phase handlers, never claim, and never stop
    /// propagation.
    fn watch_strip_input(&self, bar: &libadwaita::TabBar) {
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
        scroll.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let window_weak = self.downgrade();
        scroll.connect_scroll(move |_, _, _| {
            if let Some(window) = window_weak.upgrade()
                && window.tab_scroll_step_active()
            {
                window.cancel_tab_scroll_for_strip_input();
            }
            glib::Propagation::Proceed
        });
        bar.add_controller(scroll);

        let press = gtk4::GestureClick::new();
        press.set_button(0);
        press.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let window_weak = self.downgrade();
        press.connect_pressed(move |gesture, _, x, y| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if !window.tab_scroll_step_active()
                || window.press_is_inside_overflow_group(gesture, x, y)
            {
                return;
            }
            window.cancel_tab_scroll_for_strip_input();
        });
        bar.add_controller(press);
    }

    fn press_is_inside_overflow_group(&self, gesture: &gtk4::GestureClick, x: f64, y: f64) -> bool {
        let Some(widget) = gesture.widget() else {
            return false;
        };
        let Some(controls) = self.imp().tab_overflow.controls.borrow().clone() else {
            return false;
        };
        widget
            .pick(x, y, gtk4::PickFlags::DEFAULT)
            .is_some_and(|picked| {
                picked == *controls.group.upcast_ref::<gtk4::Widget>()
                    || picked.is_ancestor(&controls.group)
            })
    }

    /// The tab model's events that precede a scroll of the strip's own.
    fn watch_tab_model(&self) {
        let tab_view = self.imp().tab_view.get();
        let window_weak = self.downgrade();
        tab_view.connect_selected_page_notify(move |_| {
            if let Some(window) = window_weak.upgrade() {
                if !window.tab_overflow_reveals_selection() {
                    // A selection of the user's own: the strip scrolls to it.
                    window.end_selection_reveal();
                }
                window.cancel_tab_scroll_for_strip_input();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_page_attached(move |_, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.cancel_tab_scroll_for_strip_input();
                // The All Tabs count follows the tab set.
                window.schedule_tab_overflow_reconcile();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_page_detached(move |_, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.cancel_tab_scroll_for_strip_input();
                window.schedule_tab_overflow_reconcile();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_page_reordered(move |_, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.cancel_tab_scroll_for_strip_input();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_is_transferring_page_notify(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.cancel_tab_scroll_for_strip_input();
            }
        });
    }

    /// Schedule the one reconcile (inversion: control resumes in
    /// `reconcile_tab_overflow` from a zero-delay timeout, outside the
    /// allocation that notified).
    pub(crate) fn schedule_tab_overflow_reconcile(&self) {
        let state = &self.imp().tab_overflow;
        if !state.installed.get() || self.imp().disposing.get() {
            return;
        }
        let _handle = state
            .reconcile
            .schedule(self, Duration::ZERO, |window, handle| {
                if window.tab_projection_refresh_deferred() {
                    window.imp().tab_overflow.waiting_for_terminal.set(true);
                } else {
                    window.reconcile_tab_overflow();
                }
                handle.finish_if_current();
            });
    }

    /// Inversion terminal without a publish: the projection batch a reconcile
    /// waited for was released without its terminal publish (a restore
    /// cancelled by a superseding empty session), so the reconcile resumes
    /// here instead of at `refresh_tab_model_projections`.
    pub(crate) fn resume_tab_overflow_after_unpublished_batch(&self) {
        if self.imp().tab_overflow.waiting_for_terminal.get() {
            self.schedule_tab_overflow_reconcile();
        }
    }

    /// Stage 2: decide and publish the controls from the strip's state.
    pub(super) fn reconcile_tab_overflow(&self) {
        let imp = self.imp();
        let state = &imp.tab_overflow;
        state.waiting_for_terminal.set(false);
        let Some(controls) = state.controls.borrow().clone() else {
            return;
        };
        let (Some(bar), Some(tab_view)) = (imp.tab_bar.try_get(), imp.tab_view.try_get()) else {
            return;
        };
        let unpinned = state.unpinned.borrow().clone();
        let extent = unpinned
            .as_ref()
            .map(|scroller| current_extent(&scroller.hadjustment()));
        let pinned_overflows =
            state.pinned.borrow().as_ref().is_some_and(|scroller| {
                unpinned_overflows(current_extent(&scroller.hadjustment()))
            });
        // An empty unpinned box is not reallocated, so its adjustment can keep
        // the range of the tabs it held: no unpinned tab, no overflow.
        let unpinned_tabs = tab_view.n_pages() > tab_view.n_pinned_pages();
        let facts = OverflowFacts {
            strip_shown: bar.property::<bool>("visible") && tab_view.n_pages() > 0,
            unpinned_overflows: unpinned_tabs && extent.is_some_and(unpinned_overflows),
            bar_overflowing: bar.is_overflowing(),
            pinned_overflows,
        };
        state.unpinned_overflowing.set(facts.unpinned_overflows);
        let was_visible = state.controls_visible.get();
        let visible =
            scroll_controls_available(unpinned.is_some()) && controls_visible(was_visible, facts);
        let tab_count = u32::try_from(tab_view.n_pages()).unwrap_or(0);
        let all_tabs_visible = all_tabs_button_visible(tab_count, facts.strip_shown);
        let rtl = is_rtl(&bar);
        let back = visible && extent.is_some_and(|extent| can_scroll_back(extent, rtl));
        let forward = visible && extent.is_some_and(|extent| can_scroll_forward(extent, rtl));
        let focused = controls.focused_control(self);

        if visible != was_visible {
            controls.back.set_visible(visible);
            controls.forward.set_visible(visible);
            state.controls_visible.set(visible);
            if visible {
                state
                    .controls_shown_count
                    .set(state.controls_shown_count.get().saturating_add(1));
            } else {
                self.end_tab_scroll_hold();
            }
        }
        self.set_tab_overflow_action_enabled(SCROLL_BACK_ACTION, back);
        self.set_tab_overflow_action_enabled(SCROLL_FORWARD_ACTION, forward);
        state.can_scroll_back.set(back);
        state.can_scroll_forward.set(forward);

        if all_tabs_visible {
            controls.show_all_tabs_count(tab_count);
            if state.all_tabs_count.replace(tab_count) != tab_count {
                state
                    .all_tabs_count_publications
                    .set(state.all_tabs_count_publications.get().saturating_add(1));
            }
        } else if controls.popover.is_visible() {
            // The group is about to hide or detach: close the popover first,
            // with its focus restore (design D1).
            controls.popover.popdown();
        }
        if all_tabs_visible != state.all_tabs_visible.get() {
            controls.all_tabs.set_visible(all_tabs_visible);
            state.all_tabs_visible.set(all_tabs_visible);
        }
        self.set_tab_overflow_action_enabled(TOGGLE_ALL_TABS_ACTION, all_tabs_visible);
        self.set_tab_overflow_action_enabled(SET_ALL_TABS_VISIBLE_ACTION, all_tabs_visible);

        let attached = group_attached(visible, all_tabs_visible);
        if attached != state.group_attached.get() {
            if attached {
                bar.set_end_action_widget(Some(&controls.group));
            } else {
                bar.set_end_action_widget(None::<&gtk4::Widget>);
            }
            state.group_attached.set(attached);
        }

        let focus_facts = ControlFocusFacts::new(back, forward, all_tabs_visible);
        if let Some(control) = focused
            && !focus_facts.available(control)
            && !controls.focus_control(focus_after_disable(control, focus_facts))
        {
            self.focus_selected_editor_after_action();
        }
    }

    pub(super) fn set_tab_overflow_action_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self.lookup_action(name)
            && let Some(simple) = action.downcast_ref::<gtk4::gio::SimpleAction>()
            && simple.is_enabled() != enabled
        {
            simple.set_enabled(enabled);
        }
    }

    /// Teardown: drop every pending source and the animation.
    pub(crate) fn dispose_tab_overflow(&self) {
        self.end_selection_reveal();
        let state = &self.imp().tab_overflow;
        if let Some(source) = state.repeat.take() {
            source.remove();
        }
        if let Some(animation) = state.animation.take() {
            libadwaita::prelude::AnimationExt::pause(&animation);
        }
        state.animation_target.set(None);
        state.waiting_for_terminal.set(false);
        let _ = state.reconcile.clear();
        let _ = state.resnapshot.invalidate();
        state.saved_focus.replace(None);
        let watches: Vec<_> = state.page_watches.borrow_mut().drain().collect();
        for (_, watch) in watches {
            release_page_watch(watch);
        }
        if let Some(controls) = state.controls.borrow().clone()
            && controls.popover.is_visible()
        {
            controls.popover.popdown();
        }
    }
}
