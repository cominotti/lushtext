// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`, stage-qualified: the scroll stage order)
//! of `WFR-TAB-OVERFLOW-NAVIGATION`.
//!
//! Owns the lookup of the unpinned tab scroller and of a page's tab in it
//! (with the read of whether the selected tab is in view), the 200 ms step
//! animation, the hold-to-repeat source, the selection reveal, and the
//! cancellation of the step and the hold by the strip's own input (design
//! D3). The pure decisions (may it move, where does a step land) come from
//! `policy.rs`; this module only rounds the adjustment once into whole pixels
//! and writes the result back.
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
    RevealProgress, RevealStep, ScrollDirection, StripScrollExtent, TAB_SCROLL_ANIMATION_MS,
    TAB_SCROLL_REPEAT_DELAY_MS, TAB_SCROLL_REPEAT_INTERVAL_MS, can_scroll, next_scroll_target,
    reveal_frame, reveal_holds_readiness, strip_overflows, tab_in_view,
};
use super::watch::TabOverflowState;

/// The deepest descendant level of an `AdwTabBar` the lookup walks: bar →
/// revealer → box → scrolled window (`adw-tab-bar.ui`, ledger A132).
///
/// Kind: external fact; the depth of Libadwaita's tab-bar template (ledger A132).
const TAB_SCROLLER_LOOKUP_DEPTH: usize = 3;

/// One running selection reveal; dropping it removes its tick callback.
pub(crate) struct SelectionReveal {
    bar: glib::WeakRef<libadwaita::TabBar>,
    tick: Option<gtk4::TickCallbackId>,
    /// Set by the callback when it returned `Break` (GTK removed it then).
    ended: std::rc::Rc<std::cell::Cell<bool>>,
    /// When the reveal started, in `glib::monotonic_time` microseconds.
    started_at_us: i64,
}

impl SelectionReveal {
    /// Whether the reveal still holds the readiness blocker at `now_us`
    /// (`policy::reveal_holds_readiness`): it runs on the frame clock, and a
    /// window that paints no frame would otherwise hold readiness
    /// indefinitely. The reveal itself goes on when frames resume.
    pub(super) fn blocks_readiness(&self, now_us: i64) -> bool {
        reveal_holds_readiness(self.started_at_us, now_us)
    }
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

/// The private type `AdwTabBox` draws each page's tab with (`adw-tab.c`),
/// matched by name because it is not exported.
///
/// Kind: external fact; a Libadwaita type name (ledger A132).
const TAB_WIDGET_TYPE: &str = "AdwTab";

/// The tab `scroller`'s tab box draws for `page`: an `AdwTab` whose `page` is
/// `page`, among the children of the box's children (the A132 and A133
/// probes' walk). `None` when the box has another shape (fail closed).
fn find_page_tab(
    scroller: &gtk4::ScrolledWindow,
    page: &libadwaita::TabPage,
) -> Option<gtk4::Widget> {
    let tab_box = scroller.child()?;
    let mut container = tab_box.first_child();
    while let Some(current) = container {
        let mut child = current.first_child();
        while let Some(candidate) = child {
            if candidate.type_().name() == TAB_WIDGET_TYPE
                && candidate.find_property("page").is_some()
                && candidate
                    .property::<Option<libadwaita::TabPage>>("page")
                    .is_some_and(|shown| &shown == page)
            {
                return Some(candidate);
            }
            child = candidate.next_sibling();
        }
        container = current.next_sibling();
    }
    None
}

impl LushtextWindow {
    /// Whether the strip shows the whole selected tab (`policy::tab_in_view`
    /// over the tab's bounds in the unpinned scroller). `None` while the
    /// strip is hidden, with no selected page or a pinned one (the pinned
    /// tabs do not scroll with the strip), and when the scroller or the tab
    /// cannot be found or has no bounds yet. A read: it moves nothing.
    pub(crate) fn selected_tab_in_view(&self) -> Option<bool> {
        let imp = self.imp();
        let bar = imp.tab_bar.try_get()?;
        if !bar.is_mapped() {
            return None;
        }
        let page = imp.tab_view.selected_page()?;
        if page.is_pinned() {
            return None;
        }
        let scroller = imp.tab_overflow.unpinned.borrow().clone()?;
        let tab = find_page_tab(&scroller, &page)?;
        let bounds = tab.compute_bounds(&scroller)?;
        Some(tab_in_view(
            whole_px(f64::from(bounds.x())),
            whole_px(f64::from(bounds.width())),
            lushtext_model::values::Sp::new(scroller.width()),
        ))
    }
}

/// `value` rounded to the nearest whole pixel, saturating at the `i32` range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is rounded and clamped to the i32 range first; an adjustment value \
              in pixels never approaches it"
)]
pub(super) fn whole_px(value: f64) -> lushtext_model::values::Sp {
    if value.is_nan() {
        return lushtext_model::values::Sp::ZERO;
    }
    lushtext_model::values::Sp::new(
        value
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
    )
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

impl TabOverflowState {
    /// Whether the pinned tabs overflow their own scroller, read as the
    /// unpinned extent is; false when the lookup did not find it.
    pub(super) fn pinned_overflows(&self) -> bool {
        self.pinned
            .borrow()
            .as_ref()
            .is_some_and(|scroller| strip_overflows(current_extent(&scroller.hadjustment())))
    }
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
        self.animate_tab_strip_to(&adjustment, adjustment.lower() + f64::from(target.as_sp()));
        true
    }

    /// Whether our step animation is playing.
    pub(super) fn tab_scroll_animation_playing(&self) -> bool {
        self.imp().tab_overflow.animation_playing()
    }

    fn animate_tab_strip_to(&self, adjustment: &gtk4::Adjustment, target: f64) {
        let state = &self.imp().tab_overflow;
        // A playing step stops where it is; the new one starts from there.
        let _was_playing = state.pause_step();
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
        let _was_repeating = self.end_tab_scroll_hold();
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
    /// The step in flight finishes; no further step starts. Returns whether
    /// a repeat was pending.
    pub(super) fn end_tab_scroll_hold(&self) -> bool {
        self.imp()
            .tab_overflow
            .repeat
            .take()
            .map(glib::SourceId::remove)
            .is_some()
    }

    /// Inversion terminal: the strip's own input started, or is about to
    /// start, a scroll of its own. Pause our step and drop the repeat, so the
    /// strip's scroll wins (design D3). Counts only when something was active.
    pub(super) fn cancel_tab_scroll_for_strip_input(&self) {
        if self.stop_tab_scroll() {
            let state = &self.imp().tab_overflow;
            state
                .guard_cancellations
                .set(state.guard_cancellations.get().saturating_add(1));
            self.schedule_tab_overflow_reconcile();
        }
    }

    /// Pause our step where it stands and drop the repeat (a cancellation,
    /// disposal). Returns whether either was active.
    pub(super) fn stop_tab_scroll(&self) -> bool {
        let state = &self.imp().tab_overflow;
        state.animation_target.set(None);
        let stepping = state.pause_step();
        let repeating = self.end_tab_scroll_hold();
        stepping || repeating
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
    /// once per frame, after the strip has been allocated, it follows
    /// `policy::reveal_frame`: one re-announcement of the unchanged selection
    /// once the strip comes to rest, the end once it stays still after it (or
    /// at once when neither tab scroller overflows), at most
    /// `SELECTION_REVEAL_MAX_FRAMES` frames. It holds the readiness blocker
    /// for at most `SELECTION_REVEAL_READINESS_US`. While the strip is hidden
    /// the reveal is owed to the bar's next `map` instead, which does not
    /// block readiness.
    pub(in crate::ui::window) fn reveal_selected_tab(&self) {
        self.end_selection_reveal();
        let imp = self.imp();
        let Some(bar) = imp.tab_bar.try_get() else {
            return;
        };
        if imp.tab_view.selected_page().is_none() || imp.tab_overflow.unpinned.borrow().is_none() {
            return;
        }
        if !bar.is_mapped() {
            // The strip is hidden (Focus Mode): owe the reveal until it maps,
            // when the strip shows wherever the hidden attaches left it.
            self.owe_selection_reveal(&bar);
            return;
        }
        self.forget_owed_selection_reveal();
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
                started_at_us: glib::monotonic_time(),
            }));
    }

    /// One frame of the reveal: `Break` once it is done. The decision is
    /// `policy::reveal_frame`'s; this adapter reads the strip, announces, and
    /// ends with no scroller or no selected page.
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
        if imp.tab_view.selected_page().is_none() {
            return glib::ControlFlow::Break;
        }
        let (step, next) = reveal_frame(
            progress.get(),
            current_extent(&adjustment),
            imp.tab_overflow.pinned_overflows(),
            frames,
            frame_time_us,
        );
        progress.set(next);
        match step {
            RevealStep::Wait => glib::ControlFlow::Continue,
            RevealStep::Announce => {
                let state = &imp.tab_overflow;
                state.revealing_selection.set(true);
                imp.tab_view.notify("selected-page");
                state.revealing_selection.set(false);
                glib::ControlFlow::Continue
            }
            RevealStep::End => glib::ControlFlow::Break,
        }
    }

    /// Owe a reveal to the bar's next `map` (inversion: control resumes in
    /// `reveal_selected_tab` from the bar's one-shot `map` handler). A later
    /// request replaces the debt.
    fn owe_selection_reveal(&self, bar: &libadwaita::TabBar) {
        self.forget_owed_selection_reveal();
        let window_weak = self.downgrade();
        let handler = bar.connect_map(move |bar| {
            if let Some(window) = window_weak.upgrade()
                && let Some(handler) = window.imp().tab_overflow.owed_reveal.take()
            {
                bar.disconnect(handler);
                window.reveal_selected_tab();
            }
        });
        self.imp().tab_overflow.owed_reveal.replace(Some(handler));
    }

    /// Drop an owed reveal (a reveal started, disposal). Idempotent.
    pub(super) fn forget_owed_selection_reveal(&self) {
        let handler = self.imp().tab_overflow.owed_reveal.take();
        if let Some(handler) = handler
            && let Some(bar) = self.imp().tab_bar.try_get()
        {
            bar.disconnect(handler);
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
