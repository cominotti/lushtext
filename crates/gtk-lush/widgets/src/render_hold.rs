// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rendered-pixel hold overlay for caller-owned reflow workflows.

use std::cell::Cell;

use gtk4::glib;
use gtk4::prelude::*;

/// Result of attempting to capture a render hold.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderHoldCapture {
    /// A new texture was captured and the live child was hidden.
    Captured,
    /// A previous hold is still hiding the live child, so it was preserved.
    AlreadyHolding,
    /// Capture was skipped because GTK did not have usable rendered geometry.
    NotReady(RenderHoldNotReady),
}

/// Reason a render-hold capture could not run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderHoldNotReady {
    /// Either the overlay or child was not mapped or drawable.
    NotMapped,
    /// The child or overlay had no usable allocation.
    EmptyAllocation,
    /// The child had no renderer available through its native.
    MissingRenderer,
    /// The snapshot produced no render node.
    EmptySnapshot,
}

/// Holds rendered child pixels in a non-interactive overlay picture.
///
/// `RenderHoldOverlay` owns the GTK mechanics for an intentional temporary
/// cover: capture child pixels, show them in a `GtkPicture`, hide the live
/// child, warm the live child under the cover, and clear the cover while
/// restoring opacity. Callers own when those phases happen.
///
/// The hold refers to its live child **weakly**: it never keeps a child alive
/// after its owner dropped it. A caller that retires a child (unparents and
/// drops it) gets it finalized; a child pinned by its hold kept running, for
/// example a retired `GtkSourceMap` that went on laying out the document.
pub struct RenderHoldOverlay {
    overlay: gtk4::Overlay,
    live_child: glib::WeakRef<gtk4::Widget>,
    cover: gtk4::Picture,
    active: Cell<bool>,
    warmed: Cell<bool>,
    original_opacity: Cell<f64>,
}

impl RenderHoldOverlay {
    /// Create a render-hold owner and install its cover picture on `overlay`.
    #[must_use]
    pub fn new<W>(overlay: &gtk4::Overlay, live_child: &W) -> Self
    where
        W: IsA<gtk4::Widget>,
    {
        let cover = gtk4::Picture::new();
        cover.set_visible(false);
        cover.set_can_target(false);
        cover.set_content_fit(gtk4::ContentFit::Fill);
        cover.set_halign(gtk4::Align::Fill);
        cover.set_valign(gtk4::Align::Fill);
        cover.add_css_class("gtk-lush-render-hold-cover");
        overlay.add_overlay(&cover);
        overlay.set_measure_overlay(&cover, false);

        Self {
            overlay: overlay.clone(),
            live_child: live_child.upcast_ref::<gtk4::Widget>().downgrade(),
            cover,
            active: Cell::new(false),
            warmed: Cell::new(false),
            original_opacity: Cell::new(1.0),
        }
    }

    /// Return the overlay that owns the cover picture.
    #[must_use]
    pub const fn overlay(&self) -> &gtk4::Overlay {
        &self.overlay
    }

    /// Return the live child being covered, or `None` once its owner dropped it.
    #[must_use]
    pub fn live_child(&self) -> Option<gtk4::Widget> {
        self.live_child.upgrade()
    }

    /// Return the non-targetable cover picture for diagnostics.
    ///
    /// Application code should prefer narrow methods such as
    /// [`add_cover_css_class`](Self::add_cover_css_class) and
    /// [`cover_is_visible`](Self::cover_is_visible). The picture is exposed so
    /// test and automation harnesses can inspect geometry for the actual
    /// surface GTK will snapshot.
    #[must_use]
    pub const fn cover(&self) -> &gtk4::Picture {
        &self.cover
    }

    /// Add a CSS class to the cover picture.
    ///
    /// Prefer this narrow styling hook over mutating the diagnostic cover
    /// widget directly. The cover must stay non-targetable and owned by the
    /// render-hold state machine.
    pub fn add_cover_css_class(&self, css_class: &str) {
        self.cover.add_css_class(css_class);
    }

    /// Return whether the cover picture is currently visible.
    #[must_use]
    pub fn cover_is_visible(&self) -> bool {
        self.cover.is_visible()
    }

    /// Return whether the cover picture can receive input.
    #[must_use]
    pub fn cover_can_target(&self) -> bool {
        self.cover.can_target()
    }

    /// Replace the live child and clear any active hold.
    pub fn set_live_child<W>(&mut self, live_child: &W)
    where
        W: IsA<gtk4::Widget>,
    {
        self.clear();
        self.live_child = live_child.upcast_ref::<gtk4::Widget>().downgrade();
    }

    /// Capture the live child into the cover picture.
    ///
    /// When a hold is already hiding the live child, this returns
    /// `AlreadyHolding` to preserve the original pre-burst pixels.
    #[must_use]
    pub fn capture(&self) -> RenderHoldCapture {
        if self.is_active() {
            return RenderHoldCapture::AlreadyHolding;
        }
        // A dropped live child is not mapped either.
        let Some(live_child) = self.live_child.upgrade() else {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::NotMapped);
        };
        if !self.overlay.is_mapped()
            || !live_child.is_mapped()
            || !self.overlay.is_drawable()
            || !live_child.is_drawable()
        {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::NotMapped);
        }
        if self.overlay.width() <= 0
            || self.overlay.height() <= 0
            || live_child.width() <= 0
            || live_child.height() <= 0
        {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::EmptyAllocation);
        }
        let Some(child_bounds) = live_child.compute_bounds(&self.overlay) else {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::EmptyAllocation);
        };
        if child_bounds.width() <= 0.0 || child_bounds.height() <= 0.0 {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::EmptyAllocation);
        }
        let Some(renderer) = live_child.native().and_then(|native| native.renderer()) else {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::MissingRenderer);
        };

        let snapshot = gtk4::Snapshot::new();
        self.overlay.snapshot_child(&live_child, &snapshot);
        let Some(node) = snapshot.to_node() else {
            return RenderHoldCapture::NotReady(RenderHoldNotReady::EmptySnapshot);
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "GTK widget allocations are logical-pixel values that fit f32 for render viewports"
        )]
        let viewport = gtk4::graphene::Rect::new(
            0.0,
            0.0,
            self.overlay.width() as f32,
            self.overlay.height() as f32,
        );
        let texture = renderer.render_texture(&node, Some(&viewport));
        self.cover.set_paintable(Some(&texture));
        self.cover.set_visible(true);
        self.original_opacity.set(live_child.opacity());
        live_child.set_opacity(0.0);
        self.active.set(true);
        self.warmed.set(false);
        RenderHoldCapture::Captured
    }

    /// Restore live-child opacity while keeping the cover visible.
    pub fn warm_live_child(&self) {
        if !self.cover.is_visible() {
            return;
        }
        if let Some(live_child) = self.live_child.upgrade() {
            live_child.set_opacity(self.original_opacity.get());
            live_child.queue_draw();
        }
        self.warmed.set(true);
    }

    /// Reveal the live child by clearing the active cover.
    pub fn reveal(&self) {
        self.clear();
    }

    /// Clear the cover and restore live-child opacity.
    pub fn clear(&self) {
        if (self.active.get() || self.cover.is_visible())
            && let Some(live_child) = self.live_child.upgrade()
        {
            live_child.set_opacity(self.original_opacity.get());
        }
        self.cover.set_paintable(None::<&gtk4::gdk::Paintable>);
        self.cover.set_visible(false);
        self.active.set(false);
        self.warmed.set(false);
    }

    /// Return whether a hold is currently visible.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.get() && self.cover.is_visible()
    }

    /// Return whether the live child has been warmed under the cover.
    #[must_use]
    pub fn is_warmed(&self) -> bool {
        self.warmed.get()
    }

    #[cfg(test)]
    fn force_active_for_test(&self) {
        let live_child = self.live_child.upgrade().expect("live child");
        self.cover.set_visible(true);
        self.original_opacity.set(live_child.opacity());
        live_child.set_opacity(0.0);
        self.active.set(true);
        self.warmed.set(false);
    }
}

impl Drop for RenderHoldOverlay {
    fn drop(&mut self) {
        self.clear();
        self.overlay.remove_overlay(&self.cover);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gtk_available() -> bool {
        gtk4::init().is_ok()
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn cover_is_non_targetable() {
        if !gtk_available() {
            return;
        }
        let overlay = gtk4::Overlay::new();
        let label = gtk4::Label::new(Some("live"));
        overlay.set_child(Some(&label));

        let hold = RenderHoldOverlay::new(&overlay, &label);

        assert!(!hold.cover_can_target());
        assert!(!hold.cover_is_visible());
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn unmapped_capture_is_not_ready() {
        if !gtk_available() {
            return;
        }
        let overlay = gtk4::Overlay::new();
        let label = gtk4::Label::new(Some("live"));
        overlay.set_child(Some(&label));
        let hold = RenderHoldOverlay::new(&overlay, &label);

        assert_eq!(
            hold.capture(),
            RenderHoldCapture::NotReady(RenderHoldNotReady::NotMapped)
        );
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn warm_and_clear_restore_opacity() {
        if !gtk_available() {
            return;
        }
        let overlay = gtk4::Overlay::new();
        let label = gtk4::Label::new(Some("live"));
        label.set_opacity(0.42);
        overlay.set_child(Some(&label));
        let hold = RenderHoldOverlay::new(&overlay, &label);

        hold.force_active_for_test();
        assert!(hold.is_active());
        assert_eq!(label.opacity(), 0.0);

        hold.warm_live_child();
        assert!(hold.is_warmed());
        assert_eq!(label.opacity(), 0.42);

        hold.clear();
        assert!(!hold.is_active());
        assert!(!hold.is_warmed());
        assert_eq!(label.opacity(), 0.42);
        assert!(!hold.cover_is_visible());
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn a_dropped_live_child_is_not_kept_alive() {
        if !gtk_available() {
            return;
        }
        let overlay = gtk4::Overlay::new();
        let label = gtk4::Label::new(Some("live"));
        overlay.set_child(Some(&label));
        let hold = RenderHoldOverlay::new(&overlay, &label);
        let weak = label.downgrade();

        overlay.set_child(None::<&gtk4::Widget>);
        drop(label);

        assert!(
            weak.upgrade().is_none(),
            "the hold must not keep its child alive"
        );
        assert!(hold.live_child().is_none());
        assert_eq!(
            hold.capture(),
            RenderHoldCapture::NotReady(RenderHoldNotReady::NotMapped)
        );
        hold.clear();
    }
}
