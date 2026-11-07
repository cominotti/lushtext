// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimap marker binning (`bin-minimap-markers-per-row`): every marker of
//! every kind on a line the strip shows is drawn, with no count cap; the
//! strip follows the map when it scrolls; modified-since-save tracking is a
//! property-less tag that leaves no trace in the document; and the marker
//! work fits the interaction budgets at 131,072 markers of each kind.
//!
//! `minimap_marker_count` and `minimap_marker_bounds` report the runs drawn
//! in the band the map shows, so an assertion about a marker far down the
//! document scrolls the map to it first.

use crate::common::{
    ensure_gtk_init, flush_events, test_application, timing_verdicts_enforced, wait_until,
    worst_dispatch_until,
};
use gtk4::prelude::*;
use lushtext_core::config::keys;
use lushtext_core::model::responsiveness_budget::{B_INTERACT_US, B_SETTLE_US};
use lushtext_core::ui::editor_page::{LushtextEditorPage, MinimapMarkerKind};
use std::time::{Duration, Instant};

/// 131,072 marked lines, the density the change measures.
const DENSE: i32 = 1 << 17;
/// 65,536 long lines, the most that fit under the minimap size tier.
const LONG: i32 = 1 << 16;

const KINDS: [MinimapMarkerKind; 4] = [
    MinimapMarkerKind::Bookmark,
    MinimapMarkerKind::Search,
    MinimapMarkerKind::Modified,
    MinimapMarkerKind::LongLine,
];

fn enable_minimap(long_line_markers: bool) {
    let settings = lushtext_core::ui::settings_writes::shared();
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, long_line_markers)
        .expect("long-line markers");
}

fn present(page: &LushtextEditorPage, height: i32) -> gtk4::ApplicationWindow {
    let window = gtk4::ApplicationWindow::builder()
        .application(&test_application())
        .default_width(1000)
        .default_height(height)
        .child(page)
        .build();
    window.present();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        page.source_view().is_mapped() && page.source_view().visible_rect().height() > 0
    });
    wait_until(Duration::from_secs(10), || {
        let (Some(map), Some(strip)) = (
            page.minimap_source_map_widget(),
            page.minimap_marker_strip_widget(),
        ) else {
            return false;
        };
        page.is_minimap_visible()
            && map.is_mapped()
            && strip.is_mapped()
            && strip.height() > 0
            && map.height() > 0
    });
    window
}

fn map_value(page: &LushtextEditorPage) -> f64 {
    page.minimap_source_map_widget()
        .and_then(|map| map.vadjustment())
        .map_or(0.0, |adjustment| adjustment.value())
}

/// Scroll the editor to `fraction` of its range and wait until the map
/// followed (the map scrolls with its view).
fn scroll_editor_to(page: &LushtextEditorPage, fraction: f64) {
    let adjustment = page.source_view().vadjustment().expect("editor adjustment");
    let before = map_value(page);
    let target = adjustment.lower() + (adjustment.upper() - adjustment.page_size()) * fraction;
    adjustment.set_value(target);
    let _ = worst_dispatch_until(Duration::from_secs(3), || {
        (map_value(page) - before).abs() > 0.5
    });
    flush_events();
}

/// Scroll the editor until `line` is drawn inside the marker strip (the map
/// scrolls with its view), through a mark so unvalidated lines settle first.
fn show_line_in_map(page: &LushtextEditorPage, line: i32) {
    let buffer = page.buffer();
    let iter = buffer.iter_at_line(line).expect("line");
    let mark = buffer.create_mark(None, &iter, true);
    let strip_height = f64::from(page.minimap_marker_strip_widget().expect("strip").height());
    let (shown, _) = worst_dispatch_until(Duration::from_secs(10), || {
        page.source_view()
            .scroll_to_mark(&mark, 0.0, true, 0.0, 0.5);
        let (top, bottom) = line_rows_in_strip(page, line);
        top >= 0.0 && bottom <= strip_height
    });
    buffer.delete_mark(&mark);
    assert!(shown, "line {line} is drawn in the strip");
    flush_events();
}

fn wait_for_scan(page: &LushtextEditorPage, needle: &str) {
    page.show_search();
    page.search_bar().search_entry().set_text(needle);
    wait_until(Duration::from_secs(60), || {
        page.search_bar()
            .search_context()
            .is_some_and(|context| context.occurrences_count() >= 0)
    });
}

/// `pairs` pairs of lines: a marked line (`needle`, a long tail when `long`)
/// then a plain one, so no two marked lines are adjacent.
fn alternating_document(pairs: i32, long: bool) -> String {
    let tail = "x".repeat(130);
    let mut text =
        String::with_capacity(usize::try_from(pairs).unwrap_or(0) * if long { 150 } else { 12 });
    for index in 0..pairs {
        text.push('n');
        text.push_str(&(index % 100).to_string());
        text.push_str(" needle");
        if long {
            text.push(' ');
            text.push_str(&tail);
        }
        text.push_str("\nz\n");
    }
    text
}

/// The strip rows `[top, bottom)` the map draws `line` on, as a fresh
/// projection through the map's own geometry.
fn line_rows_in_strip(page: &LushtextEditorPage, line: i32) -> (f64, f64) {
    let map = page.minimap_source_map_widget().expect("map");
    let strip = page.minimap_marker_strip_widget().expect("strip");
    let map_y = f64::from(map.compute_bounds(&strip).expect("map in strip").y());
    let iter = map.buffer().iter_at_line(line).expect("line");
    let (y, height) = map.line_yrange(&iter);
    let (_, top) = map.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, y);
    let (_, bottom) = map.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, y + height);
    (map_y + f64::from(top), map_y + f64::from(bottom))
}

/// Whether a drawn run covers a line drawn on `rows`. A line the map has not
/// laid out yet (or a hidden one) has zero height and belongs to the row group
/// above it (ledger A99), so it is matched against the row just above.
fn overlaps(
    bounds: &[lushtext_core::ui::editor_page::MinimapMarkerBounds],
    rows: (f64, f64),
) -> bool {
    let rows = if rows.1 > rows.0 {
        rows
    } else {
        (rows.0 - 1.0, rows.0)
    };
    bounds
        .iter()
        .any(|bound| bound.top < rows.1 && rows.0 < bound.bottom)
}

/// Rows of the strip's **rendered** pixels (its cached render node, through
/// `GtkWidgetPaintable`) that have paint in the strip's rightmost column.
fn rendered_marker_rows(strip: &gtk4::DrawingArea) -> Vec<i32> {
    let width = strip.width();
    let height = strip.height();
    let paintable = gtk4::WidgetPaintable::new(Some(strip));
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let Some(node) = snapshot.to_node() else {
        return Vec::new();
    };
    let renderer = strip
        .native()
        .expect("native")
        .renderer()
        .expect("renderer");
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let texture = renderer.render_texture(&node, Some(&bounds));
    let stride = usize::try_from(texture.width()).unwrap_or(0) * 4;
    let mut bytes = vec![0u8; stride * usize::try_from(texture.height()).unwrap_or(0)];
    texture.download(&mut bytes, stride);
    let column = usize::try_from(texture.width() - 1).unwrap_or(0);
    (0..texture.height())
        .filter(|&row| {
            let offset = usize::try_from(row).unwrap_or(0) * stride + column * 4;
            bytes.get(offset + 3).copied().unwrap_or(0) > 0
        })
        .collect()
}

fn projected_rows(page: &LushtextEditorPage, kind: MinimapMarkerKind) -> Vec<i32> {
    let mut rows: Vec<i32> = page
        .minimap_marker_bounds(kind)
        .iter()
        .flat_map(|bound| {
            #[expect(clippy::cast_possible_truncation, reason = "whole-pixel runs")]
            {
                (bound.top as i32)..(bound.bottom as i32)
            }
        })
        .collect();
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// 3.1: search matches past the former 2,048 cap are drawn at the end of the
/// document as they are at its top.
#[test]
fn test_minimap_search_markers_past_the_former_cap_are_drawn() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&alternating_document(DENSE, false));
    let window = present(&page, 800);
    wait_for_scan(&page, "needle");
    let last_needle_line = 2 * (DENSE - 1);
    show_line_in_map(&page, last_needle_line);
    wait_until(Duration::from_secs(5), || {
        page.minimap_marker_count(MinimapMarkerKind::Search) >= 10
    });
    let rows = line_rows_in_strip(&page, last_needle_line);
    assert!(
        overlaps(&page.minimap_marker_bounds(MinimapMarkerKind::Search), rows),
        "the last match, line {last_needle_line}, is drawn: {rows:?}"
    );
    window.destroy();
}

/// 3.2: an edit past the former 2,048-line cap is marked, and an edit whose
/// line number an earlier, shifted edit once had is marked too (`main`'s
/// line-number cache went stale when lines moved).
#[test]
fn test_minimap_modified_markers_past_the_former_cap_and_after_lines_shift() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    let text: String = (0..6_000).map(|line| format!("line {line}\n")).collect();
    page.buffer().set_text(&text);
    page.buffer().set_modified(false);
    let window = present(&page, 800);
    let buffer = page.buffer();
    for line in (0..4_098).step_by(2) {
        let mut iter = buffer.iter_at_line(line).expect("line");
        buffer.insert(&mut iter, "m");
    }
    let mut iter = buffer.iter_at_line(5_900).expect("line 5900");
    buffer.insert(&mut iter, "late edit ");
    show_line_in_map(&page, 5_900);
    wait_until(Duration::from_secs(5), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });
    assert!(
        overlaps(
            &page.minimap_marker_bounds(MinimapMarkerKind::Modified),
            line_rows_in_strip(&page, 5_900)
        ),
        "the 2,050th edited line is marked"
    );

    // The stale line-number cache: edit line 10, push it down by ten lines,
    // then edit the new line 10.
    buffer.set_modified(false);
    show_line_in_map(&page, 0);
    let mut iter = buffer.iter_at_line(10).expect("line 10");
    buffer.insert(&mut iter, "first ");
    let mut iter = buffer.iter_at_line(0).expect("line 0");
    buffer.insert(&mut iter, &"\n".repeat(10));
    // Lines 0-10 are now modified (the inserted run); untag them by saving
    // the state, then re-create only the shifted edit at line 20.
    buffer.set_modified(false);
    let mut iter = buffer.iter_at_line(20).expect("line 20");
    buffer.insert(&mut iter, "shifted ");
    let mut iter = buffer.iter_at_line(10).expect("line 10");
    buffer.insert(&mut iter, "second ");
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) >= 2
    });
    let bounds = page.minimap_marker_bounds(MinimapMarkerKind::Modified);
    assert!(
        overlaps(&bounds, line_rows_in_strip(&page, 10)),
        "line 10 is marked: {bounds:?}, line 10 at {:?}, line 20 at {:?}, map value {}",
        line_rows_in_strip(&page, 10),
        line_rows_in_strip(&page, 20),
        map_value(&page)
    );
    assert!(
        overlaps(&bounds, line_rows_in_strip(&page, 20)),
        "line 20 is marked"
    );
    window.destroy();
}

/// 3.3: long lines past the former cap are marked at the end of a document
/// with 65,536 of them (9.6 MB, inside the minimap's size tier).
#[test]
fn test_minimap_long_line_markers_past_the_former_cap_are_drawn() {
    ensure_gtk_init();
    enable_minimap(true);
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&alternating_document(LONG, true));
    let window = present(&page, 800);
    wait_until(Duration::from_secs(60), || {
        page.minimap_evidence().marker_cache_owned
    });
    assert_eq!(
        page.minimap_evidence().long_line_warning_count,
        usize::try_from(LONG).expect("count")
    );
    show_line_in_map(&page, 2 * (LONG - 1));
    wait_until(Duration::from_secs(5), || {
        page.minimap_marker_count(MinimapMarkerKind::LongLine) >= 10
    });
    assert!(overlaps(
        &page.minimap_marker_bounds(MinimapMarkerKind::LongLine),
        line_rows_in_strip(&page, 2 * (LONG - 1))
    ));
    window.destroy();
}

/// 3.4: the strip's rendered pixels follow the map when it scrolls, matching
/// a fresh projection of the visible lines (ledger A101: an overlay sibling
/// is not redrawn unless it asks).
#[test]
fn test_minimap_rendered_markers_follow_the_map_when_it_scrolls() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    let text: String = (0..3_000)
        .map(|line| {
            if line % 37 == 0 {
                format!("line {line} needle\n")
            } else {
                format!("line {line}\n")
            }
        })
        .collect();
    page.buffer().set_text(&text);
    page.buffer().set_modified(false);
    let window = present(&page, 800);
    wait_for_scan(&page, "needle");
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Search) > 0
    });
    let strip = page.minimap_marker_strip_widget().expect("strip");
    let mut previous_value = map_value(&page);
    for fraction in [0.3, 0.55, 0.8] {
        scroll_editor_to(&page, fraction);
        let value = map_value(&page);
        assert!(
            value > previous_value + 1.0,
            "the map scrolled: {previous_value} -> {value}"
        );
        previous_value = value;
        // Frames for the map's own scroll and any strip redraw it queued.
        let _ = worst_dispatch_until(Duration::from_millis(300), || false);
        let rendered = rendered_marker_rows(&strip);
        let projected = projected_rows(&page, MinimapMarkerKind::Search);
        assert!(
            !projected.is_empty(),
            "matches are in the band at {fraction}"
        );
        assert_eq!(
            rendered, projected,
            "rendered marker rows match a fresh projection after the map scrolled to {fraction}"
        );
    }
    window.destroy();
}

/// 3.5: a folded (zero-height) range holding a modified line is drawn on the
/// row group whose line range contains it.
#[test]
fn test_minimap_hidden_modified_line_marks_its_row_group() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    let text: String = (0..60).map(|line| format!("line {line}\n")).collect();
    page.buffer().set_text(&text);
    page.buffer().set_modified(false);
    let window = present(&page, 800);
    let buffer = page.buffer();
    let mut iter = buffer.iter_at_line(25).expect("line 25");
    buffer.insert(&mut iter, "edited ");
    let hidden = buffer
        .create_tag(Some("test-fold"), &[("invisible", &true)])
        .expect("fold tag");
    let (start, end) = (
        buffer.iter_at_line(20).expect("line 20"),
        buffer.iter_at_line(31).expect("line 31"),
    );
    buffer.apply_tag(&hidden, &start, &end);
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });
    let line_19 = line_rows_in_strip(&page, 19);
    let line_31 = line_rows_in_strip(&page, 31);
    let bounds = page.minimap_marker_bounds(MinimapMarkerKind::Modified);
    assert!(
        overlaps(&bounds, (line_19.0, line_31.0)),
        "the group showing lines 19-30 draws the modified lane: {bounds:?}, {line_19:?} {line_31:?}"
    );
    window.destroy();
}

fn modified_tag_names(page: &LushtextEditorPage) -> Vec<String> {
    let mut names = Vec::new();
    page.buffer().tag_table().foreach(|tag| {
        if let Some(name) = tag.name()
            && name.starts_with("lushtext-minimap-modified-")
        {
            names.push(name.to_string());
        }
    });
    names
}

/// 3.6: tracking leaves no trace: undo restores text only, saved bytes are the
/// buffer text, a paste into another tab carries no tag, a load is not
/// tracked, and a clean transition clears the markers in its own dispatch.
#[test]
fn test_minimap_modified_tracking_leaves_no_trace_in_the_document() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tracked.txt");
    lushtext_core::services::filesystem::fixture::write_text(&path, "alpha\nbeta\ngamma\n");
    let window = present(&page, 800);
    page.set_file_path(&path);
    page.load_file_async(&path);
    wait_until(Duration::from_secs(5), || {
        page.load_state() == lushtext_core::ui::editor_page::EditorLoadState::Loaded
    });
    flush_events();
    assert_eq!(
        page.minimap_marker_count(MinimapMarkerKind::Modified),
        0,
        "a load is not recorded as an edit"
    );

    let buffer = page.buffer();
    buffer.begin_user_action();
    let mut iter = buffer.iter_at_line(1).expect("line 1");
    buffer.insert(&mut iter, "X");
    buffer.end_user_action();
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });
    buffer.undo();
    let (start, end) = buffer.bounds();
    assert_eq!(buffer.text(&start, &end, true), "alpha\nbeta\ngamma\n");
    assert!(
        !buffer.can_undo(),
        "the undo history held only the text edit"
    );

    let mut iter = buffer.iter_at_line(2).expect("line 2");
    buffer.insert(&mut iter, "Y");
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |result| {
        result.expect("save");
        done_clone.set(true);
    });
    wait_until(Duration::from_secs(5), || done.get());
    let (start, end) = buffer.bounds();
    assert_eq!(
        lushtext_core::services::filesystem::fixture::read_text(&path),
        buffer.text(&start, &end, true).to_string(),
        "the saved bytes are exactly the buffer text"
    );
    assert_eq!(page.minimap_marker_count(MinimapMarkerKind::Modified), 0);

    // A paste into another tab is plain text: only that tab's own tag.
    let mut iter = buffer.iter_at_line(0).expect("line 0");
    buffer.insert(&mut iter, "tagged ");
    let other = LushtextEditorPage::new();
    let other_window = present(&other, 600);
    let clipboard = gtk4::prelude::WidgetExt::clipboard(&page);
    let (start, end) = buffer.bounds();
    buffer.select_range(&start, &end);
    buffer.copy_clipboard(&clipboard);
    other.buffer().paste_clipboard(&clipboard, None, true);
    wait_until(Duration::from_secs(5), || other.buffer().char_count() > 0);
    let other_tags = modified_tag_names(&other);
    assert!(
        other_tags.len() <= 1,
        "only the other tab's own tag: {other_tags:?}"
    );

    // The clean transition clears the markers in its own dispatch.
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });
    buffer.set_modified(false);
    assert_eq!(
        page.minimap_marker_count(MinimapMarkerKind::Modified),
        0,
        "no main-loop turn between the clean transition and the cleared markers"
    );
    other_window.destroy();
    window.destroy();
}

/// 3.8: with a search active, an edit sends the occurrence count to -1 until
/// its rescan ends (ledger A100); a map scroll inside that window drops the
/// search lane, which returns within the rescan's idle cycle.
#[test]
fn test_minimap_search_lane_returns_after_an_edit_and_a_scroll() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    let text: String = (0..2_000)
        .map(|line| {
            if line % 10 == 0 {
                format!("line {line} needle\n")
            } else {
                format!("line {line}\n")
            }
        })
        .collect();
    page.buffer().set_text(&text);
    let window = present(&page, 800);
    wait_for_scan(&page, "needle");
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Search) > 0
    });
    let buffer = page.buffer();
    let mut iter = buffer.iter_at_line(5).expect("line 5");
    buffer.insert(&mut iter, "typed");
    let adjustment = page.source_view().vadjustment().expect("adjustment");
    adjustment.set_value(adjustment.value() + 400.0);
    let start = Instant::now();
    let (returned, worst) = worst_dispatch_until(Duration::from_secs(2), || {
        page.minimap_marker_count(MinimapMarkerKind::Search) > 0
            && page
                .search_bar()
                .search_context()
                .is_some_and(|context| context.occurrences_count() >= 0)
    });
    println!(
        "bmm-evidence {{\"m\":\"search-after-edit\",\"returned_us\":{},\"worst_us\":{}}}",
        start.elapsed().as_micros(),
        worst.as_micros()
    );
    assert!(returned, "the search lane returns once the rescan ends");
    if timing_verdicts_enforced() {
        assert!(worst.as_micros() < u128::from(B_INTERACT_US), "{worst:?}");
    }
    window.destroy();
}

/// 3.9: retiring tags queue: two saves while the first still retires retire
/// both and remove them from the table, the current tag is untouched, edits
/// between slices do not disturb the cursor, and dispose cancels the slices.
#[test]
fn test_minimap_modified_tag_retirement_queues_and_cancels() {
    ensure_gtk_init();
    enable_minimap(false);
    let page = LushtextEditorPage::new();
    let text: String = (0..40_000).map(|line| format!("l{line}\n")).collect();
    page.buffer().set_text(&text);
    page.buffer().set_modified(false);
    let window = present(&page, 800);
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().retiring_modified_tags == 0
    });
    let buffer = page.buffer();
    for line in (0..40_000).step_by(2) {
        let mut iter = buffer.iter_at_line(line).expect("line");
        buffer.insert(&mut iter, "e");
    }
    let slices_before = page.minimap_evidence().retirement_slices;
    buffer.set_modified(false);
    let mut iter = buffer.iter_at_line(3).expect("line 3");
    buffer.insert(&mut iter, "again");
    buffer.set_modified(false);
    assert_eq!(page.minimap_evidence().retiring_modified_tags, 2);
    // One slice, then an edit above the cursor, then the rest.
    let _ = worst_dispatch_until(Duration::from_secs(2), || {
        page.minimap_evidence().retirement_slices > slices_before
    });
    let mut iter = buffer.start_iter();
    buffer.insert(&mut iter, "new first line\n");
    let current_line = 0;
    show_line_in_map(&page, current_line);
    wait_until(Duration::from_secs(10), || {
        page.minimap_evidence().retiring_modified_tags == 0
    });
    let evidence = page.minimap_evidence();
    assert!(
        evidence.retirement_slices >= slices_before + 3,
        "{evidence:?}"
    );
    assert!(!evidence.retirement_armed);
    assert_eq!(
        modified_tag_names(&page).len(),
        1,
        "both retired tags left the table; only the current one remains"
    );
    wait_until(Duration::from_secs(3), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });
    assert!(overlaps(
        &page.minimap_marker_bounds(MinimapMarkerKind::Modified),
        line_rows_in_strip(&page, current_line)
    ));

    // Dispose mid-retirement cancels the slices.
    for line in (0..40_000).step_by(2) {
        let mut iter = buffer.iter_at_line(line).expect("line");
        buffer.insert(&mut iter, "f");
    }
    buffer.set_modified(false);
    assert!(page.minimap_evidence().retirement_armed);
    window.destroy();
    flush_events();
    let evidence = page.minimap_evidence();
    assert!(
        !evidence.retirement_armed,
        "dispose removed the slice source"
    );
    assert_eq!(evidence.retiring_modified_tags, 0);
}

/// 3.7: the minimap's own marker work (bins, refresh, retirement slices) fits
/// `B_INTERACT_US`, and the frames that show it `B_SETTLE_US`, at 131,072
/// search matches and modified runs, in strips of 713 and 1,425 rows. The
/// optimized widget build enforces them at 131,072; the debug suite runs the
/// same flow at 2,048 markers and prints the figures, because its 131,072
/// separate edits alone take minutes unoptimized.
#[test]
fn test_minimap_marker_work_fits_the_budgets_at_131072_markers() {
    ensure_gtk_init();
    enable_minimap(false);
    let pairs = if timing_verdicts_enforced() {
        DENSE
    } else {
        1 << 11
    };
    for height in [800, 1_600] {
        let page = LushtextEditorPage::new();
        page.buffer().set_text(&alternating_document(pairs, false));
        page.buffer().set_modified(false);
        let window = present(&page, height);
        wait_for_scan(&page, "needle");
        let buffer = page.buffer();
        for line in (0..2 * pairs).step_by(2) {
            let mut iter = buffer.iter_at_line(line).expect("line");
            buffer.insert(&mut iter, "m");
        }
        let (_, refresh_worst) = worst_dispatch_until(Duration::from_secs(3), || {
            !page.minimap_evidence().work_pending
        });
        let mut bins_worst = Duration::ZERO;
        let mut frame_worst = Duration::ZERO;
        for step in 0..=20 {
            scroll_editor_to(&page, f64::from(step) / 20.0);
            let start = Instant::now();
            for kind in KINDS {
                let _ = page.minimap_marker_bounds(kind);
            }
            bins_worst = bins_worst.max(start.elapsed());
            let (_, frame) = worst_dispatch_until(Duration::from_millis(120), || false);
            frame_worst = frame_worst.max(frame);
        }
        let evidence = page.minimap_evidence();
        assert!(
            evidence.search_queries <= evidence.row_groups + 1,
            "{evidence:?}"
        );
        assert!(
            evidence.modified_queries <= evidence.row_groups + 1,
            "{evidence:?}"
        );
        assert!(page.minimap_marker_count(MinimapMarkerKind::Search) > 0);
        assert!(page.minimap_marker_count(MinimapMarkerKind::Modified) > 0);
        let save_start = Instant::now();
        buffer.set_modified(false);
        let save_us = save_start.elapsed();
        assert_eq!(page.minimap_marker_count(MinimapMarkerKind::Modified), 0);
        let (_, retire_worst) = worst_dispatch_until(Duration::from_secs(10), || {
            page.minimap_evidence().retiring_modified_tags == 0
        });
        println!(
            "bmm-evidence {{\"m\":\"budget\",\"pairs\":{pairs},\"rows\":{},\"groups\":{},\"refresh_worst_us\":{},\"bins_worst_us\":{},\"scroll_frame_worst_us\":{},\"save_us\":{},\"retire_worst_us\":{},\"retire_slices\":{}}}",
            evidence.binned_rows,
            evidence.row_groups,
            refresh_worst.as_micros(),
            bins_worst.as_micros(),
            frame_worst.as_micros(),
            save_us.as_micros(),
            retire_worst.as_micros(),
            page.minimap_evidence().retirement_slices,
        );
        if timing_verdicts_enforced() {
            let interact = u128::from(B_INTERACT_US);
            assert!(bins_worst.as_micros() < interact, "bins {bins_worst:?}");
            assert!(save_us.as_micros() < interact, "save {save_us:?}");
            assert!(
                retire_worst.as_micros() < interact,
                "retirement {retire_worst:?}"
            );
            assert!(
                frame_worst.as_micros() < u128::from(B_SETTLE_US),
                "frame {frame_worst:?}"
            );
        }
        window.destroy();
        flush_events();
    }
}
