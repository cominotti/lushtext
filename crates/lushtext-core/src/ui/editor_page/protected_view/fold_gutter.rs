// SPDX-License-Identifier: GPL-3.0-or-later

//! The fold-marker gutter (`protected-view-inspector-and-surfaces`, design
//! D8): a **called presentation surface** of the Protected View workflow, with
//! no role. A `GtkSourceGutterRendererText` in the left gutter, shown at entry
//! and hidden at exit, marks each paragraph with a hidden tail. Its
//! per-frame work is one `query-data` per visible row, each a binary search of
//! the fold plan. The tooltip gives the hidden-character count.
//!
//! Presses on the gutter never reach the renderer: the capture-phase gesture
//! (`interaction_execution`) claims them, and a single primary click on a
//! folded line's gutter opens the Line Inspector (`PressHit::FoldMarker`).

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::ui::accessibility;
use crate::ui::editor_page::LushtextEditorPage;

/// The marker drawn beside a folded paragraph.
const FOLD_MARKER: &str = "⋯";

/// The gutter position of the fold markers: after the line numbers and marks
/// GtkSourceView inserts at negative positions.
const FOLD_GUTTER_POSITION: i32 = 8;

impl LushtextEditorPage {
    /// Show the fold-marker renderer (`on`) or hide it. It is inserted at
    /// the first entry and then only shown and hidden: removing a renderer
    /// from a `GtkSourceGutter` releases a reference the gutter never took
    /// (`g_object_unref: assertion 'G_IS_OBJECT (object)' failed` when the
    /// last owner drops it), and a hidden renderer is neither measured nor
    /// drawn.
    pub(in crate::ui::editor_page) fn set_fold_gutter(&self, on: bool) {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        let slot = &self.imp().protected_fold_gutter;
        if let Some(renderer) = slot.borrow().as_ref() {
            renderer.set_visible(on);
            return;
        }
        if !on {
            return;
        }
        let view = self.source_view();
        let gutter = sourceview5::prelude::ViewExt::gutter(view, gtk4::TextWindowType::Left);
        let renderer = sourceview5::GutterRendererText::new();
        renderer.set_xpad(4);
        renderer.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::First);
        accessibility::set_label(&renderer, "Folded lines");
        accessibility::set_description(
            &renderer,
            "A marker shows a line whose end is hidden. Click the marker or press \
             Ctrl+Alt+I to inspect the line.",
        );
        let page = self.downgrade();
        renderer.connect_query_data(move |renderer, _lines, line| {
            let folded = page
                .upgrade()
                .and_then(|page| page.protected_fold_plan())
                .is_some_and(|plan| plan.range_of(u64::from(line)).is_some());
            renderer.set_text(if folded { FOLD_MARKER } else { "" });
        });
        renderer.set_has_tooltip(true);
        let page = self.downgrade();
        renderer.connect_query_tooltip(move |renderer, _x, y, _keyboard, tooltip| {
            let Some(page) = page.upgrade() else {
                return false;
            };
            let Some(text) = page.fold_marker_tooltip(renderer.upcast_ref(), f64::from(y)) else {
                return false;
            };
            tooltip.set_text(Some(&text));
            true
        });
        gutter.insert(&renderer, FOLD_GUTTER_POSITION);
        slot.replace(Some(renderer));
    }

    /// The tooltip of the marker at `y` in `widget`'s coordinates.
    fn fold_marker_tooltip(&self, widget: &gtk4::Widget, y: f64) -> Option<String> {
        let paragraph = self.paragraph_at_widget_y(widget, y)?;
        let range = self.protected_fold_plan()?.range_of(paragraph)?;
        Some(format!(
            "Line {}: {} characters hidden. Click to inspect the line.",
            paragraph + 1,
            range.tail_chars
        ))
    }

    /// The paragraph (GTK line) at `y` in `widget`'s coordinates, `widget`
    /// being the view or one of its descendants.
    pub(in crate::ui::editor_page) fn paragraph_at_widget_y(
        &self,
        widget: &gtk4::Widget,
        y: f64,
    ) -> Option<u64> {
        let view = self.source_view();
        // Widget coordinates are `f32` in graphene and integers in
        // GtkTextView's window coordinates: whole pixels either way.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a widget coordinate converted to graphene's f32 and to GtkTextView's integer window coordinates"
        )]
        let view_y = {
            let point = widget.compute_point(view, &gtk4::graphene::Point::new(0.0, y as f32))?;
            point.y().round() as i32
        };
        let (_, buffer_y) = view.window_to_buffer_coords(gtk4::TextWindowType::Widget, 0, view_y);
        let (line, _) = view.line_at_y(buffer_y);
        u64::try_from(line.line()).ok()
    }

    /// Whether `x` in the view's coordinates lies over the left gutter.
    pub(in crate::ui::editor_page) fn x_is_over_left_gutter(&self, x: f64) -> bool {
        let view = self.source_view();
        let gutter = sourceview5::prelude::ViewExt::gutter(view, gtk4::TextWindowType::Left);
        gutter
            .compute_bounds(view)
            .is_some_and(|bounds| f64::from(bounds.x() + bounds.width()) > x && x >= 0.0)
    }
}
