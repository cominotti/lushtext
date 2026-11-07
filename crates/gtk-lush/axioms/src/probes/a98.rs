// SPDX-License-Identifier: MIT OR Apache-2.0

//! A98: a `GtkTextTag` with **no properties set** is bookkeeping only.
//!
//! - Applying it to one line costs about the same in a 262,144-line buffer
//!   as in a 1,024-line one (a B-tree walk, not a line scan).
//! - It queues no relayout of a presented view: the main loop has as little
//!   work after it is applied over the whole buffer as after nothing, where a
//!   tag that only sets `scale` to 1.0 revalidates every line.
//! - It creates no undo entry: after a tagged user edit, one undo restores
//!   the text, and on a buffer with no history it leaves `can-undo` false.
//! - It moves with its text when lines are inserted above it.
//! - Pasting tagged text into a buffer with **another tag table** carries no
//!   tag: the paste is plain text.
//! - On a `GtkSourceBuffer` that highlights syntax, adding a fresh tag to the
//!   table, applying it to the whole buffer, removing it and removing it from
//!   the table trigger no `highlight-updated`.
//! - Applying it from an `insert-text` or `delete-range` handler connected
//!   after the default one leaves the signal's iterators valid for the
//!   handlers that run after it: no GTK warning, and the location still reads
//!   the end of the insertion.
//!
//! Recorded beside it, as values only: whether text inserted inside, at the
//! start of, and at the end of a tagged range carries the tag, and whether a
//! paste inside the same buffer does.
//!
//! LushText's minimap tracks modified-since-save lines with one such tag
//! (`bin-minimap-markers-per-row` D4), applied from the buffer's edit
//! after-handlers, and retires it in slices before removing it from the table.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{LogCapture, TextViewFixture, record_text_toolkit, time_us};
use crate::session::{Presented, REALIZE_BUDGET, flush_events, settle, wait_until};
use crate::{AxiomId, Observation};

/// Lines of the large buffer the application cost is timed in.
pub const LARGE_LINES: usize = 1 << 18;
/// Lines of the small buffer it is compared with.
pub const SMALL_LINES: usize = 1_024;
/// Separate lines tagged in each buffer for the cost comparison.
pub const TAGGED_LINES: usize = 1_024;
/// Lines of the presented view the relayout comparison runs in.
pub const PRESENTED_LINES: usize = 20_000;
/// How long the main loop is drained after a whole-buffer application.
const DRAIN: Duration = Duration::from_millis(600);

/// `lines` lines of short text, each ending in a newline.
#[must_use]
pub fn numbered_lines(lines: usize) -> String {
    let mut text = String::with_capacity(lines * 12);
    for line in 0..lines {
        text.push_str("line ");
        text.push_str(&line.to_string());
        text.push('\n');
    }
    text
}

/// A property-less tag added to `buffer`'s table under `name`.
#[must_use]
pub fn bookkeeping_tag(buffer: &impl IsA<gtk4::TextBuffer>, name: &str) -> gtk4::TextTag {
    let tag = gtk4::TextTag::new(Some(name));
    buffer.as_ref().tag_table().add(&tag);
    tag
}

/// Apply `tag` to line `line`, from its start to the start of the next line
/// (the buffer end on the last line).
pub fn tag_line(buffer: &impl IsA<gtk4::TextBuffer>, tag: &gtk4::TextTag, line: i32) {
    let buffer = buffer.as_ref();
    let Some(start) = buffer.iter_at_line(line) else {
        return;
    };
    let end = buffer
        .iter_at_line(line.saturating_add(1))
        .filter(|end| end.line() > line)
        .unwrap_or_else(|| buffer.end_iter());
    buffer.apply_tag(tag, &start, &end);
}

/// Lines whose start carries `tag`, in order.
#[must_use]
pub fn tagged_lines(buffer: &impl IsA<gtk4::TextBuffer>, tag: &gtk4::TextTag) -> Vec<i32> {
    let buffer = buffer.as_ref();
    let mut lines = Vec::new();
    let mut iter = buffer.start_iter();
    loop {
        if iter.has_tag(tag) {
            lines.push(iter.line());
        }
        if !iter.forward_line() {
            break;
        }
    }
    lines
}

/// Microseconds to tag [`TAGGED_LINES`] lines spread over a `lines`-line
/// buffer, then to remove the tag from the whole buffer.
fn spread_application_us(lines: usize) -> (u128, u128) {
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(&numbered_lines(lines));
    let tag = bookkeeping_tag(&buffer, "a98-cost");
    let stride = i32::try_from(lines / TAGGED_LINES).unwrap_or(1).max(2);
    let ((), apply) = time_us(|| {
        for index in 0..TAGGED_LINES {
            tag_line(&buffer, &tag, i32::try_from(index).unwrap_or(0) * stride);
        }
    });
    let ((), remove) = time_us(|| {
        let (start, end) = buffer.bounds();
        buffer.remove_tag(&tag, &start, &end);
    });
    (apply, remove)
}

/// Busy microseconds of the main loop over [`DRAIN`], dispatching every
/// ready source in turn.
fn drain_busy_us() -> u128 {
    let deadline = Instant::now() + DRAIN;
    let mut busy = 0;
    while Instant::now() < deadline {
        let start = Instant::now();
        let mut dispatched = false;
        while gtk4::glib::MainContext::default().iteration(false) {
            dispatched = true;
        }
        if dispatched {
            busy += start.elapsed().as_micros();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    busy
}

/// Main-loop work after applying `tag` to the whole buffer of a presented
/// view, in busy microseconds.
fn presented_drain_after(fixture: &TextViewFixture, tag: &gtk4::TextTag) -> u128 {
    let (start, end) = fixture.buffer.bounds();
    fixture.buffer.apply_tag(tag, &start, &end);
    drain_busy_us()
}

/// Whether one undo after a tagged user edit restores the text, and whether
/// a buffer with no history can undo after a tag is applied.
fn undo_ignores_tag() -> (bool, bool, bool) {
    let buffer = sourceview5::Buffer::new(None);
    buffer.begin_irreversible_action();
    buffer.set_text("alpha\nbeta\ngamma\n");
    buffer.end_irreversible_action();
    let tag = bookkeeping_tag(&buffer, "a98-undo");
    tag_line(&buffer, &tag, 1);
    let no_history_after_tag = !buffer.can_undo();
    buffer.begin_user_action();
    let mut at = buffer.iter_at_line(1).unwrap_or_else(|| buffer.end_iter());
    buffer.insert(&mut at, "X");
    buffer.end_user_action();
    at = buffer.iter_at_line(1).unwrap_or_else(|| buffer.end_iter());
    let mut line_end = at;
    line_end.forward_to_line_end();
    buffer.apply_tag(&tag, &at, &line_end);
    buffer.undo();
    let (start, end) = buffer.bounds();
    let restored = buffer.text(&start, &end, true) == "alpha\nbeta\ngamma\n";
    (no_history_after_tag, restored, !buffer.can_undo())
}

/// Where the tag of line 5 sits after two lines are inserted above it, and
/// whether text inserted inside, at the start and at the end of the tagged
/// range carries the tag.
fn movement() -> (Vec<i32>, bool, bool, bool) {
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(&numbered_lines(10));
    let tag = bookkeeping_tag(&buffer, "a98-move");
    tag_line(&buffer, &tag, 5);
    buffer.insert(&mut buffer.start_iter(), "a\nb\n");
    let moved = tagged_lines(&buffer, &tag);
    let inserted_has_tag = |offset_in_line: i32, at_end: bool| {
        let mut at = buffer.iter_at_line(7).unwrap_or_else(|| buffer.end_iter());
        if at_end {
            at.forward_to_line_end();
            at.forward_char();
        } else {
            at.forward_chars(offset_in_line);
        }
        let mark = buffer.create_mark(None, &at, true);
        buffer.insert(&mut at, "Z");
        let inserted = buffer.iter_at_mark(&mark);
        buffer.delete_mark(&mark);
        let carries = inserted.has_tag(&tag);
        let mut after = inserted;
        after.forward_char();
        let mut inserted = inserted;
        buffer.delete(&mut inserted, &mut after);
        carries
    };
    let inside = inserted_has_tag(2, false);
    let at_start = inserted_has_tag(0, false);
    let at_end = inserted_has_tag(0, true);
    (moved, inside, at_start, at_end)
}

/// Whether a paste into a buffer with another tag table carries the tag (by
/// name or as any toggle), and whether one inside the same buffer does.
fn paste_carries_tag() -> Option<(bool, bool)> {
    let display = gtk4::gdk::Display::default()?;
    let clipboard = display.clipboard();
    let source = gtk4::TextBuffer::new(None);
    source.set_text("tagged text\n");
    let tag = bookkeeping_tag(&source, "a98-paste");
    let (start, end) = source.bounds();
    source.apply_tag(&tag, &start, &end);
    source.select_range(&start, &end);
    source.copy_clipboard(&clipboard);
    flush_events();
    let target = gtk4::TextBuffer::new(None);
    target.paste_clipboard(&clipboard, None, true);
    if !wait_until(REALIZE_BUDGET, || target.char_count() > 0) {
        return None;
    }
    let carries_by_name = target.tag_table().lookup("a98-paste").is_some();
    let carries_toggle = target
        .start_iter()
        .forward_to_tag_toggle(None::<&gtk4::TextTag>);
    let mut same_end = source.end_iter();
    let (copy_start, copy_end) = source.bounds();
    let copy = source.text(&copy_start, &copy_end, true);
    let _ = copy;
    let same_start_offset = same_end.offset();
    source.insert_range(&mut same_end, &copy_start, &copy_end);
    let same_carries = source.iter_at_offset(same_start_offset).has_tag(&tag);
    Some((carries_by_name || carries_toggle, same_carries))
}

/// `highlight-updated` emissions on a highlighting `GtkSourceBuffer` while a
/// fresh tag is added, applied, removed and dropped from the table, after the
/// buffer's own highlighting settled; `None` with no `html` language.
fn highlight_updates_from_tag() -> Option<(u32, u32)> {
    let language = sourceview5::LanguageManager::default().language("html")?;
    let buffer = sourceview5::Buffer::with_language(&language);
    buffer.set_highlight_syntax(true);
    buffer.set_highlight_matching_brackets(true);
    let view = sourceview5::View::with_buffer(&buffer);
    let scrolled = gtk4::ScrolledWindow::builder().child(&view).build();
    let shown = Presented::detached(&scrolled);
    let updates = Rc::new(Cell::new(0u32));
    {
        let updates = updates.clone();
        buffer.connect_highlight_updated(move |_, _, _| updates.set(updates.get() + 1));
    }
    let mut text = String::new();
    for index in 0..400 {
        text.push_str(&format!("<p class=\"c{index}\">text {index} (x)</p>\n"));
    }
    buffer.set_text(&text);
    settle(Duration::from_millis(400));
    let initial = updates.get();
    updates.set(0);
    let tag = bookkeeping_tag(&buffer, "a98-highlight");
    let (start, end) = buffer.bounds();
    buffer.apply_tag(&tag, &start, &end);
    settle(Duration::from_millis(100));
    let (start, end) = buffer.bounds();
    buffer.remove_tag(&tag, &start, &end);
    buffer.tag_table().remove(&tag);
    settle(Duration::from_millis(300));
    drop(shown);
    Some((initial, updates.get()))
}

/// Run edits through `insert-text` and `delete-range` handlers connected
/// after the default ones, the first applying a tag and the second reading
/// the signal's iterators; returns whether the second read what the edit
/// left, and the warnings GTK logged.
fn after_handler_iterators() -> (bool, bool, usize) {
    let capture = LogCapture::start();
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(&numbered_lines(20));
    let tag = bookkeeping_tag(&buffer, "a98-handler");
    {
        let tag = tag.clone();
        buffer.connect_local("insert-text", true, move |values| {
            let buffer = values.first()?.get::<gtk4::TextBuffer>().ok()?;
            let end = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let start = buffer.iter_at_line(end.line()).unwrap_or(end);
            let mut line_end = end;
            line_end.forward_line();
            buffer.apply_tag(&tag, &start, &line_end);
            None
        });
    }
    let insert_end = Rc::new(Cell::new(-1));
    {
        let insert_end = insert_end.clone();
        buffer.connect_local("insert-text", true, move |values| {
            let end = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            insert_end.set(end.offset());
            None
        });
    }
    {
        buffer.connect_local("delete-range", true, move |values| {
            let buffer = values.first()?.get::<gtk4::TextBuffer>().ok()?;
            let start = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let line_start = buffer.iter_at_line(start.line()).unwrap_or(start);
            let mut line_end = start;
            line_end.forward_line();
            buffer.apply_tag(&tag, &line_start, &line_end);
            None
        });
    }
    let delete_at = Rc::new(Cell::new((-1, -1)));
    {
        let delete_at = delete_at.clone();
        buffer.connect_local("delete-range", true, move |values| {
            let start = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let end = values.get(2)?.get::<gtk4::TextIter>().ok()?;
            delete_at.set((start.offset(), end.offset()));
            None
        });
    }
    let mut at = buffer.iter_at_line(3).unwrap_or_else(|| buffer.end_iter());
    let insert_offset = at.offset();
    buffer.insert(&mut at, "abc\ndef");
    let insert_ok = insert_end.get() == insert_offset + 7;
    let mut start = buffer.iter_at_line(10).unwrap_or_else(|| buffer.end_iter());
    let delete_offset = start.offset();
    let mut end = start;
    end.forward_chars(9);
    buffer.delete(&mut start, &mut end);
    let delete_ok = delete_at.get() == (delete_offset, delete_offset);
    let warnings = capture.messages().len();
    (insert_ok, delete_ok, warnings)
}

/// Probe A98.
#[must_use]
pub fn probe_a98() -> Observation {
    Recorder::run(AxiomId::new(98), |recorder| {
        record_text_toolkit(recorder);

        let (small_apply, small_remove) = spread_application_us(SMALL_LINES);
        let (large_apply, large_remove) = spread_application_us(LARGE_LINES);
        recorder.measure("apply_1024_lines_in_1024_line_buffer_us", small_apply);
        recorder.measure("apply_1024_lines_in_262144_line_buffer_us", large_apply);
        recorder.measure("remove_1024_runs_in_1024_line_buffer_us", small_remove);
        recorder.measure("remove_1024_runs_in_262144_line_buffer_us", large_remove);

        let fixture = TextViewFixture::new(gtk4::WrapMode::None);
        fixture.buffer.set_text(&numbered_lines(PRESENTED_LINES));
        let shown = Presented::checked(recorder, &fixture.scrolled, "the view is presented")?;
        // Let the view validate its lines before anything is compared.
        settle(Duration::from_millis(400));
        let _ = drain_busy_us();
        let idle = drain_busy_us();
        let plain = bookkeeping_tag(&fixture.buffer, "a98-plain");
        let plain_busy = presented_drain_after(&fixture, &plain);
        let scaled = gtk4::TextTag::builder()
            .name("a98-scale")
            .scale(1.0)
            .build();
        fixture.buffer.tag_table().add(&scaled);
        let scaled_busy = presented_drain_after(&fixture, &scaled);
        drop(shown);
        recorder.measure("idle_drain_us", idle);
        recorder.measure("plain_tag_drain_us", plain_busy);
        recorder.measure("scale_tag_drain_us", scaled_busy);

        let (no_history_after_tag, restored, history_empty_after_undo) = undo_ignores_tag();
        recorder.measure("can_undo_after_tag_on_fresh_history", !no_history_after_tag);
        recorder.measure("one_undo_restores_text", restored);
        recorder.measure("history_empty_after_one_undo", history_empty_after_undo);

        let (moved, inside, at_start, at_end) = movement();
        recorder.measure(
            "tagged_lines_after_two_inserted_above",
            format!("{moved:?}"),
        );
        recorder.measure("text_inserted_inside_carries_tag", inside);
        recorder.measure("text_inserted_at_range_start_carries_tag", at_start);
        recorder.measure("text_inserted_at_range_end_carries_tag", at_end);

        let paste = recorder.require(paste_carries_tag(), "the clipboard paste arrived")?;
        recorder.measure("cross_table_paste_carries_tag", paste.0);
        recorder.measure("same_buffer_insert_range_carries_tag", paste.1);

        let highlight = recorder.require(
            highlight_updates_from_tag(),
            "GtkSourceView has an html language",
        )?;
        recorder.measure("highlight_updates_while_loading", highlight.0);
        recorder.measure("highlight_updates_from_tag", highlight.1);

        let (insert_ok, delete_ok, warnings) = after_handler_iterators();
        recorder.measure("after_handler_insert_location_ok", insert_ok);
        recorder.measure("after_handler_delete_iters_ok", delete_ok);
        recorder.measure("after_handler_warnings", warnings);

        recorder.control(
            scaled_busy > 20_000,
            "control: a size-affecting tag over the whole view makes it revalidate",
        )?;
        recorder.control(highlight.0 > 0, "control: the buffer highlighted its text")?;
        recorder.axiom(
            large_apply < 100_000 && large_apply < small_apply.saturating_mul(16).max(4_000),
            "tagging a line costs about the same in a 262,144-line buffer as in a 1,024-line one",
        )?;
        recorder.axiom(
            plain_busy.saturating_mul(4) < scaled_busy && plain_busy < 8_000,
            "a property-less tag over the whole presented buffer queues no relayout",
        )?;
        recorder.axiom(
            no_history_after_tag && restored && history_empty_after_undo,
            "applying the tag creates no undo entry",
        )?;
        recorder.axiom(moved == vec![7], "the tag moves with its text")?;
        recorder.axiom(
            !paste.0,
            "a paste into a buffer with another tag table carries no tag",
        )?;
        recorder.axiom(
            highlight.1 == 0,
            "adding, applying, removing and dropping the tag trigger no re-highlight",
        )?;
        recorder.axiom(
            insert_ok && delete_ok && warnings == 0,
            "tagging from an edit after-handler leaves the signal's iterators valid",
        )
    })
}
