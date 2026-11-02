// SPDX-License-Identifier: MIT OR Apache-2.0

//! A77: appending **invisible** text to a paragraph in slices separated by
//! left-gravity marks (A76) makes the view lay the paragraph out again after
//! every slice, at a cost bounded by the paragraph's **visible** text and its
//! segment count, not by its hidden bytes: the per-append re-layout stays
//! flat as the hidden tail grows from one slice to `SLICES` slices.
//!
//! Two orders are measured, as `sliced-protected-install` (C3) chose between
//! them (design D2):
//!
//! - **head first**: the visible head is in the paragraph while the tail
//!   grows, so every slice re-lays out the head (Pango shapes it again);
//! - **tail first**: the tail is appended to an empty paragraph and the head
//!   is inserted afterwards, before the tail, at a left-gravity mark at the
//!   paragraph start (A76), so every slice re-lays out a paragraph with no
//!   visible text, and the head is laid out once.
//!
//! The re-layout is the view's line-display construction, timed through the
//! first location query after the append (as A65 times its first
//! validation); no cursor or word motion runs, which would read the hidden
//! text's log attributes (A26).
//!
//! A third order puts only the head's **first character** before the tail,
//! and the probe also times mapping x = 0 of the line to an iterator and a
//! GtkSourceView snapshot after each append. Both stay flat in every order: the
//! per-frame walk the C3 lane first met in the application was A79's (a point
//! below the layout on a last line with a hidden run), not the head's
//! placement.

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::a76::{ends, is_flat};
use crate::probes::text::{ParagraphKind, TextViewFixture, invisible_tag, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters of the visible head: `HEAD_CAP`, the most a protected
/// paragraph shows.
pub const HEAD_CHARS: usize = 10_000;
/// Bytes of one hidden slice.
pub const SLICE_BYTES: usize = 256 * 1024;
/// Hidden slices appended: a 16 MiB tail.
pub const SLICES: usize = 64;

/// The visible head.
#[must_use]
pub fn head() -> String {
    paragraph(ParagraphKind::Ascii, HEAD_CHARS, 77)
}

/// One hidden slice.
#[must_use]
pub fn slice_text() -> String {
    paragraph(ParagraphKind::Ascii, SLICE_BYTES, 78)
}

/// The cost of the view's next layout of the first paragraph.
#[must_use]
pub fn relayout_us(fixture: &TextViewFixture) -> u128 {
    let start = fixture.buffer.start_iter();
    let started = Instant::now();
    let _ = fixture.view.iter_location(&start);
    started.elapsed().as_micros()
}

/// The cost of mapping x = 0 of the first line to an iterator, as
/// GtkSourceView's snapshot does for every drawn frame
/// (`gtk_text_view_get_iter_at_location`).
#[must_use]
pub fn line_start_lookup_us(fixture: &TextViewFixture) -> u128 {
    let started = Instant::now();
    let _ = fixture.view.iter_at_location(0, 0);
    started.elapsed().as_micros()
}

/// What one sliced install measured after each hidden append.
#[derive(Clone, Debug, Default)]
pub struct SlicedInstall {
    /// The paragraph's re-layout after each append.
    pub relayout_us: Vec<u128>,
    /// The x = 0 lookup after each append.
    pub lookup_us: Vec<u128>,
    /// The re-layout after the rest of the head went in.
    pub head_us: u128,
}

/// Replace the buffer with one paragraph followed by `"\nafter\n"`, append
/// `SLICES` hidden slices to the paragraph (each followed by a left-gravity
/// mark) after the first `prefix_chars` characters of `head`, then insert
/// the rest of the head at a left-gravity mark after that prefix. A prefix of
/// the whole head is head first; a prefix of 0 is tail first.
#[must_use]
pub fn sliced_install(
    fixture: &TextViewFixture,
    tag: &gtk4::TextTag,
    head: &str,
    slice: &str,
    prefix_chars: usize,
) -> SlicedInstall {
    let buffer = &fixture.buffer;
    let split = head
        .char_indices()
        .nth(prefix_chars)
        .map_or(head.len(), |(index, _)| index);
    buffer.set_text(&head[..split]);
    let mut after = buffer.end_iter();
    buffer.insert(&mut after, "\nafter\n");
    let tail_at = i32::try_from(head[..split].chars().count()).unwrap_or(i32::MAX);
    let rest = buffer.create_mark(None, &buffer.iter_at_offset(tail_at), true);
    let end = buffer.create_mark(None, &buffer.iter_at_offset(tail_at), false);
    settle(LAYOUT_SETTLE);
    let mut measured = SlicedInstall::default();
    let mut marks = Vec::with_capacity(SLICES);
    for _ in 0..SLICES {
        let mut at = buffer.iter_at_mark(&end);
        buffer.insert_with_tags(&mut at, slice, &[tag]);
        marks.push(buffer.create_mark(None, &at, true));
        measured.relayout_us.push(relayout_us(fixture));
        measured.lookup_us.push(line_start_lookup_us(fixture));
    }
    if split < head.len() {
        let mut at = buffer.iter_at_mark(&rest);
        buffer.insert(&mut at, &head[split..]);
        measured.head_us = relayout_us(fixture);
    }
    for mark in marks.iter().chain([&rest, &end]) {
        buffer.delete_mark(mark);
    }
    measured
}

/// The first location query after one hidden insert of `mib` MiB after the
/// head, in microseconds.
#[must_use]
pub fn one_turn_relayout_us(
    fixture: &TextViewFixture,
    tag: &gtk4::TextTag,
    head: &str,
    slice: &str,
    slices: usize,
) -> u128 {
    let buffer = &fixture.buffer;
    buffer.set_text(head);
    let mut after = buffer.end_iter();
    buffer.insert(&mut after, "\nafter\n");
    settle(LAYOUT_SETTLE);
    let mut at = buffer.iter_at_offset(i32::try_from(head.chars().count()).unwrap_or(i32::MAX));
    buffer.insert_with_tags(&mut at, &slice.repeat(slices), &[tag]);
    relayout_us(fixture)
}

/// Probe A77.
#[must_use]
pub fn probe_a77() -> Observation {
    Recorder::run(AxiomId::new(77), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let tag = invisible_tag(&fixture.buffer);
        let head = head();
        let slice = slice_text();
        let mut flat = true;
        let mut complete = true;
        let mut lookups_flat = true;
        for wrap in [gtk4::WrapMode::WordChar, gtk4::WrapMode::None] {
            fixture.view.set_wrap_mode(wrap);
            let key_suffix = if wrap == gtk4::WrapMode::None {
                "wrap_off"
            } else {
                "wrap_on"
            };
            let mut rows = Vec::new();
            for (order, prefix) in [
                ("head_first", HEAD_CHARS),
                ("first_char", 1),
                ("tail_first", 0),
            ] {
                let measured = sliced_install(&fixture, &tag, &head, &slice, prefix);
                let expected = HEAD_CHARS + SLICES * SLICE_BYTES + "\nafter\n".len();
                complete &= usize::try_from(fixture.buffer.char_count()).unwrap_or(0) == expected
                    && fixture
                        .buffer
                        .iter_at_offset(i32::try_from(HEAD_CHARS).unwrap_or(i32::MAX))
                        .has_tag(&tag)
                    && !fixture.buffer.start_iter().has_tag(&tag)
                    && !fixture
                        .buffer
                        .iter_at_offset(i32::try_from(HEAD_CHARS - 1).unwrap_or(i32::MAX))
                        .has_tag(&tag);
                flat &= is_flat(&measured.relayout_us);
                if prefix > 0 {
                    lookups_flat &= is_flat(&measured.lookup_us);
                }
                rows.push(format!(
                        "{order} {key_suffix}: re-layout per append first/last medians {:?} (sum {} us), \
                         x=0 lookup first/last medians {:?}, rest of head {} us",
                        ends(&measured.relayout_us),
                        measured.relayout_us.iter().sum::<u128>(),
                        ends(&measured.lookup_us),
                        measured.head_us
                ));
                fixture.buffer.set_text("");
                settle(LAYOUT_SETTLE);
            }
            recorder.measure(
                if wrap == gtk4::WrapMode::None {
                    "sliced_install_wrap_off"
                } else {
                    "sliced_install_wrap_on"
                },
                rows.join(" | "),
            );
            let one_turn: Vec<String> = [4usize, 16, 64]
                .iter()
                .map(|slices| {
                    let us = one_turn_relayout_us(&fixture, &tag, &head, &slice, *slices);
                    fixture.buffer.set_text("");
                    settle(LAYOUT_SETTLE);
                    format!("{} MiB: {us} us", slices * SLICE_BYTES / (1024 * 1024))
                })
                .collect();
            recorder.measure(
                if wrap == gtk4::WrapMode::None {
                    "one_turn_relayout_wrap_off"
                } else {
                    "one_turn_relayout_wrap_on"
                },
                one_turn.join(" | "),
            );
        }
        recorder.control(
            complete,
            "control: every install holds the head untagged and the whole tail tagged",
        )?;
        recorder.axiom(
            flat,
            "the re-layout after each hidden append stays flat as the hidden tail grows, in \
             every order and both wrap modes",
        )?;
        recorder.axiom(
            lookups_flat,
            "with a visible character before the hidden run, mapping x = 0 of the line to an \
             iterator costs a flat time as the hidden run grows",
        )?;
        // GtkSourceView's own snapshot (line numbers on), painted after each
        // append: with the head last it walked the hidden run in the app
        // (perf, C3); measured here in the two orders.
        let source = source_view_fixture();
        let _shown_source = Presented::checked(
            recorder,
            &source.1,
            "control: the source view was presented",
        )?;
        let source_tag = invisible_tag(source.0.upcast_ref());
        let mut painted_flat = true;
        let mut rows = Vec::new();
        for (order, prefix) in [("first_char", 1), ("tail_first", 0)] {
            let paints = source_view_paints(&source, &source_tag, &head, &slice, prefix);
            if prefix > 0 {
                painted_flat &= is_flat(&paints);
            }
            rows.push(format!(
                "{order}: GtkSourceView snapshot per append first/last medians {:?}",
                ends(&paints)
            ));
        }
        recorder.measure("source_view_snapshot", rows.join(" | "));
        recorder.axiom(
            painted_flat,
            "with a visible character before the hidden run, a GtkSourceView snapshot costs a \
             flat time as the hidden run grows",
        )
    })
}

/// A plain-text `GtkSourceView` with line numbers, in a scrolled window.
fn source_view_fixture() -> (sourceview5::Buffer, gtk4::ScrolledWindow, sourceview5::View) {
    use sourceview5::prelude::ViewExt as _;
    let buffer = sourceview5::Buffer::new(None);
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_wrap_mode(gtk4::WrapMode::WordChar);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .build();
    (buffer, scrolled, view)
}

/// Install like [`sliced_install`] into the source view and time a snapshot
/// of it after each append.
fn source_view_paints(
    source: &(sourceview5::Buffer, gtk4::ScrolledWindow, sourceview5::View),
    tag: &gtk4::TextTag,
    head: &str,
    slice: &str,
    prefix_chars: usize,
) -> Vec<u128> {
    use gtk4::gdk::prelude::PaintableExt as _;
    let (buffer, _, view) = source;
    let split = head
        .char_indices()
        .nth(prefix_chars)
        .map_or(head.len(), |(index, _)| index);
    buffer.set_text(&head[..split]);
    let mut after = buffer.end_iter();
    buffer.insert(&mut after, "\n");
    let tail_at = i32::try_from(head[..split].chars().count()).unwrap_or(i32::MAX);
    let end = buffer.create_mark(None, &buffer.iter_at_offset(tail_at), false);
    settle(LAYOUT_SETTLE);
    let paintable = gtk4::WidgetPaintable::new(Some(view));
    let (width, height) = (
        f64::from(view.width().max(1)),
        f64::from(view.height().max(1)),
    );
    let mut paints = Vec::with_capacity(SLICES);
    let mut marks = Vec::with_capacity(SLICES);
    for _ in 0..SLICES {
        let mut at = buffer.iter_at_mark(&end);
        buffer.insert_with_tags(&mut at, slice, &[tag]);
        marks.push(buffer.create_mark(None, &at, true));
        settle(std::time::Duration::from_millis(20));
        let snapshot = gtk4::Snapshot::new();
        let started = Instant::now();
        paintable.snapshot(&snapshot, width, height);
        let _node = snapshot.to_node();
        paints.push(started.elapsed().as_micros());
    }
    for mark in marks.iter().chain([&end]) {
        buffer.delete_mark(mark);
    }
    buffer.set_text("");
    settle(LAYOUT_SETTLE);
    paints
}
