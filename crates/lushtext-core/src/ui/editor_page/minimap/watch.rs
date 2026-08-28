// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **watch**.
//!
//! Maintaining observation of the external sources this workflow reacts to: the
//! buffer's insert/delete/modified/changed signals, the editor and source-map
//! vertical adjustments, and the in-tab search session. Everything installed
//! here that can outlive its closure is tracked in `MinimapState::buffer_signals`
//! (a `SignalBag`) so `dispose()` can disconnect it.
//!
//! This module also builds the native `GtkSourceMap` itself, because widget
//! creation and the observation wired onto it are one installation step: reflow
//! recovery rebuilds the same native widget configuration rather than restyling
//! or drawing over the upstream effect.

use glib::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::policy::{
    MINIMAP_MARKER_STRIP_WIDTH, MINIMAP_VIEWPORT_HORIZONTAL_OUTSET, fitting_source_map_page_size,
};
use super::projection_execution::{draw_marker_strip, sync_source_map_geometry};

impl LushtextEditorPage {
    /// Install the per-tab minimap widgets and signal glue.
    pub(super) fn install_minimap(&self) {
        let imp = self.imp();

        let source_map = self.create_native_source_map();

        let marker_strip = gtk4::DrawingArea::new();
        marker_strip.set_width_request(MINIMAP_MARKER_STRIP_WIDTH);
        marker_strip.set_halign(gtk4::Align::End);
        marker_strip.set_valign(gtk4::Align::Fill);
        marker_strip.set_vexpand(true);
        marker_strip.set_can_target(false);
        // Keep the strip over the map's right edge now that the shell gutter
        // is a map margin instead of CSS padding on the overlay.
        marker_strip.set_margin_end(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET);
        marker_strip.add_css_class("minimap-marker-strip");

        {
            let editor_weak = self.downgrade();
            marker_strip.set_draw_func(move |area, cr, width, height| {
                if let Some(editor) = editor_weak.upgrade() {
                    draw_marker_strip(&editor, area, cr, width, height);
                }
            });
        }

        imp.minimap_overlay.add_css_class("minimap-shell");
        imp.minimap_overlay.set_child(Some(&source_map));
        // Hidden freeze layer for width-reflow bursts. It sits between the map
        // and the marker strip so frozen pixels replace the native slider while
        // live semantic markers stay on top.
        let render_hold =
            gtk_lush_widgets::RenderHoldOverlay::new(&imp.minimap_overlay, &source_map);
        render_hold.add_cover_css_class("minimap-reflow-freeze");
        imp.minimap_overlay.add_overlay(&marker_strip);

        let source_map_vadjustment = source_map.vadjustment();

        *imp.minimap.source_map.borrow_mut() = Some(source_map);
        *imp.minimap.render_hold.borrow_mut() = Some(render_hold);
        *imp.minimap.marker_strip.borrow_mut() = Some(marker_strip);

        if let Some(vadj) = source_map_vadjustment {
            let editor_weak = self.downgrade();
            vadj.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.guard_fitting_source_map_adjustment();
                }
            });
        }

        let buffer = self.buffer();
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_insert_text(move |_, iter, text| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                let start_line = u32::try_from(iter.line()).unwrap_or(0);
                let inserted_lines = u32::try_from(text.chars().filter(|ch| *ch == '\n').count())
                    .unwrap_or(u32::MAX);
                editor.record_modified_line_marks(
                    start_line,
                    start_line.saturating_add(inserted_lines),
                );
                editor.arm_minimap_refresh();
            });
            imp.minimap.buffer_signals.track(&buffer, handler_id);
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_delete_range(move |_, start, end| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                let start_line = u32::try_from(start.line()).unwrap_or(0);
                let end_line = u32::try_from(end.line()).unwrap_or(start_line);
                editor.record_modified_line_marks(start_line, end_line);
                editor.arm_minimap_refresh();
            });
            imp.minimap.buffer_signals.track(&buffer, handler_id);
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_modified_changed(move |buffer| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                if !buffer.is_modified() {
                    editor.release_modified_line_marks();
                }
                editor.arm_minimap_refresh();
            });
            imp.minimap.buffer_signals.track(&buffer, handler_id);
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                editor.discard_minimap_analysis_content();
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                editor.arm_minimap_refresh();
            });
            imp.minimap.buffer_signals.track(&buffer, handler_id);
        }

        {
            let editor_weak = self.downgrade();
            self.search_bar().connect_search_state_changed(move || {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.arm_minimap_refresh();
                }
            });
        }

        if let Some(vadj) = self.source_view().vadjustment() {
            let editor_weak = self.downgrade();
            vadj.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.guard_fitting_source_map_adjustment();
                    editor.arm_minimap_refresh();
                }
            });
        }

        self.guard_fitting_source_map_adjustment();
        self.arm_minimap_refresh();
    }

    /// Keep GtkSourceMap 5.20 out of its zero-range adjustment division.
    ///
    /// Upstream maps the editor value with `value / (upper - page_size)` when
    /// its own child adjustment still looks scrollable. A fitting source view
    /// has a zero denominator, so mirror the semantic no-scroll state onto the
    /// map adjustment before its queued frame callback runs.
    fn guard_fitting_source_map_adjustment(&self) {
        let Some(source_adjustment) = self.source_view().vadjustment() else {
            return;
        };
        let Some(map_adjustment) = self
            .imp()
            .minimap
            .source_map
            .borrow()
            .as_ref()
            .and_then(sourceview5::prelude::ScrollableExt::vadjustment)
        else {
            return;
        };
        let Some(page_size) = fitting_source_map_page_size(
            source_adjustment.upper(),
            source_adjustment.page_size(),
            map_adjustment.upper(),
            map_adjustment.page_size(),
        ) else {
            return;
        };
        map_adjustment.set_page_size(page_size);
    }

    /// Build a native `GtkSourceMap` with LushText's stable minimap presentation.
    ///
    /// The map owns the visible viewport highlight. Keeping creation in one
    /// helper lets reflow recovery rebuild the same native widget configuration
    /// instead of restyling or drawing over the upstream effect.
    fn create_native_source_map(&self) -> sourceview5::Map {
        let source_map = sourceview5::Map::new();
        source_map.set_view(self.source_view());
        source_map.set_editable(false);
        source_map.set_cursor_visible(false);
        source_map.set_can_focus(false);
        source_map.set_wrap_mode(gtk4::WrapMode::None);
        source_map.set_show_line_numbers(false);
        source_map.set_show_line_marks(false);
        source_map.set_highlight_current_line(false);
        source_map.set_monospace(true);
        source_map.set_overflow(gtk4::Overflow::Visible);
        source_map.add_css_class("monospace");
        source_map.add_css_class("minimap-view");
        source_map.set_hexpand(true);
        source_map.set_vexpand(true);
        // The shell's side gutters are widget margins rather than CSS padding
        // so the reflow freeze picture can span the full shell content box
        // (map plus the native slider's CSS outset) without negative margins.
        source_map.set_margin_start(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET);
        source_map.set_margin_end(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET);
        sync_source_map_geometry(&source_map, self.source_view());
        source_map
    }
}
