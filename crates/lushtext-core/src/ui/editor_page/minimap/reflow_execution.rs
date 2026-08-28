// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **execution**, for the width-reflow freeze/settle/reveal stage order.
//!
//! One state machine with a shared invariant: **frozen pixels are revealed
//! exactly once, after the settled repair, and never while the burst is still
//! pending.** The freeze, the settled repair, the quiet-window reveal, and the
//! out-of-band early reveal on user scroll all live here together, deliberately.
//! Giving the cover's removal to `retirement` would put
//! `reveal_repaired_minimap_early`'s
//! `reflow_settle.pending() || !reflow_reveal_pending.get()` guard on one side of
//! a role boundary and its setter on the other, which is exactly the class of
//! defect the seam rules exist to make unrepresentable. `retirement` gets
//! analysis payloads, which really are finished with; it does not get a live
//! overlay.
//!
//! ## Two entry actors, and why the difference is a behavior contract
//!
//! `schedule_minimap_reflow_settle_with_freeze` is the **user-action** path: a
//! shell transition calls it on the GTK main thread *before* the split-view
//! animation starts, so the captured cover still holds the exact previously
//! rendered native minimap pixels. `schedule_minimap_reflow_settle` is the
//! **passive observer** path: an allocation- or adjustment-derived signal can
//! fire after GTK has already invalidated or partially realized the native map,
//! so it schedules the settled repair and captures nothing. `.agents/rules/ui.md`
//! states that distinction as a contract, not as an implementation detail; the
//! two differ by one boolean into a shared implementation and must stay two
//! named operations.
//!
//! **Inversions.** Three, all in this module: the `SettleBurst`
//! (`MINIMAP_REFLOW_SETTLE_DEBOUNCE`, 150ms) resuming in
//! `finish_minimap_reflow_settle`; that handle's
//! `schedule_follow_up(MINIMAP_REFLOW_REVEAL_DELAY, ..)` 800ms reveal; and the
//! out-of-band early reveal, which re-enters the same machine from a different
//! actor while the follow-up is still armed.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::SettleHandle;
use gtk_lush_viewport::ViewportAxis;
use gtk_lush_widgets::RenderHoldCapture;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::policy::{
    MINIMAP_REFLOW_REVEAL_DELAY, MINIMAP_REFLOW_SETTLE_DEBOUNCE,
    finite_adjustment_distance_from_lower,
};

impl LushtextEditorPage {
    /// Coalesce a width-reflow burst into one settled minimap repair.
    ///
    /// `AdwOverlaySplitView` sidebar animation allocates a new editor width on
    /// every frame, and GtkTextView revalidates wrapped line heights
    /// asynchronously while that happens. Repair work scheduled per allocation
    /// always lands at least one frame late and reads mid-validation estimates.
    /// Action-owned shell transitions can freeze the rendered map before the
    /// first allocation frame; passive allocation observers only schedule the
    /// later repair so they never capture an unpainted or partially realized map.
    pub(super) fn schedule_minimap_reflow_settle_impl(&self, freeze_rendered_map: bool) {
        let minimap = &self.imp().minimap;
        if !minimap.reflow_settle.pending() {
            minimap.reflow_reveal_pending.set(false);
            *self.imp().overscroll.reflow_pause.borrow_mut() =
                Some(self.imp().overscroll.rest_state.pause());
        }
        if freeze_rendered_map {
            self.freeze_native_minimap_for_reflow();
        }

        minimap.reflow_settle.schedule(
            self,
            MINIMAP_REFLOW_SETTLE_DEBOUNCE,
            move |editor, handle| {
                editor.finish_minimap_reflow_settle(&handle);
            },
        );
    }

    /// Run the one-shot post-reflow repair after the editor width stops moving.
    ///
    /// The repair restores user scroll anchors, reapplies the fixed native-map
    /// geometry from settled document heights, and clears any stale source-map
    /// scroll. If a shell action captured a cover, the live map repaints under
    /// that cover before reveal; passive bursts have no cover and become ready
    /// as soon as the settled repair finishes.
    fn finish_minimap_reflow_settle(&self, handle: &SettleHandle) {
        // Clear the pin first so the geometry sync below applies the settled margin.
        let handle_current = handle.is_current();
        handle.finish_if_current();
        if handle_current {
            self.imp().overscroll.reflow_pause.borrow_mut().take();
        }

        // The rest flag was recorded from user scrolling outside the burst, so
        // a stale GTK-preserved offset during reallocation cannot suppress the
        // top anchor the user actually had before the reflow started.
        if self
            .imp()
            .overscroll
            .rest_state
            .at_lower(ViewportAxis::Vertical)
            && let Some(adjustment) = self.source_view().vadjustment()
        {
            let lower = adjustment.lower();
            if (adjustment.value() - lower).abs() > 0.5 {
                adjustment.set_value(lower);
            }
        }

        self.sync_projection_geometry();
        self.clamp_native_minimap_to_top_if_editor_at_top();
        self.arm_minimap_refresh();
        self.queue_marker_strip_draw();
        if self.minimap_reflow_freeze_visible() {
            self.warm_live_minimap_under_reflow_freeze();
            self.imp().minimap.reflow_reveal_pending.set(true);
        } else {
            // Passive adjustment-driven bursts never captured a cover, so there
            // is no reveal window to protect after the settled repair runs.
            self.imp().minimap.reflow_reveal_pending.set(false);
            self.drop_minimap_reflow_freeze();
            return;
        }
        handle.schedule_follow_up(self, MINIMAP_REFLOW_REVEAL_DELAY, move |editor| {
            if !editor.imp().minimap.reflow_reveal_pending.replace(false) {
                return;
            }
            editor.drop_minimap_reflow_freeze();
        });
    }

    /// Gate the post-repair reveal window on an actually visible frozen cover.
    ///
    /// Passive repairs skip capture; this helper keeps those repairs from
    /// blocking visual-readiness on a reveal delay that has nothing to reveal.
    fn minimap_reflow_freeze_visible(&self) -> bool {
        self.imp()
            .minimap
            .render_hold
            .borrow()
            .as_ref()
            .is_some_and(gtk_lush_widgets::RenderHoldOverlay::is_active)
    }

    /// Cover the native map with its last rendered pixels while reflow settles.
    ///
    /// Shell actions arm the snapshot before the consuming width transition.
    /// Passive allocation-derived observers only schedule the settled repair,
    /// because by the time they fire GTK may already have invalidated or
    /// partially realized the native map. The capture spans the overlay content
    /// box, including the native slider's CSS outset, so the frozen picture
    /// contains the full rendered effect.
    fn freeze_native_minimap_for_reflow(&self) {
        if !self.is_minimap_visible() {
            return;
        }
        let render_hold = self.imp().minimap.render_hold.borrow();
        let Some(render_hold) = render_hold.as_ref() else {
            return;
        };
        match render_hold.capture() {
            RenderHoldCapture::Captured | RenderHoldCapture::AlreadyHolding => {}
            RenderHoldCapture::NotReady(reason) => {
                tracing::trace!(?reason, "skipping minimap reflow render hold");
            }
        }
    }

    /// Let the real source map repaint while frozen pixels still cover it.
    ///
    /// `GtkSourceMap` can still paint its first visible frame from stale private
    /// slider state when it becomes visible again. Restoring opacity before the
    /// cover is removed gives GTK a few real frames to update the native map
    /// while the temporary picture keeps the user-visible effect unchanged.
    fn warm_live_minimap_under_reflow_freeze(&self) {
        let render_hold = self.imp().minimap.render_hold.borrow();
        if let Some(render_hold) = render_hold.as_ref() {
            render_hold.warm_live_child();
            debug_assert!(render_hold.live_child().opacity() >= 0.99);
        }
    }

    /// Reveal the repaired live minimap early when the user scrolls during warmup.
    ///
    /// The initial settle window keeps the cover in place because the native map
    /// is still reading transient geometry. After opacity is restored under the
    /// cover, the live map is ready underneath it, so user-driven scroll
    /// should trade the conservative delay for immediate responsiveness.
    pub(super) fn reveal_repaired_minimap_early(&self) {
        let minimap = &self.imp().minimap;
        if minimap.reflow_settle.pending() || !minimap.reflow_reveal_pending.get() {
            return;
        }

        minimap.reflow_reveal_pending.set(false);
        self.drop_minimap_reflow_freeze();
    }

    /// Remove the frozen overlay and show the live native map again.
    ///
    /// Every exit path must restore source-map opacity before hiding and clearing
    /// the picture, or the next live minimap frame can remain invisible.
    pub(super) fn drop_minimap_reflow_freeze(&self) {
        let render_hold = self.imp().minimap.render_hold.borrow();
        if let Some(render_hold) = render_hold.as_ref() {
            render_hold.clear();
            debug_assert!(render_hold.live_child().opacity() >= 0.99);
            debug_assert!(!render_hold.cover_is_visible());
        }
    }

    /// Restore the bound source map's own vertical adjustment to the top edge.
    ///
    /// Upstream derives the native slider from the editor scroll position and
    /// then subtracts the source map's visible rect. If GTK preserves a stale
    /// source-map adjustment across width-only reflow, the outer allocation can
    /// be stable while the rendered slider and first map row drift by a few
    /// pixels. Returns whether a stale adjustment was actually cleared.
    fn clamp_native_minimap_to_top_if_editor_at_top(&self) -> bool {
        let Some(source_adjustment) = self.source_view().vadjustment() else {
            return false;
        };
        let Some(source_distance) = finite_adjustment_distance_from_lower(
            source_adjustment.value(),
            source_adjustment.lower(),
        ) else {
            return false;
        };
        if source_distance > 0.5 {
            return false;
        }

        let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref().cloned() else {
            return false;
        };
        let Some(map_adjustment) = source_map.vadjustment() else {
            return false;
        };

        let lower = map_adjustment.lower();
        let Some(map_distance) =
            finite_adjustment_distance_from_lower(map_adjustment.value(), lower)
        else {
            return false;
        };
        if map_distance <= 0.5 {
            return false;
        }
        map_adjustment.set_value(lower);
        true
    }
}
