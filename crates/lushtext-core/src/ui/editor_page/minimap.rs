// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor minimap workflow for one tab.
//!
//! This module stays in the GTK driving-adapter layer because it wires
//! `GtkSourceMap`, buffer signals, scroll adjustments, and gesture input
//! directly to the editor widget tree. The logic is still kept in its own
//! workflow file so `mod.rs` and `imp.rs` do not become a mixed pile of
//! unrelated editor concerns.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, cairo};
use sourceview5::prelude::*;

use crate::config::keys;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

use super::LushtextEditorPage;

/// Width reserved for the semantic marker strip painted over the map edge.
///
/// Eight pixels is enough to show four stacked marker lanes while still
/// leaving almost all of the overview map readable underneath.
const MINIMAP_MARKER_STRIP_WIDTH: i32 = 8;
/// Debounce for minimap marker refresh work.
///
/// This coalesces bursts from buffer edits, search updates, and resize-driven
/// adjustment changes so the main thread does not rescan the document on every
/// single notify signal.
const MINIMAP_REFRESH_DEBOUNCE: Duration = Duration::from_millis(80);
/// Line length that triggers a long-line warning marker in the minimap.
///
/// The proposal called out 120 characters explicitly, so the marker layer uses
/// the same threshold instead of trying to infer it from unrelated formatting settings.
const MINIMAP_LONG_LINE_WARNING_THRESHOLD: usize = 120;
/// Cap on search matches converted into minimap markers during one refresh.
///
/// The minimap only needs a spatial hint, not every exact hit, so we stop once
/// the marker strip is already dense enough to communicate "many matches here".
const MINIMAP_SEARCH_MATCH_CAP: usize = 2_000;
/// Hidden source-mark category used to keep modified lines attached to buffer edits.
const MINIMAP_MODIFIED_MARK_CATEGORY: &str = "lushtext-minimap-modified";

/// Whether the minimap is currently usable for the active editor state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MinimapAvailability {
    /// The user preference currently disables the minimap everywhere.
    #[default]
    Disabled,
    /// The current document is above the minimap-supported size tier.
    TooLarge,
    /// The tab buffer was evicted and will be reloaded only when focused again.
    Evicted,
    /// The minimap is visible and fully active for this editor page.
    Visible,
}

/// Semantic marker categories painted in the minimap strip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinimapMarkerKind {
    /// Saved-file bookmarks projected from the editor's live bookmark marks.
    Bookmark,
    /// Matches for the active in-tab search session.
    Search,
    /// Lines changed since the last save or clean restore point.
    Modified,
    /// Lines that exceed the minimap long-line threshold.
    LongLine,
}

/// One normalized marker segment rendered in the minimap strip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimapMarker {
    /// Semantic category used to pick color and lane width.
    pub kind: MinimapMarkerKind,
    /// Inclusive start line in the current buffer.
    pub start_line: u32,
    /// Inclusive end line in the current buffer.
    pub end_line: u32,
}

impl LushtextEditorPage {
    /// Report the current minimap availability for this editor page.
    #[must_use]
    pub fn minimap_availability(&self) -> MinimapAvailability {
        self.imp().minimap.availability.get()
    }

    /// Whether the minimap is currently visible for this editor page.
    #[must_use]
    pub fn is_minimap_visible(&self) -> bool {
        self.minimap_availability() == MinimapAvailability::Visible
    }

    /// Count the currently rendered markers for one semantic category.
    ///
    /// This exists mainly so widget tests can assert that bookmark, search,
    /// modified, and long-line markers appear and disappear as expected.
    #[must_use]
    pub fn minimap_marker_count(&self, kind: MinimapMarkerKind) -> usize {
        self.imp()
            .minimap
            .markers
            .borrow()
            .iter()
            .filter(|marker| marker.kind == kind)
            .count()
    }

    /// Install the per-tab minimap widgets and signal glue.
    pub(crate) fn setup_minimap(&self) {
        let imp = self.imp();

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
        source_map.set_top_margin(5);
        source_map.set_bottom_margin(5);
        source_map.set_left_margin(0);
        source_map.set_right_margin(0);
        source_map.set_overflow(gtk4::Overflow::Visible);
        source_map.add_css_class("monospace");
        source_map.add_css_class("minimap-view");
        source_map.set_hexpand(true);
        source_map.set_vexpand(true);

        let marker_strip = gtk4::DrawingArea::new();
        marker_strip.set_width_request(MINIMAP_MARKER_STRIP_WIDTH);
        marker_strip.set_halign(gtk4::Align::End);
        marker_strip.set_valign(gtk4::Align::Fill);
        marker_strip.set_vexpand(true);
        marker_strip.set_can_target(false);
        marker_strip.add_css_class("minimap-marker-strip");

        {
            let editor_weak = self.downgrade();
            marker_strip.set_draw_func(move |_area, cr, width, height| {
                if let Some(editor) = editor_weak.upgrade() {
                    draw_marker_strip(&editor, cr, width, height);
                }
            });
        }

        {
            let editor_weak = self.downgrade();
            let map = source_map.clone();
            let gesture = gtk4::GestureClick::new();
            gesture.connect_pressed(move |gesture, _, _x, y| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                if let Some(editor) = editor_weak.upgrade() {
                    scroll_editor_from_minimap(&editor, y, f64::from(map.height()));
                }
            });
            source_map.add_controller(gesture);
        }
        {
            let editor_weak = self.downgrade();
            let map = source_map.clone();
            let gesture = gtk4::GestureDrag::new();
            gesture.connect_drag_begin(move |gesture, _x, y| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                if let Some(editor) = editor_weak.upgrade() {
                    scroll_editor_from_minimap(&editor, y, f64::from(map.height()));
                }
            });

            let editor_weak = self.downgrade();
            let map = source_map.clone();
            gesture.connect_drag_update(move |gesture, _dx, dy| {
                let Some((_, start_y)) = gesture.start_point() else {
                    return;
                };
                if let Some(editor) = editor_weak.upgrade() {
                    scroll_editor_from_minimap(&editor, start_y + dy, f64::from(map.height()));
                }
            });
            source_map.add_controller(gesture);
        }

        imp.minimap_overlay.add_css_class("minimap-shell");
        imp.minimap_overlay.set_child(Some(&source_map));
        imp.minimap_overlay.add_overlay(&marker_strip);

        *imp.minimap.source_map.borrow_mut() = Some(source_map);
        *imp.minimap.marker_strip.borrow_mut() = Some(marker_strip);
        self.apply_minimap_width_from_settings();

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
                editor.record_modified_lines(start_line, start_line.saturating_add(inserted_lines));
                editor.schedule_minimap_refresh();
            });
            imp.minimap.insert_text_handler_id.replace(Some(handler_id));
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
                editor.record_modified_lines(start_line, end_line);
                editor.schedule_minimap_refresh();
            });
            imp.minimap
                .delete_range_handler_id
                .replace(Some(handler_id));
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_modified_changed(move |buffer| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if !buffer.is_modified() {
                    editor.clear_modified_line_marks();
                }
                editor.schedule_minimap_refresh();
            });
            imp.minimap
                .modified_changed_handler_id
                .replace(Some(handler_id));
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                editor.schedule_minimap_refresh();
            });
            imp.minimap.changed_handler_id.replace(Some(handler_id));
        }

        {
            let editor_weak = self.downgrade();
            self.search_bar().connect_search_state_changed(move || {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
        }

        if let Some(vadj) = self.source_view().vadjustment() {
            let editor_weak = self.downgrade();
            vadj.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
        }

        self.schedule_minimap_refresh();
    }

    /// Refresh minimap visibility, markers, and any one-shot availability feedback.
    pub(crate) fn refresh_minimap(&self) {
        let availability = current_availability(self);
        self.imp().minimap.availability.set(availability);
        if availability != MinimapAvailability::TooLarge {
            self.imp().minimap.too_large_feedback_shown.set(false);
        }

        let overlay = &self.imp().minimap_overlay;
        overlay.set_visible(availability == MinimapAvailability::Visible);

        if availability != MinimapAvailability::Visible {
            self.imp().minimap.markers.borrow_mut().clear();
            self.queue_minimap_draw();
            self.publish_minimap_unavailable_feedback_if_needed(availability);
            return;
        }

        self.apply_minimap_width_from_settings();
        *self.imp().minimap.markers.borrow_mut() = collect_markers(self);
        self.queue_minimap_draw();
    }

    /// Debounce marker recomputation after search, edits, or viewport changes.
    pub(crate) fn schedule_minimap_refresh(&self) {
        let generation = self.imp().minimap.refresh_generation.get().wrapping_add(1);
        self.imp().minimap.refresh_generation.set(generation);

        let editor_weak = self.downgrade();
        glib::timeout_add_local_once(MINIMAP_REFRESH_DEBOUNCE, move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().minimap.refresh_generation.get() != generation {
                return;
            }
            editor.refresh_minimap();
        });
    }

    /// Apply the persisted minimap width to this editor page.
    pub(crate) fn apply_minimap_width_from_settings(&self) {
        let width = self.imp().settings.int(keys::MINIMAP_WIDTH).clamp(64, 160);
        self.imp().minimap_overlay.set_width_request(width);
    }

    /// Queue a redraw of the semantic marker strip when it exists.
    pub(crate) fn queue_minimap_draw(&self) {
        if let Some(strip) = self.imp().minimap.marker_strip.borrow().as_ref() {
            strip.queue_draw();
        }
    }

    /// Temporarily suspend edit tracking while programmatic buffer mutations run.
    pub(crate) fn set_minimap_tracking_suspended(&self, suspended: bool) {
        self.imp().minimap.tracking_suspended.set(suspended);
    }

    /// Clear all modified-since-save markers for this editor.
    pub(crate) fn clear_modified_line_marks(&self) {
        let buffer = self.buffer();
        buffer.remove_source_marks(
            &buffer.start_iter(),
            &buffer.end_iter(),
            Some(MINIMAP_MODIFIED_MARK_CATEGORY),
        );
        self.imp().minimap.modified_marks.borrow_mut().clear();
        self.imp().minimap.modified_lines_cache.borrow_mut().clear();
    }

    /// Mark the inclusive line range as modified-since-save.
    pub(crate) fn record_modified_lines(&self, start_line: u32, end_line: u32) {
        let mut known_lines = self.imp().minimap.modified_lines_cache.borrow_mut();
        let mut marks = self.imp().minimap.modified_marks.borrow_mut();
        let buffer = self.buffer();

        for line in start_line..=end_line {
            if !known_lines.insert(line) {
                continue;
            }
            let iter = iter_at_line_or_last(&buffer, line);
            let mark = buffer.create_source_mark(None, MINIMAP_MODIFIED_MARK_CATEGORY, &iter);
            marks.push(mark);
        }
    }

    /// Mark every current line as modified-since-save.
    ///
    /// Restored drafts are already different from the saved file content, so
    /// this helper lets that programmatic restore surface as minimap feedback.
    pub(crate) fn mark_entire_buffer_modified(&self) {
        self.clear_modified_line_marks();
        let total_lines = document_line_count(self);
        if total_lines == 0 {
            return;
        }
        self.record_modified_lines(0, total_lines.saturating_sub(1));
        self.schedule_minimap_refresh();
    }

    /// Publish the one-shot "too large" message when this page is the active tab.
    fn publish_minimap_unavailable_feedback_if_needed(&self, availability: MinimapAvailability) {
        if availability != MinimapAvailability::TooLarge
            || self.imp().minimap.too_large_feedback_shown.get()
            || !self.imp().settings.boolean(keys::SHOW_MINIMAP)
        {
            return;
        }

        let Some(root) = self
            .root()
            .and_then(|root| root.downcast::<LushtextWindow>().ok())
        else {
            return;
        };

        if !root.is_active_editor(self) {
            return;
        }

        root.publish_status_message(
            "Minimap unavailable for this large document",
            MessageKind::Warning,
        );
        self.imp().minimap.too_large_feedback_shown.set(true);
    }
}

fn current_availability(editor: &LushtextEditorPage) -> MinimapAvailability {
    if !editor.imp().settings.boolean(keys::SHOW_MINIMAP) {
        return MinimapAvailability::Disabled;
    }
    if editor.is_evicted() {
        return MinimapAvailability::Evicted;
    }
    if !editor.size_check().syntax_enabled() {
        return MinimapAvailability::TooLarge;
    }
    MinimapAvailability::Visible
}

fn document_line_count(editor: &LushtextEditorPage) -> u32 {
    u32::try_from(editor.buffer().end_iter().line())
        .unwrap_or(0)
        .saturating_add(1)
        .max(1)
}

fn collect_markers(editor: &LushtextEditorPage) -> Vec<MinimapMarker> {
    let mut markers = Vec::new();
    markers.extend(markers_from_lines(
        MinimapMarkerKind::Bookmark,
        editor
            .bookmark_records()
            .into_iter()
            .map(|bookmark| bookmark.line),
    ));
    markers.extend(markers_from_lines(
        MinimapMarkerKind::Search,
        collect_search_match_lines(editor),
    ));
    markers.extend(markers_from_lines(
        MinimapMarkerKind::Modified,
        collect_modified_lines(editor),
    ));
    markers.extend(markers_from_lines(
        MinimapMarkerKind::LongLine,
        collect_long_line_warnings(editor),
    ));
    markers
}

fn collect_search_match_lines(editor: &LushtextEditorPage) -> Vec<u32> {
    if !editor.is_search_visible() {
        return Vec::new();
    }

    let Some(context) = editor.search_bar().search_context() else {
        return Vec::new();
    };

    let buffer = context.buffer();
    let mut iter = buffer.start_iter();
    let mut lines = std::collections::BTreeSet::new();

    for _ in 0..MINIMAP_SEARCH_MATCH_CAP {
        let Some((match_start, match_end, wrapped)) = context.forward(&iter) else {
            break;
        };
        if wrapped {
            break;
        }
        if let Ok(line) = u32::try_from(match_start.line()) {
            lines.insert(line);
        }

        if match_end == iter {
            if !iter.forward_char() {
                break;
            }
        } else {
            iter = match_end;
        }
    }

    lines.into_iter().collect()
}

fn collect_modified_lines(editor: &LushtextEditorPage) -> Vec<u32> {
    editor
        .imp()
        .minimap
        .modified_marks
        .borrow()
        .iter()
        .filter_map(|mark| u32::try_from(editor.buffer().iter_at_mark(mark).line()).ok())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collect_long_line_warnings(editor: &LushtextEditorPage) -> Vec<u32> {
    editor
        .buffer()
        .text(
            &editor.buffer().start_iter(),
            &editor.buffer().end_iter(),
            true,
        )
        .lines()
        .enumerate()
        .filter_map(|(line, text)| {
            (text.chars().count() > MINIMAP_LONG_LINE_WARNING_THRESHOLD)
                .then(|| u32::try_from(line).ok())
                .flatten()
        })
        .collect()
}

fn markers_from_lines(
    kind: MinimapMarkerKind,
    lines: impl IntoIterator<Item = u32>,
) -> Vec<MinimapMarker> {
    let mut sorted = lines.into_iter().collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted.dedup();
    normalize_line_runs(kind, &sorted)
}

fn normalize_line_runs(kind: MinimapMarkerKind, lines: &[u32]) -> Vec<MinimapMarker> {
    let Some((&first, rest)) = lines.split_first() else {
        return Vec::new();
    };

    let mut markers = Vec::new();
    let mut start = first;
    let mut end = first;
    for &line in rest {
        if line == end.saturating_add(1) {
            end = line;
            continue;
        }
        markers.push(MinimapMarker {
            kind,
            start_line: start,
            end_line: end,
        });
        start = line;
        end = line;
    }
    markers.push(MinimapMarker {
        kind,
        start_line: start,
        end_line: end,
    });
    markers
}

fn draw_marker_strip(editor: &LushtextEditorPage, cr: &cairo::Context, width: i32, height: i32) {
    let width = f64::from(width.max(1));
    let height = f64::from(height.max(1));
    let total_lines = f64::from(document_line_count(editor).max(1));
    let dark = libadwaita::StyleManager::default().is_dark();

    for marker in editor.imp().minimap.markers.borrow().iter() {
        let top = (f64::from(marker.start_line) / total_lines) * height;
        let bottom = (f64::from(marker.end_line.saturating_add(1)) / total_lines) * height;
        let marker_height = (bottom - top).max(2.0);
        let lane_width = marker_lane_width(marker.kind, width);
        let x = width - lane_width;
        let (red, green, blue, alpha) = marker_rgba(marker.kind, dark);
        cr.set_source_rgba(red, green, blue, alpha);
        cr.rectangle(x, top, lane_width, marker_height);
        let _ = cr.fill();
    }
}

fn marker_lane_width(kind: MinimapMarkerKind, total_width: f64) -> f64 {
    let ratio = match kind {
        MinimapMarkerKind::Bookmark => 1.0,
        MinimapMarkerKind::Search => 0.82,
        MinimapMarkerKind::Modified => 0.64,
        MinimapMarkerKind::LongLine => 0.46,
    };
    (total_width * ratio).max(2.0)
}

fn marker_rgba(kind: MinimapMarkerKind, dark: bool) -> (f64, f64, f64, f64) {
    match (kind, dark) {
        (MinimapMarkerKind::Bookmark, false) => (0.11, 0.44, 0.85, 0.95),
        (MinimapMarkerKind::Bookmark, true) => (0.39, 0.65, 0.95, 0.95),
        (MinimapMarkerKind::Search, false) => (0.95, 0.45, 0.0, 0.92),
        (MinimapMarkerKind::Search, true) => (1.0, 0.58, 0.17, 0.92),
        (MinimapMarkerKind::Modified, false) => (0.17, 0.68, 0.42, 0.92),
        (MinimapMarkerKind::Modified, true) => (0.32, 0.84, 0.55, 0.92),
        (MinimapMarkerKind::LongLine, false) => (0.88, 0.11, 0.14, 0.92),
        (MinimapMarkerKind::LongLine, true) => (1.0, 0.46, 0.45, 0.92),
    }
}

fn scroll_editor_from_minimap(editor: &LushtextEditorPage, y: f64, height: f64) {
    let total_lines = document_line_count(editor);
    let target_line = target_line_for_position(total_lines, y, height);
    let mut iter = iter_at_line_or_last(&editor.buffer(), target_line);
    editor
        .source_view()
        .scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.5);
    editor.buffer().place_cursor(&iter);
    editor.source_view().grab_focus();
}

fn target_line_for_position(total_lines: u32, y: f64, height: f64) -> u32 {
    if total_lines <= 1 || height <= 0.0 {
        return 0;
    }

    let clamped = y.clamp(0.0, height);
    let normalized = clamped / height;
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let line = (normalized * f64::from(total_lines)).floor() as u32;
    line.min(total_lines.saturating_sub(1))
}

fn iter_at_line_or_last(buffer: &sourceview5::Buffer, line: u32) -> gtk4::TextIter {
    i32::try_from(line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or_else(|| buffer.end_iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_line_runs_merges_contiguous_lines() {
        let markers = normalize_line_runs(MinimapMarkerKind::Bookmark, &[1, 2, 3, 8, 10, 11]);
        assert_eq!(
            markers,
            vec![
                MinimapMarker {
                    kind: MinimapMarkerKind::Bookmark,
                    start_line: 1,
                    end_line: 3,
                },
                MinimapMarker {
                    kind: MinimapMarkerKind::Bookmark,
                    start_line: 8,
                    end_line: 8,
                },
                MinimapMarker {
                    kind: MinimapMarkerKind::Bookmark,
                    start_line: 10,
                    end_line: 11,
                },
            ]
        );
    }

    #[test]
    fn test_target_line_for_position_clamps_to_document_bounds() {
        assert_eq!(target_line_for_position(100, -5.0, 200.0), 0);
        assert_eq!(target_line_for_position(100, 0.0, 200.0), 0);
        assert_eq!(target_line_for_position(100, 100.0, 200.0), 50);
        assert_eq!(target_line_for_position(100, 400.0, 200.0), 99);
    }

    #[test]
    fn test_markers_from_lines_deduplicates_before_normalizing() {
        let markers = markers_from_lines(MinimapMarkerKind::Search, [4, 4, 5, 9]);
        assert_eq!(
            markers,
            vec![
                MinimapMarker {
                    kind: MinimapMarkerKind::Search,
                    start_line: 4,
                    end_line: 5,
                },
                MinimapMarker {
                    kind: MinimapMarkerKind::Search,
                    start_line: 9,
                    end_line: 9,
                },
            ]
        );
    }
}
