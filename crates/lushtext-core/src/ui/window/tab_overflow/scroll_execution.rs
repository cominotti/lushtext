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
    TAB_SCROLL_REPEAT_INTERVAL_MS, can_scroll_back, can_scroll_forward, next_scroll_target,
};

/// The deepest descendant level of an `AdwTabBar` the lookup walks: bar →
/// revealer → box → scrolled window (`adw-tab-bar.ui`, ledger A132).
///
/// Kind: external fact; the depth of Libadwaita's tab-bar template (ledger A132).
const TAB_SCROLLER_LOOKUP_DEPTH: usize = 3;
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
        let movable = match direction {
            ScrollDirection::Back => can_scroll_back(extent, rtl),
            ScrollDirection::Forward => can_scroll_forward(extent, rtl),
        };
        if !movable {
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
        self.imp()
            .tab_overflow
            .animation
            .borrow()
            .as_ref()
            .is_some_and(|animation| animation.state() == libadwaita::AnimationState::Playing)
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

    /// Whether a step or a hold is active, which is when the strip observers
    /// act at all.
    pub(super) fn tab_scroll_step_active(&self) -> bool {
        self.tab_scroll_animation_playing() || self.imp().tab_overflow.repeat.borrow().is_some()
    }
}
