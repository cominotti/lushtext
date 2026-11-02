// SPDX-License-Identifier: GPL-3.0-or-later

//! The fold-marker gutter (`protected-view-inspector-and-surfaces`, design
//! D8): a **called presentation surface** of the Protected View workflow, with
//! no role. A `GtkSourceGutterRendererText` in the left gutter marks each
//! paragraph with a hidden tail. Its per-frame work is one `query-data` per
//! visible row, each a binary search of the fold plan. The tooltip gives the
//! hidden-character count.
//!
//! **Its width never changes at a Protected View entry or exit** (review S4).
//! The text area's width follows the gutter's, and a wrapping GtkTextView
//! whose text width changes lays every paragraph out again, offscreen ones
//! included (ledger A74): on a document of long lines, the whole-document
//! relayout Protected View exists to avoid (a 230 ms dispatch on thirty
//! 40,000-character lines, `test_entry_and_undo_leave_the_gutter_width_alone_evidence`). So the renderer is inserted when the editor is built, at a fixed
//! width, and is visible exactly while the gutter shows line numbers or line
//! marks (a change the user makes, which resizes the gutter anyway); entering
//! Protected View only starts drawing markers in it, and leaving stops. With
//! both off, the gutter stays hidden and Protected View shows no markers: the
//! alert, the Line Inspector's next and previous folded line, and the
//! accessible description carry the same facts.
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

/// Horizontal padding on each side of a marker.
const FOLD_GUTTER_XPAD: i32 = 4;

impl LushtextEditorPage {
    /// Insert the fold-marker renderer, once, while the editor is built. It is
    /// never removed: removing a renderer from a `GtkSourceGutter` releases a
    /// reference the gutter never took (`g_object_unref: assertion
    /// 'G_IS_OBJECT (object)' failed` when the last owner drops it).
    pub(in crate::ui::editor_page) fn install_fold_gutter(&self) {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        let slot = &self.imp().protected_fold_gutter;
        if slot.borrow().is_some() {
            return;
        }
        let view = self.source_view();
        let gutter = sourceview5::prelude::ViewExt::gutter(view, gtk4::TextWindowType::Left);
        // A `group` role: GTK computes no accessible name for the default
        // `generic` role, so the label below would never be exposed.
        let renderer = glib::Object::builder::<sourceview5::GutterRendererText>()
            .property("accessible-role", gtk4::AccessibleRole::Group)
            .build();
        renderer.set_xpad(FOLD_GUTTER_XPAD);
        renderer.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::First);
        // A fixed width, whatever the row shows: the marker's own width plus
        // the padding, so drawing or clearing markers never resizes the gutter.
        let (marker_width, _) = GutterRendererTextExt::measure(&renderer, FOLD_MARKER);
        renderer.set_size_request(marker_width + 2 * FOLD_GUTTER_XPAD, -1);
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
                .filter(LushtextEditorPage::is_protected_view)
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
        // Visible exactly while the gutter shows something else.
        let sync = {
            let renderer = renderer.downgrade();
            move |view: &sourceview5::View| {
                if let Some(renderer) = renderer.upgrade() {
                    renderer.set_visible(view.shows_line_numbers() || view.shows_line_marks());
                }
            }
        };
        sync(view);
        let sync_numbers = sync.clone();
        view.connect_show_line_numbers_notify(move |view| sync_numbers(view));
        view.connect_show_line_marks_notify(move |view| sync(view));
        slot.replace(Some(renderer));
    }

    /// Draw the fold markers (`on`) or stop drawing them: a redraw of the
    /// gutter's rows, never a resize (module docs).
    pub(in crate::ui::editor_page) fn set_fold_gutter(&self, _on: bool) {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        if let Some(renderer) = self.imp().protected_fold_gutter.borrow().as_ref() {
            renderer.queue_draw();
        }
    }

    /// Whether fold markers are being drawn now (evidence): Protected View with
    /// the marker column shown.
    pub(crate) fn fold_markers_shown(&self) -> bool {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.is_protected_view()
            && self
                .imp()
                .protected_fold_gutter
                .borrow()
                .as_ref()
                .is_some_and(gtk4::prelude::WidgetExt::get_visible)
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
