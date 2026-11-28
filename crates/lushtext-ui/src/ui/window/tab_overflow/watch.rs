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
//!   event that precedes a scroll of the strip's own. The bar's observers
//!   cancel only while a step is active, and ignore a press or a wheel inside
//!   our own group; a wheel or press outside it, while the All Tabs popover is
//!   closed, also ends a running selection reveal: the user moves the strip.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::SettleBurst;
use gtk4::prelude::*;
use lushtext_model::values::FocusRestoreGeneration;

use super::super::LushtextWindow;
use super::all_tabs_execution::{SET_ALL_TABS_VISIBLE_ACTION, TOGGLE_ALL_TABS_ACTION};
use super::controls::{self, OverflowControls, SCROLL_BACK_ACTION, SCROLL_FORWARD_ACTION};
use super::policy::{
    ControlFocusFacts, OverflowControlsInputs, OverflowControlsPlan, OverflowFacts,
    focus_after_disable, plan_overflow_controls, strip_overflows,
};
use super::scroll_execution::{current_extent, find_tab_scroller, is_rtl};

/// A tab page and its title and pinned handlers.
pub(super) type PageWatch = (
    glib::WeakRef<libadwaita::TabPage>,
    [glib::SignalHandlerId; 2],
);

/// Installation lifecycle for the overflow controls and strip scrollers.
#[derive(Debug, Default)]
pub(super) enum OverflowInstallPhase {
    #[default]
    Uninstalled,
    Installed {
        controls: OverflowControls,
        unpinned: Option<gtk4::ScrolledWindow>,
        pinned: Option<gtk4::ScrolledWindow>,
    },
}

/// Last-published overflow control visibility, enablement, and counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "read-only last-published control state snapshot DTO"
)]
pub(super) struct PublishedControlsSnapshot {
    pub(super) controls_visible: bool,
    pub(super) group_attached: bool,
    pub(super) unpinned_overflowing: bool,
    pub(super) can_scroll_back: bool,
    pub(super) can_scroll_forward: bool,
    pub(super) all_tabs_visible: bool,
    pub(super) pointer_in_group: bool,
    pub(super) controls_shown_count: u32,
    pub(super) all_tabs_count: u32,
    pub(super) all_tabs_count_publications: u32,
}

/// Step-animation lifecycle for strip scrolling.
#[derive(Debug, Default)]
pub(super) enum TabScrollAnimationPhase {
    #[default]
    Idle,
    Playing {
        animation: libadwaita::TimedAnimation,
        target: f64,
    },
}

/// Active scroll step animation, hold-repeat timer, and guard cancellation count.
#[derive(Debug, Default)]
pub(super) struct TabScrollController {
    pub(super) animation: TabScrollAnimationPhase,
    pub(super) repeat: gtk_lush_settle::SupersedingTimer,
    pub(super) guard_cancellations: u32,
}

/// Lifecycle of bringing the selected tab into view after a restore or pick.
#[derive(Debug, Default)]
pub(super) enum SelectionRevealPhase {
    #[default]
    Idle,
    Owed(glib::SignalHandlerId),
    Running {
        reveal: super::scroll_execution::SelectionReveal,
        announcing: bool,
    },
}

/// Focus save/restore lifecycle for the All Tabs popup.
#[derive(Debug, Default)]
pub(super) enum AllTabsFocusPhase {
    #[default]
    Idle,
    Saved {
        widget: Option<glib::WeakRef<gtk4::Widget>>,
    },
    RestorePending {
        widget: Option<glib::WeakRef<gtk4::Widget>>,
        generation: FocusRestoreGeneration,
    },
}

/// The workflow's one production state, held by the window. The readiness
/// blocker, the automation snapshot and the evidence surface all read it.
#[derive(Debug, Default)]
pub(crate) struct TabOverflowState {
    /// Whether the observers and end-group controls are installed.
    pub(super) install: RefCell<OverflowInstallPhase>,
    /// Last-published control flags and counters.
    pub(super) published: Cell<PublishedControlsSnapshot>,
    /// The scheduled reconcile.
    pub(super) reconcile: SettleBurst,
    /// A reconcile that found the tab model in a projection batch and waits
    /// for its terminal.
    pub(super) waiting_for_terminal: Cell<bool>,
    /// Playing step animation, hold-to-repeat timer, and guard cancellations.
    pub(super) scroll: RefCell<TabScrollController>,
    /// Selection reveal lifecycle (`Idle`, `Owed` on next map, or `Running`).
    pub(super) reveal: RefCell<SelectionRevealPhase>,
    /// Focus restore lifecycle for the All Tabs popup.
    pub(super) all_tabs_focus: RefCell<AllTabsFocusPhase>,
    /// The generation of the latest focus restore; a reopened popup or
    /// disposal advances it, so an older restore's idle does nothing.
    pub(super) focus_restore_generation: Cell<FocusRestoreGeneration>,
    /// The re-snapshot of an open popup after a tab change.
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
    /// Whether the observers and controls have been installed.
    pub(super) fn is_installed(&self) -> bool {
        matches!(
            *self.install.borrow(),
            OverflowInstallPhase::Installed { .. }
        )
    }

    /// The end group, once the install stage built it (a clone, so no borrow
    /// is held while a caller drives GTK).
    pub(super) fn controls(&self) -> Option<OverflowControls> {
        match &*self.install.borrow() {
            OverflowInstallPhase::Installed { controls, .. } => Some(controls.clone()),
            OverflowInstallPhase::Uninstalled => None,
        }
    }

    /// The unpinned tab scroller, when installed and found.
    pub(super) fn unpinned(&self) -> Option<gtk4::ScrolledWindow> {
        match &*self.install.borrow() {
            OverflowInstallPhase::Installed { unpinned, .. } => unpinned.clone(),
            OverflowInstallPhase::Uninstalled => None,
        }
    }

    /// The pinned tab scroller, when installed and found.
    pub(super) fn pinned(&self) -> Option<gtk4::ScrolledWindow> {
        match &*self.install.borrow() {
            OverflowInstallPhase::Installed { pinned, .. } => pinned.clone(),
            OverflowInstallPhase::Uninstalled => None,
        }
    }

    /// Snapshot of last-published control flags and counters.
    pub(super) fn published(&self) -> PublishedControlsSnapshot {
        self.published.get()
    }

    /// Update the published controls snapshot in place.
    pub(super) fn update_published(&self, update: impl FnOnce(&mut PublishedControlsSnapshot)) {
        let mut published = self.published.get();
        update(&mut published);
        self.published.set(published);
    }

    /// Whether our step animation is playing.
    pub(super) fn animation_playing(&self) -> bool {
        matches!(
            self.scroll.borrow().animation,
            TabScrollAnimationPhase::Playing { ref animation, .. } if is_playing(animation)
        )
    }

    /// Whether a hold-to-repeat timer is currently armed.
    pub(super) fn repeat_active(&self) -> bool {
        self.scroll.borrow().repeat.is_armed()
    }

    /// Steps and holds the strip's own input cancelled.
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn guard_cancellations(&self) -> u32 {
        self.scroll.borrow().guard_cancellations
    }

    /// Take our step animation, pausing it where it stands when it plays
    /// (`adw_animation_pause`, never `reset` or `skip`: module docs of
    /// `scroll_execution.rs`). Returns whether it was playing.
    pub(super) fn pause_step(&self) -> bool {
        let taken = std::mem::take(&mut self.scroll.borrow_mut().animation);
        match taken {
            TabScrollAnimationPhase::Playing { animation, .. } if is_playing(&animation) => {
                libadwaita::prelude::AnimationExt::pause(&animation);
                true
            }
            _ => false,
        }
    }

    /// Whether a selection reveal is currently running.
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn selection_reveal_pending(&self) -> bool {
        matches!(*self.reveal.borrow(), SelectionRevealPhase::Running { .. })
    }

    /// Whether a popup focus restore idle is currently pending.
    pub(super) fn focus_restore_pending(&self) -> bool {
        matches!(
            *self.all_tabs_focus.borrow(),
            AllTabsFocusPhase::RestorePending { .. }
        )
    }

    /// Whether the readiness blocker `tab-overflow` holds: a step, a hold, a
    /// reconcile, a focus restore or a selection reveal (for a bounded time,
    /// `policy::reveal_holds_readiness`) is pending, or the All Tabs popup
    /// is still publishing or retiring rows.
    pub(crate) fn blocks_readiness(&self) -> bool {
        let reveal_blocks = match &*self.reveal.borrow() {
            SelectionRevealPhase::Running { reveal, .. } => {
                reveal.blocks_readiness(glib::monotonic_time())
            }
            SelectionRevealPhase::Idle | SelectionRevealPhase::Owed(_) => false,
        };
        self.reconcile.pending()
            || self.waiting_for_terminal.get()
            || self.repeat_active()
            || self.focus_restore_pending()
            || reveal_blocks
            || self
                .controls()
                .is_some_and(|controls| controls.popover.publishing())
            || self.animation_playing()
    }
}

fn is_playing(animation: &libadwaita::TimedAnimation) -> bool {
    libadwaita::prelude::AnimationExt::state(animation) == libadwaita::AnimationState::Playing
}

impl LushtextWindow {
    /// Stage 1: find the scrollers, build the end group, and connect the
    /// observers. Runs once, from window construction.
    pub(crate) fn install_tab_overflow(&self) {
        let imp = self.imp();
        let state = &imp.tab_overflow;
        if state.is_installed() {
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
        state.install.replace(OverflowInstallPhase::Installed {
            controls: built,
            unpinned,
            pinned,
        });

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
                && !window.imp().tab_overflow.published().pointer_in_group
            {
                window.yield_to_strip_input();
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
            // A press on the strip (a tab, a drag) yields as a wheel does.
            if !window.press_is_inside_overflow_group(gesture, x, y) {
                window.yield_to_strip_input();
            }
        });
        bar.add_controller(press);

        // A wheel over our own group moves no strip: track the pointer there,
        // as the press observer resolves its own position.
        if let Some(controls) = self.imp().tab_overflow.controls() {
            let motion = gtk4::EventControllerMotion::new();
            let window_weak = self.downgrade();
            motion.connect_enter(move |_, _, _| {
                if let Some(window) = window_weak.upgrade() {
                    window
                        .imp()
                        .tab_overflow
                        .update_published(|published| published.pointer_in_group = true);
                }
            });
            let window_weak = self.downgrade();
            motion.connect_leave(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window
                        .imp()
                        .tab_overflow
                        .update_published(|published| published.pointer_in_group = false);
                }
            });
            controls.group.add_controller(motion);
        }
    }

    /// A wheel or press on the strip outside our group: the user moves the
    /// strip, so our step or hold is cancelled. A reveal after a restore
    /// yields too, so its re-announced selection cannot undo the user's
    /// scroll, but not while the All Tabs popover is open: the popover is a
    /// descendant of our group, its own input reaches these observers, and it
    /// moves no strip. A step still playing when such input arrives (within
    /// its 200 ms) is paused like any other.
    fn yield_to_strip_input(&self) {
        if !self.all_tabs_popover_visible() {
            self.end_selection_reveal();
        }
        if self.tab_scroll_step_active() {
            self.cancel_tab_scroll_for_strip_input();
        }
    }

    fn press_is_inside_overflow_group(&self, gesture: &gtk4::GestureClick, x: f64, y: f64) -> bool {
        let Some(widget) = gesture.widget() else {
            return false;
        };
        let Some(controls) = self.imp().tab_overflow.controls() else {
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
        if !state.is_installed() || self.imp().disposing.get() {
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

    /// The actions exist only once `setup_actions` has run, which is after
    /// the install stage's reconcile in `constructed`: publish their
    /// enablement then, so none is enabled while its control is hidden.
    pub(in crate::ui::window) fn sync_tab_overflow_actions(&self) {
        self.reconcile_tab_overflow();
        self.set_all_tabs_query_enabled(false);
    }

    /// Stage 2: decide and publish the controls from the strip's state.
    pub(super) fn reconcile_tab_overflow(&self) {
        let imp = self.imp();
        let state = &imp.tab_overflow;
        state.waiting_for_terminal.set(false);
        let Some(controls) = state.controls() else {
            return;
        };
        let (Some(bar), Some(tab_view)) = (imp.tab_bar.try_get(), imp.tab_view.try_get()) else {
            return;
        };
        let unpinned = state.unpinned();
        let extent = unpinned
            .as_ref()
            .map(|scroller| current_extent(&scroller.hadjustment()));
        // An empty unpinned box is not reallocated, so its adjustment can keep
        // the range of the tabs it held: no unpinned tab, no overflow.
        let unpinned_tabs = tab_view.n_pages() > tab_view.n_pinned_pages();
        let facts = OverflowFacts {
            strip_shown: bar.property::<bool>("visible") && tab_view.n_pages() > 0,
            unpinned_overflows: unpinned_tabs && extent.is_some_and(strip_overflows),
            bar_overflowing: bar.is_overflowing(),
            pinned_overflows: state.pinned_overflows(),
        };
        let prev = state.published();
        let was_visible = prev.controls_visible;
        let tab_count = u32::try_from(tab_view.n_pages()).unwrap_or(0);
        let OverflowControlsPlan {
            buttons_visible,
            can_scroll_back,
            can_scroll_forward,
            all_tabs_visible,
            group_attached,
        } = plan_overflow_controls(OverflowControlsInputs {
            scroller_found: unpinned.is_some(),
            buttons_visible: was_visible,
            facts,
            extent,
            rtl: is_rtl(&bar),
            tab_count,
        });
        let focused = (!state.focus_restore_pending())
            .then(|| controls.focused_control(self))
            .flatten();

        if buttons_visible != was_visible {
            controls.back.set_visible(buttons_visible);
            controls.forward.set_visible(buttons_visible);
            if !buttons_visible {
                self.end_tab_scroll_hold();
            }
        }
        self.set_tab_overflow_action_enabled(SCROLL_BACK_ACTION, can_scroll_back);
        self.set_tab_overflow_action_enabled(SCROLL_FORWARD_ACTION, can_scroll_forward);

        if all_tabs_visible {
            controls.show_all_tabs_count(tab_count);
        } else if controls.popover.is_visible() {
            // The group is about to hide or detach: close the popup first,
            // with its focus restore (design D1).
            controls.popover.popdown();
        }
        if all_tabs_visible != prev.all_tabs_visible {
            controls.all_tabs.set_visible(all_tabs_visible);
        }
        self.set_tab_overflow_action_enabled(TOGGLE_ALL_TABS_ACTION, all_tabs_visible);
        self.set_tab_overflow_action_enabled(SET_ALL_TABS_VISIBLE_ACTION, all_tabs_visible);

        if group_attached != prev.group_attached {
            if group_attached {
                bar.set_end_action_widget(Some(&controls.group));
            } else {
                bar.set_end_action_widget(None::<&gtk4::Widget>);
            }
        }
        state.update_published(|published| {
            published.unpinned_overflowing = facts.unpinned_overflows;
            if buttons_visible != was_visible {
                published.controls_visible = buttons_visible;
                if buttons_visible {
                    published.controls_shown_count =
                        published.controls_shown_count.saturating_add(1);
                }
            }
            published.can_scroll_back = can_scroll_back;
            published.can_scroll_forward = can_scroll_forward;
            if all_tabs_visible && published.all_tabs_count != tab_count {
                published.all_tabs_count = tab_count;
                published.all_tabs_count_publications =
                    published.all_tabs_count_publications.saturating_add(1);
            }
            published.all_tabs_visible = all_tabs_visible;
            if group_attached != prev.group_attached {
                published.group_attached = group_attached;
                if !group_attached {
                    // A detached group is under no pointer, whether or not GTK
                    // delivered its `leave`.
                    published.pointer_in_group = false;
                }
            }
        });

        let focus_facts = ControlFocusFacts {
            back_available: can_scroll_back,
            forward_available: can_scroll_forward,
            all_tabs_available: all_tabs_visible,
        };
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
        self.forget_owed_selection_reveal();
        let _stopped = self.stop_tab_scroll();
        let state = &self.imp().tab_overflow;
        state.waiting_for_terminal.set(false);
        let _ = state.reconcile.clear();
        let _ = state.resnapshot.invalidate();
        self.cancel_all_tabs_focus_restore();
        state.all_tabs_focus.replace(AllTabsFocusPhase::Idle);
        let watches: Vec<_> = state.page_watches.borrow_mut().drain().collect();
        for (_, watch) in watches {
            release_page_watch(watch);
        }
        if let Some(controls) = state.controls()
            && controls.popover.is_visible()
        {
            controls.popover.popdown();
        }
    }
}
