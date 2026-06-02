// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor minimap workflow for one tab.
//!
//! This module stays in the GTK driving-adapter layer because it wires
//! `GtkSourceMap`, buffer signals, and scroll adjustments directly to the
//! editor widget tree. The logic is still kept in its own workflow file so
//! `mod.rs` and `imp.rs` do not become a mixed pile of unrelated editor
//! concerns.

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
/// Maximum document size before wrapped minimap layout needs a long-line check.
///
/// Ordinary prose and source files stay below this budget, while multi-megabyte
/// minified files can make the narrow source map build a very large visual-line
/// cache when it mirrors editor word wrap.
const MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET: u64 = 2 * 1024 * 1024;
/// Long logical lines above this size make wrapped source-map layout expensive.
///
/// The threshold is high enough for normal code/log lines but catches minified
/// JSON or generated files before a 64-160px minimap explodes one line into
/// thousands of visual rows.
const MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET: usize = 8_000;
/// Minimum visual height for a semantic marker after projection.
///
/// Collapsed or sub-pixel source-map lines still need to be discoverable, but
/// this height is clamped to the rendered document content so it cannot leak
/// into the blank EOF overscroll tail.
const MINIMAP_MARKER_MIN_HEIGHT: f64 = 2.0;
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

/// Current vertical bounds for a projected minimap marker segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapMarkerBounds {
    /// Semantic category used to pick color and lane width.
    pub kind: MinimapMarkerKind,
    /// Marker top in marker-strip widget coordinates.
    pub top: f64,
    /// Marker bottom in marker-strip widget coordinates.
    pub bottom: f64,
}

impl MinimapMarkerBounds {
    /// Height in marker-strip widget coordinates.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
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

    /// Return currently drawable marker bounds for one semantic category.
    ///
    /// The bounds are projected through the real `GtkSourceMap` layout, so
    /// widget tests can assert the marker strip follows source-map geometry
    /// instead of a hand-rolled line-count ratio.
    #[must_use]
    pub fn minimap_marker_bounds(&self, kind: MinimapMarkerKind) -> Vec<MinimapMarkerBounds> {
        let imp = self.imp();
        let Some(source_map) = imp.minimap.source_map.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let Some(marker_strip) = imp.minimap.marker_strip.borrow().as_ref().cloned() else {
            return Vec::new();
        };

        projected_minimap_marker_bounds(self, &source_map, &marker_strip, marker_strip.height())
            .into_iter()
            .filter(|bounds| bounds.kind == kind)
            .collect()
    }

    /// Install the per-tab minimap widgets and signal glue.
    pub(crate) fn setup_minimap(&self) {
        let imp = self.imp();

        let source_map = sourceview5::Map::new();
        source_map.set_view(self.source_view());
        source_map.set_editable(false);
        source_map.set_cursor_visible(false);
        source_map.set_can_focus(false);
        source_map.set_wrap_mode(self.source_view().wrap_mode());
        source_map.set_show_line_numbers(false);
        source_map.set_show_line_marks(false);
        source_map.set_highlight_current_line(false);
        source_map.set_monospace(true);
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
            marker_strip.set_draw_func(move |area, cr, width, height| {
                if let Some(editor) = editor_weak.upgrade() {
                    draw_marker_strip(&editor, area, cr, width, height);
                }
            });
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
                editor.imp().minimap.wrapped_layout_too_large.set(None);
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

    /// Keep the source map's wrapping policy aligned with the main editor.
    ///
    /// The minimap viewport is a visual promise about the editor, so width
    /// reflow from word wrap must be reflected in the map before its native
    /// viewport slider and our marker strip settle.
    pub(crate) fn sync_minimap_wrap_mode(&self) {
        let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref().cloned() else {
            return;
        };
        let wrap_mode = self.source_view().wrap_mode();
        if source_map.wrap_mode() != wrap_mode {
            source_map.set_wrap_mode(wrap_mode);
        }
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
    minimap_availability_for_policy(MinimapAvailabilityPolicy {
        focus_suppressed: editor.focus_mode_suppresses_minimap(),
        preference_enabled: editor.imp().settings.boolean(keys::SHOW_MINIMAP),
        evicted: editor.is_evicted(),
        syntax_enabled: editor.size_check().syntax_enabled(),
        wrapped_layout_too_large: wrapped_minimap_layout_exceeds_budget(editor),
    })
}

/// Pure availability inputs gathered from GTK state by `current_availability`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MinimapAvailabilityPolicy {
    focus_suppressed: bool,
    preference_enabled: bool,
    evicted: bool,
    syntax_enabled: bool,
    wrapped_layout_too_large: bool,
}

fn minimap_availability_for_policy(policy: MinimapAvailabilityPolicy) -> MinimapAvailability {
    if policy.focus_suppressed || !policy.preference_enabled {
        return MinimapAvailability::Disabled;
    }
    if policy.evicted {
        return MinimapAvailability::Evicted;
    }
    if !policy.syntax_enabled || policy.wrapped_layout_too_large {
        return MinimapAvailability::TooLarge;
    }
    MinimapAvailability::Visible
}

fn wrapped_minimap_layout_exceeds_budget(editor: &LushtextEditorPage) -> bool {
    if editor.source_view().wrap_mode() == gtk4::WrapMode::None {
        return false;
    }
    if let Some(cached) = editor.imp().minimap.wrapped_layout_too_large.get() {
        return cached;
    }

    let buffer = editor.buffer();
    let buffer_chars = u64::try_from(buffer.char_count()).unwrap_or(u64::MAX);
    let estimated_size = editor.file_size().unwrap_or(0).max(buffer_chars);
    let exceeds = wrapped_layout_budget_exceeded(
        estimated_size,
        buffer_has_line_exceeding_char_budget(&buffer, MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET),
    );
    editor
        .imp()
        .minimap
        .wrapped_layout_too_large
        .set(Some(exceeds));
    exceeds
}

fn wrapped_layout_budget_exceeded(estimated_size: u64, has_extreme_line: bool) -> bool {
    estimated_size > MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET && has_extreme_line
}

fn buffer_has_line_exceeding_char_budget(
    buffer: &sourceview5::Buffer,
    line_char_budget: usize,
) -> bool {
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    text_exceeds_line_char_budget(&text, line_char_budget)
}

fn text_exceeds_line_char_budget(text: &str, line_char_budget: usize) -> bool {
    let mut current_line_chars = 0usize;

    for ch in text.chars() {
        if ch == '\n' {
            current_line_chars = 0;
        } else {
            current_line_chars = current_line_chars.saturating_add(1);
            if current_line_chars > line_char_budget {
                return true;
            }
        }
    }

    false
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
    if editor
        .imp()
        .settings
        .boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE)
    {
        markers.extend(markers_from_lines(
            MinimapMarkerKind::LongLine,
            collect_long_line_warnings(editor),
        ));
    }
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
    let text = editor.buffer().text(
        &editor.buffer().start_iter(),
        &editor.buffer().end_iter(),
        true,
    );
    long_line_warning_lines(&text)
}

fn long_line_warning_lines(text: &str) -> Vec<u32> {
    text.lines()
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

fn draw_marker_strip(
    editor: &LushtextEditorPage,
    marker_strip: &gtk4::DrawingArea,
    cr: &cairo::Context,
    width: i32,
    height: i32,
) {
    let width = f64::from(width.max(1));
    let dark = libadwaita::StyleManager::default().is_dark();
    let Some(source_map) = editor.imp().minimap.source_map.borrow().as_ref().cloned() else {
        return;
    };
    let markers = projected_minimap_marker_bounds(editor, &source_map, marker_strip, height);

    for marker in &markers {
        let lane_width = marker_lane_width(marker.kind, width);
        let x = marker_lane_x(width, lane_width);
        let (red, green, blue, alpha) = marker_rgba(marker.kind, dark);
        cr.set_source_rgba(red, green, blue, alpha);
        cr.rectangle(x, marker.top, lane_width, marker.height());
        let _ = cr.fill();
    }
}

fn projected_minimap_marker_bounds(
    editor: &LushtextEditorPage,
    source_map: &sourceview5::Map,
    marker_strip: &gtk4::DrawingArea,
    strip_height: i32,
) -> Vec<MinimapMarkerBounds> {
    let Some(space) = marker_projection_space(source_map, marker_strip, strip_height) else {
        return Vec::new();
    };

    editor
        .imp()
        .minimap
        .markers
        .borrow()
        .iter()
        .filter_map(|marker| project_marker_bounds(marker, source_map, space))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkerProjectionSpace {
    strip_height: f64,
    content_top: f64,
    content_bottom: f64,
    map_y_in_strip: f64,
    min_height: f64,
}

fn marker_projection_space(
    source_map: &sourceview5::Map,
    marker_strip: &gtk4::DrawingArea,
    strip_height: i32,
) -> Option<MarkerProjectionSpace> {
    if strip_height <= 0 || !source_map.is_mapped() || !marker_strip.is_mapped() {
        return None;
    }

    let map_bounds = source_map.compute_bounds(marker_strip)?;
    let map_y_in_strip = f64::from(map_bounds.y());
    let buffer = source_map.buffer();
    let start_iter = buffer.start_iter();
    let end_line = u32::try_from(buffer.end_iter().line()).unwrap_or_default();
    let end_iter = text_iter_at_line_or_last(&buffer, end_line);
    let content_top = line_top_in_strip(source_map, map_y_in_strip, &start_iter);
    let content_bottom = line_bottom_in_strip(source_map, map_y_in_strip, &end_iter);

    if !content_top.is_finite() || !content_bottom.is_finite() || content_bottom <= content_top {
        return None;
    }

    Some(MarkerProjectionSpace {
        strip_height: f64::from(strip_height),
        content_top,
        content_bottom,
        map_y_in_strip,
        min_height: MINIMAP_MARKER_MIN_HEIGHT,
    })
}

fn project_marker_bounds(
    marker: &MinimapMarker,
    source_map: &sourceview5::Map,
    space: MarkerProjectionSpace,
) -> Option<MinimapMarkerBounds> {
    let buffer = source_map.buffer();
    let start_iter = text_iter_at_line_or_last(&buffer, marker.start_line);
    let end_iter = text_iter_at_line_or_last(&buffer, marker.end_line);
    let raw_top = line_top_in_strip(source_map, space.map_y_in_strip, &start_iter);
    let raw_bottom = line_bottom_in_strip(source_map, space.map_y_in_strip, &end_iter);

    fit_marker_bounds(marker.kind, raw_top, raw_bottom, space)
}

fn text_iter_at_line_or_last(buffer: &gtk4::TextBuffer, line: u32) -> gtk4::TextIter {
    let end_iter = buffer.end_iter();
    let end_line = u32::try_from(end_iter.line()).unwrap_or_default();
    let clamped_line = line.min(end_line);

    i32::try_from(clamped_line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or(end_iter)
}

fn line_top_in_strip(
    source_map: &sourceview5::Map,
    map_y_in_strip: f64,
    iter: &gtk4::TextIter,
) -> f64 {
    let (line_y, _) = source_map.line_yrange(iter);
    buffer_y_to_strip_y(source_map, map_y_in_strip, line_y)
}

fn line_bottom_in_strip(
    source_map: &sourceview5::Map,
    map_y_in_strip: f64,
    iter: &gtk4::TextIter,
) -> f64 {
    let (line_y, line_height) = source_map.line_yrange(iter);
    buffer_y_to_strip_y(
        source_map,
        map_y_in_strip,
        line_y.saturating_add(line_height.max(0)),
    )
}

fn buffer_y_to_strip_y(source_map: &sourceview5::Map, map_y_in_strip: f64, buffer_y: i32) -> f64 {
    let (_, widget_y) =
        source_map.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, buffer_y);
    map_y_in_strip + f64::from(widget_y)
}

fn fit_marker_bounds(
    kind: MinimapMarkerKind,
    raw_top: f64,
    raw_bottom: f64,
    space: MarkerProjectionSpace,
) -> Option<MinimapMarkerBounds> {
    if !raw_top.is_finite()
        || !raw_bottom.is_finite()
        || !space.strip_height.is_finite()
        || !space.content_top.is_finite()
        || !space.content_bottom.is_finite()
        || space.strip_height <= 0.0
    {
        return None;
    }

    let lower = space.content_top.max(0.0);
    let upper = space.content_bottom.min(space.strip_height);
    if upper <= lower {
        return None;
    }

    let (raw_top, raw_bottom) = if raw_top <= raw_bottom {
        (raw_top, raw_bottom)
    } else {
        (raw_bottom, raw_top)
    };
    if raw_bottom < lower || raw_top > upper {
        return None;
    }

    let mut top = raw_top.max(lower);
    let mut bottom = raw_bottom.min(upper);
    if bottom < top {
        bottom = top;
    }

    let target_height = space.min_height.max(0.0).min(upper - lower);
    if bottom - top < target_height {
        let center = ((top + bottom) / 2.0).clamp(lower, upper);
        top = center - (target_height / 2.0);
        bottom = center + (target_height / 2.0);

        if top < lower {
            bottom += lower - top;
            top = lower;
        }
        if bottom > upper {
            top -= bottom - upper;
            bottom = upper;
        }

        top = top.max(lower);
        bottom = bottom.min(upper);
    }

    (bottom > top).then_some(MinimapMarkerBounds { kind, top, bottom })
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

fn marker_lane_x(total_width: f64, lane_width: f64) -> f64 {
    total_width - lane_width
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
    fn test_minimap_policy_constants_are_stable() {
        assert_eq!(MINIMAP_MARKER_STRIP_WIDTH, 8);
        assert_eq!(MINIMAP_REFRESH_DEBOUNCE, Duration::from_millis(80));
        assert_eq!(MINIMAP_LONG_LINE_WARNING_THRESHOLD, 120);
        assert_eq!(MINIMAP_SEARCH_MATCH_CAP, 2_000);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET, 2_097_152);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET, 8_000);
        assert_eq!(MINIMAP_MARKER_MIN_HEIGHT, 2.0);
        assert_eq!(MINIMAP_MODIFIED_MARK_CATEGORY, "lushtext-minimap-modified");
    }

    #[test]
    fn test_marker_bounds_height_uses_bottom_minus_top() {
        let bounds = MinimapMarkerBounds {
            kind: MinimapMarkerKind::Search,
            top: 3.25,
            bottom: 11.75,
        };

        assert_eq!(bounds.height(), 8.5);
    }

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

    #[test]
    fn test_markers_from_lines_handles_empty_input() {
        assert!(markers_from_lines(MinimapMarkerKind::LongLine, []).is_empty());
    }

    #[test]
    fn test_buffer_line_budget_resets_on_newlines_and_uses_strict_overflow() {
        assert!(!text_exceeds_line_char_budget("abcd\nabcde\nxy", 5));
        assert!(text_exceeds_line_char_budget("abcd\nabcde\nxy", 4));

        assert!(!text_exceeds_line_char_budget("éé\né", 2));
        assert!(text_exceeds_line_char_budget("éé\né", 1));
    }

    #[test]
    fn test_buffer_line_budget_returns_false_for_empty_buffers() {
        assert!(!text_exceeds_line_char_budget("", 0));
    }

    #[test]
    fn test_minimap_availability_policy_preserves_priority_order() {
        let visible = MinimapAvailabilityPolicy {
            focus_suppressed: false,
            preference_enabled: true,
            evicted: false,
            syntax_enabled: true,
            wrapped_layout_too_large: false,
        };

        assert_eq!(
            minimap_availability_for_policy(visible),
            MinimapAvailability::Visible
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                preference_enabled: false,
                evicted: true,
                syntax_enabled: false,
                wrapped_layout_too_large: true,
                ..visible
            }),
            MinimapAvailability::Disabled,
            "the user preference should win over document state"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                focus_suppressed: true,
                preference_enabled: true,
                evicted: true,
                ..visible
            }),
            MinimapAvailability::Disabled,
            "Focus Mode should suppress the minimap without changing the saved preference"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                evicted: true,
                syntax_enabled: false,
                ..visible
            }),
            MinimapAvailability::Evicted,
            "evicted tabs report their reload state before size-tier feedback"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                syntax_enabled: false,
                ..visible
            }),
            MinimapAvailability::TooLarge
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                wrapped_layout_too_large: true,
                ..visible
            }),
            MinimapAvailability::TooLarge
        );
    }

    #[test]
    fn test_wrapped_layout_budget_requires_size_above_budget_and_extreme_line() {
        assert!(!wrapped_layout_budget_exceeded(
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET,
            true
        ));
        assert!(!wrapped_layout_budget_exceeded(
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET + 1,
            false
        ));
        assert!(wrapped_layout_budget_exceeded(
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET + 1,
            true
        ));
    }

    #[test]
    fn test_long_line_warning_lines_use_strict_character_threshold() {
        let exact = "a".repeat(MINIMAP_LONG_LINE_WARNING_THRESHOLD);
        let too_long = "b".repeat(MINIMAP_LONG_LINE_WARNING_THRESHOLD + 1);
        let another_too_long = "c".repeat(MINIMAP_LONG_LINE_WARNING_THRESHOLD + 2);
        let text = format!("{exact}\n{too_long}\nshort\n{another_too_long}");

        assert_eq!(long_line_warning_lines(&text), vec![1, 3]);
        assert!(long_line_warning_lines("short\nalso short").is_empty());
    }

    #[test]
    fn test_fit_marker_bounds_keeps_min_height_above_eof_tail() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Search,
            98.7,
            98.8,
            MarkerProjectionSpace {
                strip_height: 140.0,
                content_top: 10.0,
                content_bottom: 100.0,
                map_y_in_strip: 0.0,
                min_height: 8.0,
            },
        )
        .expect("bottom marker should still be drawable");

        assert_eq!(bounds.kind, MinimapMarkerKind::Search);
        assert!((bounds.top - 92.0).abs() < f64::EPSILON);
        assert!((bounds.bottom - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fit_marker_bounds_expands_collapsed_line_inside_content() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Bookmark,
            40.0,
            40.0,
            MarkerProjectionSpace {
                strip_height: 100.0,
                content_top: 0.0,
                content_bottom: 80.0,
                map_y_in_strip: 0.0,
                min_height: 6.0,
            },
        )
        .expect("collapsed but visible line should get a minimum marker");

        assert!((bounds.top - 37.0).abs() < f64::EPSILON);
        assert!((bounds.bottom - 43.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fit_marker_bounds_clamps_to_content_and_handles_reversed_input() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::LongLine,
            60.0,
            20.0,
            MarkerProjectionSpace {
                strip_height: 120.0,
                content_top: 25.0,
                content_bottom: 55.0,
                map_y_in_strip: 0.0,
                min_height: 2.0,
            },
        )
        .expect("partially visible reversed marker should be clamped");

        assert_eq!(bounds.kind, MinimapMarkerKind::LongLine);
        assert_eq!(bounds.top, 25.0);
        assert_eq!(bounds.bottom, 55.0);
        assert_eq!(bounds.height(), 30.0);
    }

    #[test]
    fn test_fit_marker_bounds_fills_content_when_minimum_exceeds_content_height() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Modified,
            42.0,
            42.0,
            MarkerProjectionSpace {
                strip_height: 100.0,
                content_top: 40.0,
                content_bottom: 45.0,
                map_y_in_strip: 0.0,
                min_height: 20.0,
            },
        )
        .expect("minimum height should be capped to visible content");

        assert_eq!(bounds.top, 40.0);
        assert_eq!(bounds.bottom, 45.0);
    }

    #[test]
    fn test_fit_marker_bounds_leaves_already_tall_markers_unchanged() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Search,
            10.0,
            16.0,
            MarkerProjectionSpace {
                strip_height: 100.0,
                content_top: 0.0,
                content_bottom: 90.0,
                map_y_in_strip: 0.0,
                min_height: 4.0,
            },
        )
        .expect("marker taller than the minimum should stay drawable");

        assert_eq!(bounds.top, 10.0);
        assert_eq!(bounds.bottom, 16.0);
    }

    #[test]
    fn test_fit_marker_bounds_keeps_markers_that_touch_content_edges() {
        let top_bounds = fit_marker_bounds(
            MinimapMarkerKind::Bookmark,
            25.0,
            25.0,
            MarkerProjectionSpace {
                strip_height: 120.0,
                content_top: 25.0,
                content_bottom: 55.0,
                map_y_in_strip: 0.0,
                min_height: 4.0,
            },
        )
        .expect("line at content top should still be visible");
        assert_eq!(top_bounds.top, 25.0);
        assert_eq!(top_bounds.bottom, 29.0);

        let bottom_bounds = fit_marker_bounds(
            MinimapMarkerKind::Bookmark,
            55.0,
            55.0,
            MarkerProjectionSpace {
                strip_height: 120.0,
                content_top: 25.0,
                content_bottom: 55.0,
                map_y_in_strip: 0.0,
                min_height: 4.0,
            },
        )
        .expect("line at content bottom should still be visible");
        assert_eq!(bottom_bounds.top, 51.0);
        assert_eq!(bottom_bounds.bottom, 55.0);
    }

    #[test]
    fn test_fit_marker_bounds_rejects_each_non_finite_input() {
        let finite_space = MarkerProjectionSpace {
            strip_height: 100.0,
            content_top: 10.0,
            content_bottom: 80.0,
            map_y_in_strip: 0.0,
            min_height: 2.0,
        };

        assert!(
            fit_marker_bounds(MinimapMarkerKind::Search, f64::NAN, 20.0, finite_space).is_none()
        );
        assert!(
            fit_marker_bounds(MinimapMarkerKind::Search, 20.0, f64::NAN, finite_space).is_none()
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                20.0,
                21.0,
                MarkerProjectionSpace {
                    strip_height: f64::NAN,
                    ..finite_space
                },
            )
            .is_none()
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                20.0,
                21.0,
                MarkerProjectionSpace {
                    content_top: f64::NAN,
                    ..finite_space
                },
            )
            .is_none()
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                20.0,
                21.0,
                MarkerProjectionSpace {
                    content_bottom: f64::NAN,
                    ..finite_space
                },
            )
            .is_none()
        );
    }

    #[test]
    fn test_fit_marker_bounds_rejects_zero_height_when_minimum_is_zero() {
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                30.0,
                30.0,
                MarkerProjectionSpace {
                    strip_height: 100.0,
                    content_top: 0.0,
                    content_bottom: 90.0,
                    map_y_in_strip: 0.0,
                    min_height: 0.0,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn test_fit_marker_bounds_rejects_unprojectable_geometry() {
        let space = MarkerProjectionSpace {
            strip_height: 100.0,
            content_top: 0.0,
            content_bottom: 80.0,
            map_y_in_strip: 0.0,
            min_height: 2.0,
        };

        assert!(
            fit_marker_bounds(MinimapMarkerKind::Modified, 90.0, 91.0, space).is_none(),
            "markers below rendered content must not be clamped into the EOF tail"
        );
        assert!(
            fit_marker_bounds(MinimapMarkerKind::Modified, f64::NAN, 10.0, space).is_none(),
            "non-finite geometry must not revive the old full-height fallback"
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Modified,
                10.0,
                11.0,
                MarkerProjectionSpace {
                    strip_height: 0.0,
                    content_top: 0.0,
                    content_bottom: 80.0,
                    map_y_in_strip: 0.0,
                    min_height: 2.0,
                },
            )
            .is_none(),
            "unallocated marker strips should not draw semantic markers"
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Modified,
                10.0,
                11.0,
                MarkerProjectionSpace {
                    strip_height: 100.0,
                    content_top: 20.0,
                    content_bottom: 20.0,
                    map_y_in_strip: 0.0,
                    min_height: 2.0,
                },
            )
            .is_none(),
            "empty rendered content should not synthesize marker bounds"
        );
    }

    #[test]
    fn test_marker_lane_widths_are_nested_with_two_pixel_floor() {
        assert_eq!(marker_lane_width(MinimapMarkerKind::Bookmark, 10.0), 10.0);
        assert!((marker_lane_width(MinimapMarkerKind::Search, 10.0) - 8.2).abs() < 1e-12);
        assert!((marker_lane_width(MinimapMarkerKind::Modified, 10.0) - 6.4).abs() < 1e-12);
        assert!((marker_lane_width(MinimapMarkerKind::LongLine, 10.0) - 4.6).abs() < 1e-12);
        assert_eq!(marker_lane_x(10.0, 4.6), 5.4);

        for kind in [
            MinimapMarkerKind::Bookmark,
            MinimapMarkerKind::Search,
            MinimapMarkerKind::Modified,
            MinimapMarkerKind::LongLine,
        ] {
            assert_eq!(marker_lane_width(kind, 1.0), 2.0);
        }
    }

    #[test]
    fn test_marker_rgba_palette_is_stable_for_light_and_dark_modes() {
        let expected = [
            (MinimapMarkerKind::Bookmark, false, (0.11, 0.44, 0.85, 0.95)),
            (MinimapMarkerKind::Bookmark, true, (0.39, 0.65, 0.95, 0.95)),
            (MinimapMarkerKind::Search, false, (0.95, 0.45, 0.0, 0.92)),
            (MinimapMarkerKind::Search, true, (1.0, 0.58, 0.17, 0.92)),
            (MinimapMarkerKind::Modified, false, (0.17, 0.68, 0.42, 0.92)),
            (MinimapMarkerKind::Modified, true, (0.32, 0.84, 0.55, 0.92)),
            (MinimapMarkerKind::LongLine, false, (0.88, 0.11, 0.14, 0.92)),
            (MinimapMarkerKind::LongLine, true, (1.0, 0.46, 0.45, 0.92)),
        ];

        for (kind, dark, rgba) in expected {
            assert_eq!(marker_rgba(kind, dark), rgba);
        }
    }
}
