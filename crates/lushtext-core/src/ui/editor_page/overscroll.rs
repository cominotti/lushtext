// SPDX-License-Identifier: GPL-3.0-or-later

//! Dynamic end-of-document overscroll for one editor tab.
//!
//! This workflow stays in the editor-page adapter layer because it reacts to
//! GTK allocation and updates the live `GtkSourceView` margin that
//! `GtkSourceMap` mirrors. Keeping it separate from `imp.rs` makes the "extra
//! tail room near EOF" behavior easier to reason about without mixing it into
//! template wiring or minimap marker logic.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextEditorPage;

/// Smallest bottom margin kept even before the view has a useful visible rect.
///
/// The UI template starts with 6px of breathing room, so overscroll updates
/// should never collapse below that baseline while the page is being realized.
const MIN_EDITOR_BOTTOM_MARGIN: i32 = 6;
/// Fraction of the visible editor height reserved as EOF overscroll tail room.
///
/// GNOME Text Editor uses 75% of the current visible rect, which leaves enough
/// blank space for the last lines and minimap slider to keep traveling near EOF
/// without making the document feel detached from the viewport.
const EOF_OVERSCROLL_FACTOR: f64 = 0.75;

/// Scroll axis observed by the viewport reflow detectors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewportAxis {
    /// The horizontal adjustment, whose page size is the viewport width.
    Horizontal,
    /// The vertical adjustment, whose page size is the viewport height.
    Vertical,
}

fn adjustment_rests_at_lower(adjustment: &gtk4::Adjustment) -> bool {
    (adjustment.value() - adjustment.lower()).abs() <= 0.5
}

fn overscroll_margin_from_visible_height(visible_height: i32) -> i32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "GTK visible rect heights are non-negative i32 values, and the 75% overscroll target intentionally rounds back into the same pixel domain"
    )]
    let overscroll = (f64::from(visible_height) * EOF_OVERSCROLL_FACTOR).round() as i32;
    overscroll.max(MIN_EDITOR_BOTTOM_MARGIN)
}

impl LushtextEditorPage {
    /// Observe the editor viewport through its scroll adjustments.
    ///
    /// `LushtextEditorPage` is a `GtkBox` subclass, and GTK4 never calls the
    /// `size_allocate` vfunc on widgets whose class installs a layout manager,
    /// so an allocation override on the page is silently dead code. The text
    /// view's adjustments are the reliable public signal: GtkTextView updates
    /// their page size on every allocation, including each frame of the
    /// sidebar show/hide animation, so width and height reflow can be detected
    /// without touching private GTK layout machinery.
    pub(crate) fn setup_allocation_reflow_observers(&self) {
        let source_view = self.source_view();
        let Some(hadjustment) = source_view.hadjustment() else {
            return;
        };
        let Some(vadjustment) = source_view.vadjustment() else {
            return;
        };

        let overscroll = &self.imp().overscroll;
        overscroll.h_viewport_size.set(hadjustment.page_size());
        overscroll.v_viewport_size.set(vadjustment.page_size());
        overscroll
            .h_rest_at_left
            .set(adjustment_rests_at_lower(&hadjustment));
        overscroll
            .v_rest_at_top
            .set(adjustment_rests_at_lower(&vadjustment));

        for (adjustment, axis) in [
            (hadjustment, ViewportAxis::Horizontal),
            (vadjustment, ViewportAxis::Vertical),
        ] {
            // Adjustment signals are GObject observer callbacks; weak refs keep
            // them from retaining an editor after its tab has been closed.
            let editor_weak = self.downgrade();
            adjustment.connect_changed(move |adjustment| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.on_viewport_bounds_changed(adjustment, axis);
                }
            });
            let editor_weak = self.downgrade();
            adjustment.connect_value_changed(move |adjustment| {
                if let Some(editor) = editor_weak.upgrade() {
                    // During settle this is a no-op; during reveal warmup it
                    // drops the cover before recording the new rest state.
                    editor.reveal_minimap_reflow_freeze_for_user_scroll();
                    editor.record_viewport_rest_state(adjustment, axis);
                }
            });
        }
    }

    /// Track whether the user-visible scroll position rests at the start edge.
    ///
    /// Reflow bursts are excluded because GTK can transiently preserve or
    /// clamp adjustment values while reallocating; only changes outside a
    /// burst represent intentional scrolling. The rest state is what the
    /// settle repair consults to decide whether edge anchors may be restored.
    fn record_viewport_rest_state(&self, adjustment: &gtk4::Adjustment, axis: ViewportAxis) {
        if self.imp().minimap.reflow_settle_pending.get() {
            return;
        }
        let at_lower = adjustment_rests_at_lower(adjustment);
        let overscroll = &self.imp().overscroll;
        match axis {
            ViewportAxis::Horizontal => overscroll.h_rest_at_left.set(at_lower),
            ViewportAxis::Vertical => overscroll.v_rest_at_top.set(at_lower),
        }
    }

    /// React to a viewport size change reported by a scroll adjustment.
    ///
    /// This is the working replacement for the page-level allocation hook:
    /// width changes open or extend a minimap reflow burst and re-anchor the
    /// left edge, height changes re-anchor the top edge, and either kind
    /// refreshes the EOF overscroll and Focus Mode width-derived chrome.
    fn on_viewport_bounds_changed(&self, adjustment: &gtk4::Adjustment, axis: ViewportAxis) {
        let overscroll = &self.imp().overscroll;
        let page_size = adjustment.page_size();
        let cell = match axis {
            ViewportAxis::Horizontal => &overscroll.h_viewport_size,
            ViewportAxis::Vertical => &overscroll.v_viewport_size,
        };
        if (cell.get() - page_size).abs() <= 0.5 {
            return;
        }
        cell.set(page_size);

        self.schedule_dynamic_overscroll_update();
        match axis {
            ViewportAxis::Horizontal => {
                self.schedule_minimap_reflow_settle();
                if overscroll.h_rest_at_left.get() {
                    self.schedule_left_edge_horizontal_scroll_clamp();
                }
            }
            ViewportAxis::Vertical => {
                self.note_minimap_height_reflow();
                if overscroll.v_rest_at_top.get() {
                    self.schedule_top_edge_vertical_scroll_clamp();
                }
            }
        }
        self.refresh_focus_mode_readable_column();
        self.queue_focus_mode_text_origin_guide_draw();
    }

    /// Restore the left text edge after passive width-only layout changes.
    ///
    /// GTK may preserve the previous horizontal adjustment while the editor is
    /// being narrowed. When the user was already at the left edge, that
    /// preservation turns into a stale right-biased viewport, so clamp after the
    /// new allocation settles.
    pub(crate) fn schedule_left_edge_horizontal_scroll_clamp(&self) {
        let editor_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            let Some(adjustment) = editor.source_view().hadjustment() else {
                return;
            };
            let lower = adjustment.lower();
            if (adjustment.value() - lower).abs() > 0.5 {
                adjustment.set_value(lower);
                editor.schedule_minimap_refresh();
            }
        });
    }

    /// Restore the top text edge after passive height-only layout changes.
    ///
    /// `AdwBottomSheet` overlays can shorten the visible editor without changing
    /// the document. When the user was already at the first line, GTK's preserved
    /// vertical adjustment should not leave line one clipped under the chrome.
    pub(crate) fn schedule_top_edge_vertical_scroll_clamp(&self) {
        let editor_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            let Some(adjustment) = editor.source_view().vadjustment() else {
                return;
            };
            let lower = adjustment.lower();
            if (adjustment.value() - lower).abs() > 0.5 {
                adjustment.set_value(lower);
                editor.schedule_minimap_refresh();
            }
        });
    }

    /// Coalesce repeated size allocations into one idle overscroll refresh.
    pub(crate) fn schedule_dynamic_overscroll_update(&self) {
        let generation = self
            .imp()
            .overscroll
            .update_generation
            .get()
            .wrapping_add(1);
        self.imp().overscroll.update_generation.set(generation);

        let editor_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().overscroll.update_generation.get() != generation {
                return;
            }
            editor.refresh_dynamic_overscroll();
        });
    }

    /// Recompute the editor's bottom margin from the current visible rect.
    ///
    /// This mirrors GNOME Text Editor's approach: keep extra blank tail space
    /// after the last line so both the main editor and the bound source map
    /// retain usable travel near EOF.
    pub(crate) fn refresh_dynamic_overscroll(&self) {
        let source_view = self.source_view();
        if !source_view.is_mapped() {
            return;
        }

        let visible_rect = source_view.visible_rect();
        let desired_margin = overscroll_margin_from_visible_height(visible_rect.height());

        if source_view.bottom_margin() == desired_margin {
            return;
        }

        source_view.set_bottom_margin(desired_margin);
        self.sync_minimap_view_geometry();
        self.schedule_minimap_refresh();
        self.queue_minimap_draw();
    }
}
