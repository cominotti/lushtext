// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`, stage-qualified: the scroll stage order)
//! of `WFR-TAB-OVERFLOW-NAVIGATION`.
//!
//! Owns the lookup of the unpinned tab scroller, the 200 ms step animation,
//! the hold-to-repeat source, and the cancellation of both by the strip's own
//! input (design D3). The pure decisions (may it move, where does a step
//! land) come from `policy.rs`; this module only rounds the adjustment once
//! into whole pixels and writes the result back.
//!
//! **Cancelling is `adw_animation_pause`, never `reset` or `skip`:** both of
//! those write a value into the adjustment, and that outside `value-changed`
//! would pause the box animation the strip just started (ledger A132).

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::super::LushtextWindow;
use super::policy::{
    ScrollDirection, StripScrollExtent, TAB_SCROLL_ANIMATION_MS, TAB_SCROLL_REPEAT_DELAY_MS,
    TAB_SCROLL_REPEAT_INTERVAL_MS, can_scroll, next_scroll_target,
};

/// The deepest descendant level of an `AdwTabBar` the lookup walks: bar →
/// revealer → box → scrolled window (`adw-tab-bar.ui`, ledger A132).
///
/// Kind: external fact; the depth of Libadwaita's tab-bar template (ledger A132).
const TAB_SCROLLER_LOOKUP_DEPTH: usize = 3;
/// How many frames a selection reveal may run: the wait for the strip's
/// allocation, the attach scrolls landing, and the
/// [`SELECTION_REVEAL_SCROLL_WINDOW_US`] after the announcement take about 45
/// frames at 120 Hz and 90 at 240 Hz; after the cap the reveal ends wherever
/// the strip is.
///
/// Kind: cap; a bound on frames observed, rounded down to a power of two.
pub(super) const SELECTION_REVEAL_MAX_FRAMES: u32 = 256;
/// How long the selection reveal waits after re-announcing the selection
/// before it may end: the strip scrolls to the tab with its own 200 ms
/// animation (`SCROLL_ANIMATION_DURATION`, `adw-tab-box.c`), whose eased
/// tail keeps the strip on one whole pixel for several frames, so a still
/// frame inside the window is not the end of the scroll.
///
/// Kind: out: time; spans the strip's 200 ms scroll with a margin.
pub(super) const SELECTION_REVEAL_SCROLL_WINDOW_US: i64 = 256_000;

/// Where a selection reveal stands between two frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RevealProgress {
    /// The strip's value at the previous allocated frame, in whole pixels.
    value_px: Option<i32>,
    /// The frame time of the announcement, once it was made.
    announced_at_us: Option<i64>,
}

/// One running selection reveal; dropping it removes its tick callback.
pub(crate) struct SelectionReveal {
    bar: glib::WeakRef<libadwaita::TabBar>,
    tick: Option<gtk4::TickCallbackId>,
    /// Set by the callback when it returned `Break` (GTK removed it then).
    ended: std::rc::Rc<std::cell::Cell<bool>>,
}

impl std::fmt::Debug for SelectionReveal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SelectionReveal")
            .field("ended", &self.ended.get())
            .finish_non_exhaustive()
    }
}

impl Drop for SelectionReveal {
    fn drop(&mut self) {
        if let Some(tick) = self.tick.take()
            && !self.ended.get()
            && self.bar.upgrade().is_some()
        {
            tick.remove();
        }
    }
}

/// The CSS class the bar's template gives its pinned-tab scroller.
const PINNED_SCROLLER_CLASS: &str = "pinned";

/// The first `GtkScrolledWindow` under `root`, at most the template's depth
/// down, that carries the `pinned` CSS class when `pinned` and lacks it
/// otherwise. `None` when the template has another shape: the scroll
/// buttons then never show (fail closed, design D3).
#[must_use]
pub fn find_tab_scroller(root: &gtk4::Widget, pinned: bool) -> Option<gtk4::ScrolledWindow> {
    let mut level = vec![root.clone()];
    for _ in 0..TAB_SCROLLER_LOOKUP_DEPTH {
        let mut next = Vec::new();
        for widget in level {
            let mut child = widget.first_child();
            while let Some(current) = child {
                if let Some(scroller) = current.downcast_ref::<gtk4::ScrolledWindow>()
                    && scroller.has_css_class(PINNED_SCROLLER_CLASS) == pinned
                {
                    return Some(scroller.clone());
                }
                child = current.next_sibling();
                next.push(current);
            }
        }
        level = next;
    }
    None
}

/// `value` rounded to the nearest whole pixel, saturating at the `i32` range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is rounded and clamped to the i32 range first; an adjustment value \
              in pixels never approaches it"
)]
pub(super) fn whole_px(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// `adjustment` rounded once into whole pixels, its value taken as `value`.
pub(super) fn extent_at(adjustment: &gtk4::Adjustment, value: f64) -> StripScrollExtent {
    StripScrollExtent {
        value_px: whole_px(value - adjustment.lower()),
        page_px: whole_px(adjustment.page_size()),
        upper_px: whole_px(adjustment.upper() - adjustment.lower()),
    }
}

/// `adjustment` rounded once into whole pixels at its current value.
pub(super) fn current_extent(adjustment: &gtk4::Adjustment) -> StripScrollExtent {
    extent_at(adjustment, adjustment.value())
}

/// Whether `widget` lays out right to left.
pub(super) fn is_rtl(widget: &impl IsA<gtk4::Widget>) -> bool {
    widget.direction() == gtk4::TextDirection::Rtl
}

#[cfg(feature = "test-utils")]
fn immediate_steps() -> bool {
    super::test_policy::immediate_steps()
}

#[cfg(not(feature = "test-utils"))]
const fn immediate_steps() -> bool {
    false
}

fn repeat_delay() -> Duration {
    #[cfg(feature = "test-utils")]
    if let Some(delay) = super::test_policy::repeat_delay_ms() {
        return Duration::from_millis(u64::from(delay));
    }
    Duration::from_millis(u64::from(TAB_SCROLL_REPEAT_DELAY_MS))
}

fn repeat_interval() -> Duration {
    #[cfg(feature = "test-utils")]
    if let Some(interval) = super::test_policy::repeat_interval_ms() {
        return Duration::from_millis(u64::from(interval));
    }
    Duration::from_millis(u64::from(TAB_SCROLL_REPEAT_INTERVAL_MS))
}

impl LushtextWindow {
    /// Stage: move the unpinned tabs one page increment toward `direction`.
    ///
    /// Refused while the controls are hidden, at the edge, and while a tab is
    /// being dragged (the box's drag autoscroll owns the strip then). Returns
    /// whether a step started.
    pub(super) fn step_tab_strip(&self, direction: ScrollDirection) -> bool {
        let imp = self.imp();
        let state = &imp.tab_overflow;
        if !state.controls_visible.get() || imp.tab_view.is_transferring_page() {
            return false;
        }
        // The user moves the strip now: a reveal after a restore yields.
        self.end_selection_reveal();
        let Some(scroller) = state.unpinned.borrow().clone() else {
            return false;
        };
        let adjustment = scroller.hadjustment();
        let rtl = is_rtl(&*imp.tab_bar);
        let from = state
            .animation_target
            .get()
            .filter(|_| self.tab_scroll_animation_playing())
            .unwrap_or_else(|| adjustment.value());
        let extent = extent_at(&adjustment, from);
        if !can_scroll(extent, direction, rtl) {
            return false;
        }
        let target = next_scroll_target(
            extent,
            whole_px(adjustment.page_increment()),
            direction,
            rtl,
        );
        self.animate_tab_strip_to(&adjustment, adjustment.lower() + f64::from(target));
        true
    }

    /// Whether our step animation is playing.
    pub(super) fn tab_scroll_animation_playing(&self) -> bool {
        self.imp().tab_overflow.animation_playing()
    }

    fn animate_tab_strip_to(&self, adjustment: &gtk4::Adjustment, target: f64) {
        let state = &self.imp().tab_overflow;
        // A playing step stops where it is; the new one starts from there.
        if let Some(previous) = state.animation.take()
            && previous.state() == libadwaita::AnimationState::Playing
        {
            previous.pause();
        }
        if immediate_steps() {
            state.animation_target.set(None);
            adjustment.set_value(target);
            return;
        }
        let animation = libadwaita::TimedAnimation::new(
            &*self.imp().tab_bar,
            adjustment.value(),
            target,
            TAB_SCROLL_ANIMATION_MS,
            libadwaita::PropertyAnimationTarget::new(adjustment, "value"),
        );
        animation.set_easing(libadwaita::Easing::EaseOutCubic);
        let window_weak = self.downgrade();
        animation.connect_done(move |done| {
            if let Some(window) = window_weak.upgrade() {
                window.finish_tab_scroll_animation(done);
            }
        });
        state.animation_target.set(Some(target));
        state.animation.replace(Some(animation.clone()));
        // With animations off, Libadwaita completes the animation at once.
        animation.play();
    }

    fn finish_tab_scroll_animation(&self, done: &libadwaita::TimedAnimation) {
        let state = &self.imp().tab_overflow;
        let current = state
            .animation
            .borrow()
            .as_ref()
            .is_some_and(|animation| animation == done);
        if current {
            state.animation.replace(None);
            state.animation_target.set(None);
        }
        self.schedule_tab_overflow_reconcile();
    }

    /// Stage: a pointer press on a scroll button. One step now, then a repeat
    /// after the hold delay until release, the edge, a cancellation, or the
    /// button hiding.
    pub(super) fn begin_tab_scroll_hold(&self, direction: ScrollDirection) {
        self.end_tab_scroll_hold();
        if self.step_tab_strip(direction) {
            self.arm_tab_scroll_repeat(direction, repeat_delay());
        }
    }

    fn arm_tab_scroll_repeat(&self, direction: ScrollDirection, delay: Duration) {
        let window_weak = self.downgrade();
        let source = glib::timeout_add_local_once(delay, move || {
            if let Some(window) = window_weak.upgrade() {
                // The source has dispatched: forget its id without removing it.
                let _dispatched = window.imp().tab_overflow.repeat.take();
                if window.step_tab_strip(direction) {
                    window.arm_tab_scroll_repeat(direction, repeat_interval());
                }
            }
        });
        self.imp().tab_overflow.repeat.replace(Some(source));
    }

    /// Stage: the hold ended (release, gesture cancel, or the button hiding).
    /// The step in flight finishes; no further step starts.
    pub(super) fn end_tab_scroll_hold(&self) {
        if let Some(source) = self.imp().tab_overflow.repeat.take() {
            source.remove();
        }
    }

    /// Inversion terminal: the strip's own input started, or is about to
    /// start, a scroll of its own. Pause our step and drop the repeat, so the
    /// strip's scroll wins (design D3). Counts only when something was active.
    pub(super) fn cancel_tab_scroll_for_strip_input(&self) {
        let state = &self.imp().tab_overflow;
        let mut cancelled = false;
        if let Some(animation) = state.animation.take() {
            if animation.state() == libadwaita::AnimationState::Playing {
                animation.pause();
                cancelled = true;
            }
            state.animation_target.set(None);
        }
        if let Some(source) = state.repeat.take() {
            source.remove();
            cancelled = true;
        }
        if cancelled {
            state
                .guard_cancellations
                .set(state.guard_cancellations.get().saturating_add(1));
            self.schedule_tab_overflow_reconcile();
        }
    }

    /// Stage: bring the selected tab into view through the strip's own
    /// selection scrolling (ledger A133: `select_page` → `scroll_to_tab`,
    /// which moves the strip only while the tab is outside the page). Two
    /// callers: the session restore's terminal, because every page the
    /// restore attached after its selection scrolled the strip to that page's
    /// tab (A132) and can leave the selected one out of view; and an All Tabs
    /// row of the tab that is already selected, which no selection change
    /// would scroll to.
    ///
    /// **Inversion:** the strip is often not allocated yet at the terminal
    /// (the box then skips the scroll, its tabs having no width), and an
    /// attach scroll queued in the same dispatch lands only at the next
    /// allocation. So the reveal resumes from a tick callback on the bar:
    /// once per frame, after the strip has been allocated, it re-announces the
    /// unchanged selection each time the strip comes to rest, and ends once
    /// the strip stays still after an announcement, at most
    /// [`SELECTION_REVEAL_MAX_FRAMES`] frames.
    pub(in crate::ui::window) fn reveal_selected_tab(&self) {
        self.end_selection_reveal();
        let imp = self.imp();
        let Some(bar) = imp.tab_bar.try_get() else {
            return;
        };
        if imp.tab_view.selected_page().is_none()
            || !bar.is_mapped()
            || imp.tab_overflow.unpinned.borrow().is_none()
        {
            return;
        }
        let ended = std::rc::Rc::new(std::cell::Cell::new(false));
        let window_weak = self.downgrade();
        let tick = bar.add_tick_callback({
            let ended = std::rc::Rc::clone(&ended);
            let progress = std::cell::Cell::new(RevealProgress::default());
            let frames = std::cell::Cell::new(0u32);
            move |_, clock| {
                let Some(window) = window_weak.upgrade() else {
                    ended.set(true);
                    return glib::ControlFlow::Break;
                };
                frames.set(frames.get().saturating_add(1));
                let outcome =
                    window.selection_reveal_frame(&progress, frames.get(), clock.frame_time());
                if outcome == glib::ControlFlow::Break {
                    ended.set(true);
                    window.imp().tab_overflow.selection_reveal.replace(None);
                    window.schedule_tab_overflow_reconcile();
                }
                outcome
            }
        });
        imp.tab_overflow
            .selection_reveal
            .replace(Some(SelectionReveal {
                bar: bar.downgrade(),
                tick: Some(tick),
                ended,
            }));
    }

    /// One frame of the reveal: `Break` once it is done.
    ///
    /// The selection is re-announced once, on the first frame that finds the
    /// strip where the previous frame left it, so a scroll already under way
    /// (an attach scroll landing at this allocation, the strip's own
    /// animation) finishes first. Announcing again would restart the strip's
    /// animation from wherever its eased tail stands, so the reveal never
    /// does; it ends on the first still frame once
    /// [`SELECTION_REVEAL_SCROLL_WINDOW_US`] has passed since the
    /// announcement, when the scroll it started has finished.
    fn selection_reveal_frame(
        &self,
        progress: &std::cell::Cell<RevealProgress>,
        frames: u32,
        frame_time_us: i64,
    ) -> glib::ControlFlow {
        let imp = self.imp();
        let Some(adjustment) = imp
            .tab_overflow
            .unpinned
            .borrow()
            .as_ref()
            .map(gtk4::ScrolledWindow::hadjustment)
        else {
            return glib::ControlFlow::Break;
        };
        if frames > SELECTION_REVEAL_MAX_FRAMES || imp.tab_view.selected_page().is_none() {
            return glib::ControlFlow::Break;
        }
        let extent = current_extent(&adjustment);
        // Not allocated yet: the box would skip the scroll.
        if extent.page_px <= 0 {
            return glib::ControlFlow::Continue;
        }
        let previous = progress.get();
        let still = previous.value_px == Some(extent.value_px);
        progress.set(RevealProgress {
            value_px: Some(extent.value_px),
            ..previous
        });
        match previous.announced_at_us {
            // The strip is still moving (or this is its first allocated
            // frame): let that scroll land first.
            None if !still => glib::ControlFlow::Continue,
            None => {
                let state = &imp.tab_overflow;
                state.revealing_selection.set(true);
                imp.tab_view.notify("selected-page");
                state.revealing_selection.set(false);
                progress.set(RevealProgress {
                    value_px: Some(extent.value_px),
                    announced_at_us: Some(frame_time_us),
                });
                glib::ControlFlow::Continue
            }
            Some(announced_at_us)
                if still
                    && frame_time_us.saturating_sub(announced_at_us)
                        >= SELECTION_REVEAL_SCROLL_WINDOW_US =>
            {
                glib::ControlFlow::Break
            }
            Some(_) => glib::ControlFlow::Continue,
        }
    }

    /// Stop a running reveal (a step, the user's own selection, the strip's
    /// own input, disposal). Idempotent.
    pub(super) fn end_selection_reveal(&self) {
        let reveal = self.imp().tab_overflow.selection_reveal.take();
        drop(reveal);
    }

    /// Whether the selection is being re-announced by the reveal, which the
    /// window's own selection handler ignores: nothing changed.
    pub(in crate::ui::window) fn tab_overflow_reveals_selection(&self) -> bool {
        self.imp().tab_overflow.revealing_selection.get()
    }

    /// Whether a step or a hold is active, which is when the strip observers
    /// act at all.
    pub(super) fn tab_scroll_step_active(&self) -> bool {
        self.tab_scroll_animation_playing() || self.imp().tab_overflow.repeat.borrow().is_some()
    }
}
