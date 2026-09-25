// SPDX-License-Identifier: GPL-3.0-or-later

//! Focus Mode presentation for one editor tab.
//!
//! The window owns whether Focus Mode is active. Each editor page owns how that
//! temporary shell state affects its GtkSourceView margins, minimap rendering,
//! and optional typewriter scrolling.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::config::keys;
use crate::ui::window::focus_mode::policy::readable_column_margin;

use super::LushtextEditorPage;

/// Opacity for the text-origin guide.
///
/// The guide needs to explain the centered column without becoming a ruler, so
/// it reuses the theme foreground color with a deliberately low alpha.
const TEXT_ORIGIN_GUIDE_ALPHA: f64 = 0.22;

/// Width of the origin guide in device-independent pixels.
///
/// A single pixel keeps the marker readable on ordinary displays while avoiding
/// a heavy boundary that would make Focus Mode feel more technical than calm.
const TEXT_ORIGIN_GUIDE_WIDTH: f64 = 1.0;

impl LushtextEditorPage {
    /// Create the non-interactive drawing layer used for the Focus Mode text-origin guide.
    ///
    /// The overlay lives with the editor page rather than the window because
    /// GtkTextView owns the actual buffer-to-widget coordinate conversion for
    /// column zero, including gutters and horizontal scrolling.
    pub(crate) fn setup_focus_mode_text_origin_guide(&self) {
        let guide = gtk4::DrawingArea::new();
        guide.set_halign(gtk4::Align::Fill);
        guide.set_valign(gtk4::Align::Fill);
        guide.set_hexpand(true);
        guide.set_vexpand(true);
        guide.set_can_focus(false);
        guide.set_can_target(false);
        guide.set_visible(false);
        guide.add_css_class("focus-mode-text-origin-guide");

        {
            let editor_weak = self.downgrade();
            guide.set_draw_func(move |area, cr, _width, _height| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                draw_text_origin_guide(&editor, area, cr);
            });
        }

        self.imp().overlay.add_overlay(&guide);
        self.imp().overlay.set_measure_overlay(&guide, false);
        self.imp().focus_mode.text_origin_guide.replace(Some(guide));
    }

    /// Install tab-local Focus Mode signal handlers and seed preference-backed state.
    ///
    /// The handlers stay connected for the page lifetime and no-op unless the
    /// window has activated Focus Mode, which keeps cursor changes cheap in
    /// normal editing while avoiding reconnect churn during mode toggles.
    pub(crate) fn setup_focus_mode_presentation(&self) {
        let settings = &self.imp().settings;
        self.imp()
            .focus_mode
            .target_columns
            .set(settings.uint(keys::FOCUS_MODE_TARGET_COLUMNS));
        self.imp()
            .focus_mode
            .typewriter_scrolling
            .set(settings.boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING));

        let buffer = self.buffer();
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_mark_set(move |_, _, mark| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if mark.name().as_deref() == Some("insert") {
                    editor.center_cursor_for_focus_mode();
                }
            });
            self.imp()
                .focus_mode
                .buffer_signals
                .track(&buffer, handler_id);
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.load_projection_suspended() {
                        return;
                    }
                    editor.center_cursor_for_focus_mode();
                }
            });
            self.imp()
                .focus_mode
                .buffer_signals
                .track(&buffer, handler_id);
        }
    }

    /// Apply or clear Focus Mode presentation for this editor page.
    ///
    /// Activating captures the current margins so exit can restore normal
    /// editor presentation exactly, while minimap refresh picks up the temporary
    /// focus suppression without mutating the saved minimap preference.
    pub(crate) fn set_focus_mode_active(&self, active: bool) {
        let state = &self.imp().focus_mode;
        if active == state.active.get() {
            self.refresh_focus_mode_readable_column();
            self.refresh_minimap();
            self.update_focus_mode_text_origin_guide();
            return;
        }

        if active {
            state
                .normal_left_margin
                .set(self.source_view().left_margin());
            state
                .normal_right_margin
                .set(self.source_view().right_margin());
            state.active.set(true);
            self.refresh_focus_mode_readable_column();
        } else {
            state.active.set(false);
            self.source_view()
                .set_left_margin(state.normal_left_margin.get());
            self.source_view()
                .set_right_margin(state.normal_right_margin.get());
        }
        self.refresh_minimap();
        self.update_focus_mode_text_origin_guide();
    }

    /// Update the readable-column target and immediately refresh active margins.
    pub(crate) fn set_focus_mode_target_columns(&self, target_columns: u32) {
        self.imp()
            .focus_mode
            .target_columns
            .set(target_columns.clamp(60, 120));
        self.refresh_focus_mode_readable_column();
    }

    /// Update typewriter scrolling preference state for the current page.
    ///
    /// If Focus Mode is already active this also performs one centering pass so
    /// toggling the preference has an immediate visible effect.
    pub(crate) fn set_focus_mode_typewriter_scrolling(&self, enabled: bool) {
        self.imp().focus_mode.typewriter_scrolling.set(enabled);
        self.center_cursor_for_focus_mode();
    }

    /// Return the current left margin used by widget tests for Focus Mode geometry.
    #[must_use]
    pub fn focus_mode_left_margin(&self) -> i32 {
        self.source_view().left_margin()
    }

    /// Return the current right margin used by widget tests for Focus Mode geometry.
    #[must_use]
    pub fn focus_mode_right_margin(&self) -> i32 {
        self.source_view().right_margin()
    }

    /// Recompute readable-column margins after allocation or preference changes.
    pub(crate) fn refresh_focus_mode_readable_column(&self) {
        let state = &self.imp().focus_mode;
        if !state.active.get() {
            return;
        }

        let margin = readable_column_margin(
            self.source_view().width(),
            approximate_char_width_pango_units(self.source_view().upcast_ref::<gtk4::Widget>()),
            state.target_columns.get(),
        );
        self.source_view().set_left_margin(margin);
        self.source_view().set_right_margin(margin);
        self.update_focus_mode_text_origin_guide();
    }

    /// Report whether the Focus Mode text-origin guide is currently shown.
    ///
    /// Widget tests use this as a behavior-facing query instead of poking at
    /// the private overlay widget directly.
    #[must_use]
    pub fn focus_mode_text_origin_guide_visible(&self) -> bool {
        self.imp()
            .focus_mode
            .text_origin_guide
            .borrow()
            .as_ref()
            .is_some_and(WidgetExt::is_visible)
    }

    /// Return the guide's current horizontal position in editor-page overlay coordinates.
    ///
    /// `None` means GTK has not allocated shared coordinates for the source
    /// view and guide yet, which can happen before the page is presented.
    #[must_use]
    pub fn focus_mode_text_origin_guide_x(&self) -> Option<i32> {
        let guide = self.imp().focus_mode.text_origin_guide.borrow();
        let guide = guide.as_ref()?;
        text_origin_guide_x(self, guide)
    }

    /// Queue a redraw of the origin guide if it has been created.
    pub(crate) fn queue_focus_mode_text_origin_guide_draw(&self) {
        if let Some(guide) = self.imp().focus_mode.text_origin_guide.borrow().as_ref() {
            guide.queue_draw();
        }
    }

    /// Keep guide visibility and drawing aligned with current Focus Mode state.
    fn update_focus_mode_text_origin_guide(&self) {
        let Some(guide) = self
            .imp()
            .focus_mode
            .text_origin_guide
            .borrow()
            .as_ref()
            .cloned()
        else {
            return;
        };
        guide.set_visible(self.imp().focus_mode.active.get());
        guide.queue_draw();
    }

    /// Report whether Focus Mode should temporarily hide this page's minimap.
    pub(crate) fn focus_mode_suppresses_minimap(&self) -> bool {
        self.imp().focus_mode.active.get()
    }

    /// Center the insertion cursor when Focus Mode typewriter scrolling is active.
    fn center_cursor_for_focus_mode(&self) {
        let state = &self.imp().focus_mode;
        if !state.active.get() || !state.typewriter_scrolling.get() {
            return;
        }
        let buffer = self.buffer();
        self.source_view()
            .scroll_to_mark(&buffer.get_insert(), 0.0, true, 0.0, 0.5);
    }
}

/// Draw the visible Focus Mode column-zero guide.
fn draw_text_origin_guide(
    editor: &LushtextEditorPage,
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
) {
    if !editor.imp().focus_mode.active.get() {
        return;
    }

    let Some((x, top, height)) = text_origin_guide_geometry(editor, area) else {
        return;
    };
    let color = area.color();
    cr.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        TEXT_ORIGIN_GUIDE_ALPHA,
    );
    cr.set_line_width(TEXT_ORIGIN_GUIDE_WIDTH);
    cr.move_to(x + 0.5, top);
    cr.line_to(x + 0.5, top + height);
    let _ = cr.stroke();
}

/// Calculate the guide's x coordinate for tests and drawing.
fn text_origin_guide_x(editor: &LushtextEditorPage, guide: &gtk4::DrawingArea) -> Option<i32> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "GTK drawing coordinates are bounded by widget allocations and test queries use i32 pixels"
    )]
    text_origin_guide_geometry(editor, guide).map(|(x, _, _)| x.round() as i32)
}

/// Calculate the text-origin guide line in guide-widget coordinates.
fn text_origin_guide_geometry(
    editor: &LushtextEditorPage,
    guide: &gtk4::DrawingArea,
) -> Option<(f64, f64, f64)> {
    let source_view = editor.source_view();
    let source_bounds = source_view.compute_bounds(guide)?;
    let (origin_x, _) = source_view.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, 0);
    Some((
        f64::from(source_bounds.x()) + f64::from(origin_x) + f64::from(source_view.left_margin()),
        f64::from(source_bounds.y()),
        f64::from(source_bounds.height().max(0.0)),
    ))
}

/// Measure the approximate character width for a GTK text surface, in Pango units.
///
/// Pango's own integer measure (`pango::SCALE` units per pixel) is passed
/// through unconverted, so the whole-pixel readable-column policy never has to
/// form a fraction of a pixel.
pub(crate) fn approximate_char_width_pango_units(widget: &gtk4::Widget) -> i32 {
    let context = widget.pango_context();
    let metrics = context.metrics(None, None);
    metrics.approximate_char_width()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::ui::window::focus_mode::policy::{
        MIN_FOCUS_MARGIN, PANGO_UNITS_PER_PIXEL, readable_column_margin,
    };

    /// The `f64` form the whole-pixel policy replaced, kept verbatim as the
    /// reference: it lives here because it needs the float arithmetic the
    /// policy module forbids. Its character width was the Pango measure
    /// divided by `pango::SCALE`, which is what the adapter used to pass.
    fn legacy_readable_column_margin(
        allocated_width: i32,
        approximate_char_width_pango: i32,
        target_columns: u32,
    ) -> i32 {
        let approximate_char_width =
            f64::from(approximate_char_width_pango) / f64::from(gtk4::pango::SCALE);
        let width = allocated_width.max(0);
        if width <= 0 || approximate_char_width <= 0.0 {
            return MIN_FOCUS_MARGIN;
        }

        let target_width = approximate_char_width * f64::from(target_columns.clamp(60, 120));
        let available_margin = ((f64::from(width) - target_width) / 2.0).floor();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "GTK widget widths and text margins live in i32 pixel coordinates"
        )]
        let margin = available_margin.max(f64::from(MIN_FOCUS_MARGIN)) as i32;
        margin.min(width.saturating_div(3).max(MIN_FOCUS_MARGIN))
    }

    #[test]
    fn the_policy_scale_is_pangos() {
        assert_eq!(PANGO_UNITS_PER_PIXEL, gtk4::pango::SCALE);
    }

    #[test]
    fn integer_margin_matches_the_f64_form_over_a_dense_domain() {
        // Every width a window realistically has, character widths from a
        // fraction of a pixel to sixteen pixels in steps finer than a
        // hundredth of a pixel, and column counts on and around the clamp.
        for target_columns in [0, 59, 60, 61, 72, 80, 100, 119, 120, 121, u32::MAX] {
            for width in -2..=2_600 {
                for char_width in (-3..=16 * 1024).step_by(9) {
                    assert_eq!(
                        readable_column_margin(width, char_width, target_columns),
                        legacy_readable_column_margin(width, char_width, target_columns),
                        "width {width}, char width {char_width} Pango units, {target_columns} columns"
                    );
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20_000))]

        #[test]
        fn integer_margin_matches_the_f64_form_everywhere(
            width in any::<i32>(),
            char_width in any::<i32>(),
            target_columns in any::<u32>(),
        ) {
            prop_assert_eq!(
                readable_column_margin(width, char_width, target_columns),
                legacy_readable_column_margin(width, char_width, target_columns)
            );
        }
    }
}
