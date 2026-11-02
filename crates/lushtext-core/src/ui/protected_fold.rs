// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting coordination (no role): the Protected View fold executor
//! (`long-line-protected-view` design D1, task 3.1).
//!
//! Every whole-content install of a protected body inserts its text through
//! `insert_folded`, which hides each folded paragraph's tail **in the same
//! dispatch as its head**: the head with `insert`, the tail with
//! `insert_with_tags` and the fold tag (`invisible`, not `editable`). No
//! unfolded giant paragraph therefore exists in a `GtkTextView` at any
//! dispatch boundary, so GTK never lays one out (ledger A23, A65). The plan
//! comes from the worker that profiled the body
//! (`model::protected_fold::FoldPlan`, carried by `DocumentProfile::fold_plan`).
//!
//! Callers: the load install (`ui/editor_page/load/execution.rs`) and the
//! bounded buffer replacement (`ui/editor_page/buffer_replacement/execution.rs`,
//! which serves draft recovery, set-aside open, local-history restore and
//! Undo Restore, and eviction reload).
//!
//! **Sliced installs (`sliced-protected-install`, C3).** A body installed in
//! turns goes through `FoldedInstall`, which follows the GTK-free plan of
//! `model::protected_install`: a hidden tail goes in in slices of at most
//! `PROTECTED_TAIL_SLICE_BYTES`, one per turn, each followed by a
//! left-gravity **boundary mark** so the btree never merges two slices by
//! copying them (ledger A36, A76). The paragraph's visible head goes in
//! **after** its tail, at a left-gravity mark at the paragraph start (A76),
//! so no tail slice re-lays out the head (A77), and its delimiter goes in
//! **before** the tail, so the paragraph is never the last line while the
//! tail grows: GTK maps a point below the layout to the last line's end and
//! then walks the line once per hidden byte (`gtk_text_iter_set_visible_line_index`
//! at the end iterator, ledger A79), on every drawn frame. A body whose last
//! paragraph has no delimiter gets a stand-in `\n` under an invisible,
//! non-editable terminator tag, put in before its first tail slice and kept
//! after the install (on `main` the installed paragraph was the last line and
//! that walk cost 3.5 s per frame for a 32 MiB recovered draft); it is not
//! document text (`document_end_iter`). The marks are recorded in the
//! editor's `BoundaryLedger`, which outlives the install; a later clear
//! deletes the recorded ranges paragraph by paragraph from each paragraph's
//! start, one bounded range set per turn (`BoundaryLedger::clear_turn`), so
//! no clear deletes a giant tail in one turn either. A tail that fits one
//! slice goes in right after its head, in the same turn, unmarked (C2's
//! shape), and several such paragraphs share a turn within its budget.
//! `insert_folded` remains the one-turn executor of direct installs, whose
//! bodies are small by policy.
//!
//! **One hidden-tail insert per main-loop iteration, process-wide.** GLib
//! dispatches every ready source of the same priority in one iteration, so two
//! documents' install turns (a session restore, two recovered drafts) would
//! otherwise run their tail slices back to back in one dispatch. Every
//! protected install therefore asks `tail_insert_admitted` before a turn and
//! reschedules itself when another document already inserted a tail in this
//! iteration; a high-priority idle re-admits at the start of the next one.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use gtk4::prelude::*;

use crate::model::buffer_replacement::{PacedSlice, ParagraphLength, SliceBudget};
use crate::model::paragraph_boundary::next_paragraph_break;
use crate::model::protected_fold::{FoldPlan, fold_byte_offset};
use crate::model::protected_install::{
    PRODUCTION_LIMITS, PROTECTED_TAIL_SCAN_BYTES, ProtectedInstallCursor, SliceKind,
    clear_ranges_for_turn, paragraph_content_end, plan_protected_install,
};

thread_local! {
    static TAIL_INSERTED_THIS_ITERATION: Cell<bool> = const { Cell::new(false) };
}

/// Whether a protected install may run a turn that can insert a hidden tail
/// now: no other tail went in during this main-loop iteration (module docs).
pub(crate) fn tail_insert_admitted() -> bool {
    !TAIL_INSERTED_THIS_ITERATION.with(Cell::get)
}

/// Record a hidden-tail insert; the next iteration starts with a
/// `G_PRIORITY_HIGH` idle that re-admits, before any default-priority turn.
fn note_tail_inserted() {
    if !TAIL_INSERTED_THIS_ITERATION.with(|inserted| inserted.replace(true)) {
        glib::idle_add_local_full(glib::Priority::HIGH, || {
            TAIL_INSERTED_THIS_ITERATION.with(|inserted| inserted.set(false));
            glib::ControlFlow::Break
        });
    }
}

/// The name of the fold tag in a buffer's tag table.
pub const FOLD_TAG_NAME: &str = "lushtext-protected-fold";

/// Insert a whole protected body at `iter` in one turn (`insert_folded`
/// from its first paragraph), followed by the stand-in terminator when its
/// last paragraph is folded and has no delimiter (A79). The direct installs
/// of load and buffer replacement call it.
pub(crate) fn insert_folded_document(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    text: &str,
    plan: &FoldPlan,
) {
    let last = insert_folded(buffer, iter, text, 0, plan);
    let undelimited = !text.ends_with(['\n', '\r', '\u{2029}']);
    if undelimited
        && plan
            .ranges()
            .last()
            .is_some_and(|range| range.paragraph == last)
    {
        append_terminator(buffer, iter);
    }
}

/// The end of the document text in `buffer`: its end iterator, or the
/// position before a protected install's stand-in terminator when the buffer
/// ends with one. Every reader of the whole document text from a buffer
/// (`ui::buffer_snapshot`) stops here, so the terminator never reaches a
/// save, a draft, a history snapshot or a preview.
pub(crate) fn document_end_iter(buffer: &gtk4::TextBuffer) -> gtk4::TextIter {
    let end = buffer.end_iter();
    let mut last = end;
    if last.backward_char()
        && let Some(tag) = buffer.tag_table().lookup(TERMINATOR_TAG_NAME)
        && last.has_tag(&tag)
    {
        return last;
    }
    end
}

/// Append the stand-in terminator after a body whose last paragraph is
/// folded and has no delimiter, at `at` (the install's end).
fn append_terminator(buffer: &gtk4::TextBuffer, at: &mut gtk4::TextIter) {
    let tag = terminator_tag(buffer);
    buffer.insert_with_tags(at, "\n", &[&tag]);
}

/// The name of the stand-in terminator tag (`FoldedInstall`).
pub const TERMINATOR_TAG_NAME: &str = "lushtext-protected-install-terminator";

/// The tag of the stand-in `\n` a protected install puts after a body's
/// folded, undelimited last paragraph: invisible and not editable, created on
/// first use. GTK maps a point below the layout to the last line's end and
/// then walks that line once per hidden byte (A79), on every frame
/// GtkSourceView draws while the visible area extends below the text; with
/// the stand-in the folded paragraph is never the last line. It is not
/// document text: `document_end_iter` stops before it, and every clear
/// deletes it with the rest of the buffer.
fn terminator_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(TERMINATOR_TAG_NAME) {
        return tag;
    }
    let tag = gtk4::TextTag::builder()
        .name(TERMINATOR_TAG_NAME)
        .invisible(true)
        .editable(false)
        .build();
    table.add(&tag);
    tag
}

/// The buffer's fold tag, created on first use: `invisible` (the tail is
/// never laid out, A24/A65) and not `editable`.
pub(crate) fn fold_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(FOLD_TAG_NAME) {
        return tag;
    }
    let tag = gtk4::TextTag::builder()
        .name(FOLD_TAG_NAME)
        .invisible(true)
        .editable(false)
        .build();
    table.add(&tag);
    tag
}

/// Insert `text` at `iter` with the plan's tails hidden.
///
/// `text` must start at the start of paragraph `first_paragraph` of the body
/// the plan was made for (every install slice does: slices end after a
/// paragraph delimiter). Runs of unfolded paragraphs go in one `insert`; a
/// folded paragraph goes in as its head, then its tail under the fold tag,
/// then its delimiter. Returns the index of the paragraph after the last one
/// `text` completes (a paragraph `text` leaves open at its end keeps its
/// index, so the next slice continues it: a slice never ends mid-paragraph,
/// so this happens only for the body's last paragraph).
pub(crate) fn insert_folded(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    text: &str,
    first_paragraph: u64,
    plan: &FoldPlan,
) -> u64 {
    let tag = fold_tag(buffer);
    let ranges = plan.ranges();
    // The first planned range at or after this slice.
    let mut next_range = ranges.partition_point(|range| range.paragraph < first_paragraph);
    let mut paragraph = first_paragraph;
    let mut pending_start = 0; // start of the not-yet-inserted unfolded run
    let mut cursor = 0;
    loop {
        let folded_here = ranges
            .get(next_range)
            .filter(|range| range.paragraph == paragraph)
            .copied();
        let found = next_paragraph_break(text, cursor);
        let content_end = found.map_or(text.len(), |found| found.content_end);
        if let Some(range) = folded_here {
            // Flush the unfolded run before this paragraph, then head, tail.
            if pending_start < cursor {
                buffer.insert(iter, &text[pending_start..cursor]);
            }
            let content = &text[cursor..content_end];
            let split = cursor + fold_byte_offset(content, range.head_chars);
            if cursor < split {
                buffer.insert(iter, &text[cursor..split]);
            }
            if split < content_end {
                // The hidden tail goes in in one call (the declared envelope
                // C3's gate measures); the measurement build records it.
                let started_us = glib::monotonic_time();
                buffer.insert_with_tags(iter, &text[split..content_end], &[&tag]);
                crate::ui::responsiveness_probe::record_span(
                    "protected_tail_insert",
                    started_us,
                    glib::monotonic_time(),
                );
                note_tail_inserted();
            }
            pending_start = content_end;
            next_range += 1;
        }
        match found {
            Some(found) => {
                cursor = found.next_start;
                paragraph += 1;
                if cursor >= text.len() {
                    break;
                }
            }
            None => break,
        }
    }
    if pending_start < text.len() {
        buffer.insert(iter, &text[pending_start..]);
    }
    paragraph
}

// ---------------------------------------------------------------------------
// The sliced install (C3)
// ---------------------------------------------------------------------------

/// One protected paragraph's boundary marks: the paragraph start, then one
/// mark after each hidden-tail slice, with the bytes of each range between
/// two consecutive marks (`bytes[i]` is the range ending at `marks[i]`;
/// `bytes[0]` is unused).
struct TailBoundaries {
    marks: Vec<gtk4::TextMark>,
    bytes: Vec<u64>,
}

/// The boundary marks of every protected paragraph a sliced install put into
/// one buffer, in buffer order: per-buffer state that outlives the install
/// and that the next clear consumes. It holds marks and byte counts only,
/// never text.
#[derive(Default)]
pub(crate) struct BoundaryLedger {
    tails: RefCell<Vec<TailBoundaries>>,
}

/// What one ledger clear turn did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LedgerClearTurn {
    /// Recorded bytes deleted (head and tail bytes; 0 when only marks went).
    pub(crate) deleted_bytes: u64,
}

impl BoundaryLedger {
    /// Boundary marks currently in the buffer.
    pub(crate) fn live_boundaries(&self) -> u64 {
        self.tails
            .borrow()
            .iter()
            .map(|tail| tail.marks.len() as u64)
            .sum()
    }

    /// Recorded ranges a clear has still to delete.
    pub(crate) fn ranges_remaining(&self) -> u64 {
        self.tails
            .borrow()
            .iter()
            .map(|tail| tail.marks.len().saturating_sub(1) as u64)
            .sum()
    }

    /// Whether no boundary is recorded.
    pub(crate) fn is_empty(&self) -> bool {
        self.tails.borrow().is_empty()
    }

    fn begin_paragraph(&self, start: gtk4::TextMark) {
        self.tails.borrow_mut().push(TailBoundaries {
            marks: vec![start],
            bytes: vec![0],
        });
    }

    fn record_slice(&self, mark: gtk4::TextMark, bytes: u64) {
        if let Some(tail) = self.tails.borrow_mut().last_mut() {
            tail.marks.push(mark);
            tail.bytes.push(bytes);
        }
    }

    fn record_head(&self, bytes: u64) {
        if let Some(tail) = self.tails.borrow_mut().last_mut()
            && let Some(first) = tail.bytes.get_mut(1)
        {
            *first = first.saturating_add(bytes);
        }
    }

    /// Delete the first recorded ranges of the first protected paragraph that
    /// fit `PROTECTED_CLEAR_SLICE_BYTES` (at least one), in one `delete`,
    /// together with the boundary marks that deletion collapses (all of the
    /// paragraph's, once no range is left). Returns `None` when the
    /// ledger is empty, so the caller's ordinary clear takes over.
    ///
    /// From each paragraph's start forwards: a deletion costs time growing
    /// with the text before it in the paragraph, not after it (A76). The
    /// collapsed marks sit between the deletion point and the next range,
    /// separating no two char segments, so dropping them merges nothing.
    ///
    /// The caller runs it inside its own edit (`edit_lock::with_own_edit`).
    pub(crate) fn clear_turn(&self, buffer: &gtk4::TextBuffer) -> Option<LedgerClearTurn> {
        let mut tails = self.tails.borrow_mut();
        let tail = tails.first_mut()?;
        if tail.marks.len() < 2 {
            for mark in tail.marks.drain(..) {
                if !mark.is_deleted() {
                    buffer.delete_mark(&mark);
                }
            }
            tails.remove(0);
            return Some(LedgerClearTurn { deleted_bytes: 0 });
        }
        let count = clear_ranges_for_turn(&tail.bytes[1..], PRODUCTION_LIMITS);
        let start_mark = tail.marks[0].clone();
        let end_mark = tail.marks[count].clone();
        let deleted_bytes = tail.bytes[1..=count].iter().sum();
        // The range end becomes the paragraph's new first mark; once no range
        // is left, it goes in this turn too.
        let removed: Vec<gtk4::TextMark> = if tail.marks.len() == count + 1 {
            tails.remove(0).marks
        } else {
            tail.bytes.drain(1..=count);
            tail.marks.drain(..count).collect()
        };
        drop(tails);
        if !start_mark.is_deleted() && !end_mark.is_deleted() {
            let mut start = buffer.iter_at_mark(&start_mark);
            let mut end = buffer.iter_at_mark(&end_mark);
            if start < end {
                buffer.delete(&mut start, &mut end);
            }
        }
        for mark in removed {
            if !mark.is_deleted() {
                buffer.delete_mark(&mark);
            }
        }
        Some(LedgerClearTurn { deleted_bytes })
    }

    /// Forget every boundary, deleting its mark: for a buffer that was
    /// emptied another way (a direct replacement's `set_text`).
    pub(crate) fn forget(&self, buffer: &gtk4::TextBuffer) {
        for tail in self.tails.take() {
            for mark in tail.marks {
                if !mark.is_deleted() {
                    buffer.delete_mark(&mark);
                }
            }
        }
    }
}

/// A head waiting for its tail: its text range, the planner's position just
/// before it (so a turn that stops there replans it), and, once the tail
/// turned out to need several slices, the mark at the paragraph start where
/// it goes after the tail.
struct PendingHead {
    start: usize,
    end: usize,
    before: ProtectedInstallCursor,
    mark: Option<gtk4::TextMark>,
    /// Where the scan for the paragraph's end resumes.
    scan_from: usize,
}

/// Counts a sliced protected install reports (`load/evidence.rs`,
/// `buffer_replacement/evidence.rs`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FoldedInstallMetrics {
    /// Hidden-tail slices inserted.
    pub tail_slices: u64,
    /// The largest hidden-tail slice, in bytes.
    pub largest_tail_slice_bytes: u64,
}

/// The bytes and visible characters of `run`, as a turn's budget counts them
/// (delimiters counted as characters: conservative).
fn run_length(run: &str) -> ParagraphLength {
    ParagraphLength {
        bytes: run.len() as u64,
        chars: run.chars().count() as u64,
    }
}

/// The sliced protected install and clear as the load and replacement
/// evidence surfaces report them (C3, design D7).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtectedSliceEvidence {
    /// Hidden-tail slices the newest install inserted.
    pub tail_slices: u64,
    /// The largest of them, in bytes.
    pub largest_tail_slice_bytes: u64,
    /// Boundary marks now in the buffer.
    pub boundaries_live: u64,
    /// Recorded ranges a clear has still to delete.
    pub clear_ranges_remaining: u64,
    /// Ledger clear turns the newest clear ran.
    pub clear_turns: u64,
}

impl ProtectedSliceEvidence {
    /// The evidence of an install's `metrics`, `clear_turns` and the buffer's
    /// `ledger`.
    pub(crate) fn read(
        metrics: FoldedInstallMetrics,
        clear_turns: u64,
        ledger: &BoundaryLedger,
    ) -> Self {
        Self {
            tail_slices: metrics.tail_slices,
            largest_tail_slice_bytes: metrics.largest_tail_slice_bytes,
            boundaries_live: ledger.live_boundaries(),
            clear_ranges_remaining: ledger.ranges_remaining(),
            clear_turns,
        }
    }
}

/// One sliced install of a protected body: the planner's cursor plus the head
/// waiting for its tail. Kept by the install session between turns.
pub(crate) struct FoldedInstall {
    plan: Arc<FoldPlan>,
    cursor: ProtectedInstallCursor,
    pending_head: Option<PendingHead>,
    buffer: gtk4::TextBuffer,
    metrics: FoldedInstallMetrics,
    /// The right-gravity mark a sliced tail goes in at, before its
    /// already-inserted delimiter.
    tail_at: Option<gtk4::TextMark>,
    /// Bytes of a sliced paragraph's delimiter already inserted, which the
    /// next visible slice starts with.
    delimiter_inserted: usize,
    /// Whether a folded, undelimited last paragraph gets the stand-in
    /// terminator (editor buffers; not the read-only preview or an editable
    /// note, whose text is the buffer's).
    terminate: bool,
}

impl Drop for FoldedInstall {
    fn drop(&mut self) {
        // A cancelled install leaves no insertion mark behind.
        self.drop_tail_mark();
    }
}

impl FoldedInstall {
    /// A sliced install of the body `plan` was made for, into `buffer`.
    pub(crate) fn new(plan: Arc<FoldPlan>, buffer: &gtk4::TextBuffer, terminate: bool) -> Self {
        Self {
            plan,
            cursor: ProtectedInstallCursor::new(),
            pending_head: None,
            buffer: buffer.clone(),
            metrics: FoldedInstallMetrics::default(),
            tail_at: None,
            delimiter_inserted: 0,
            terminate,
        }
    }

    /// The counts so far.
    pub(crate) fn metrics(&self) -> FoldedInstallMetrics {
        self.metrics
    }

    /// Run one install turn of `text`. A turn inserts planned slices while
    /// they fit `budget` together (bytes, and visible characters for a dense
    /// body): whole visible paragraphs, and a folded paragraph whose tail fits
    /// one tail slice as its head then its tagged tail (C2's shape, no
    /// boundary). A tail that needs several slices goes in alone, one slice
    /// per turn: its first slice marks the paragraph start, each slice is
    /// followed by a boundary mark the `ledger` records, and its last slice
    /// is followed by the head, inserted at the paragraph-start mark (A76,
    /// A77). `append` gives the iterator where the install appends (an end
    /// mark or the buffer end). Returns the byte offset installed so far; the
    /// body is complete when it equals `text.len()`. Runs inside the caller's
    /// own edit.
    pub(crate) fn step(
        &mut self,
        append: &dyn Fn() -> gtk4::TextIter,
        text: &str,
        budget: SliceBudget,
        ledger: &BoundaryLedger,
    ) -> usize {
        let buffer = self.buffer.clone();
        let mut turn = PacedSlice::new(budget);
        loop {
            let before = self.cursor;
            let start = before.offset();
            let Some(slice) = plan_protected_install(
                text,
                self.plan.ranges(),
                &mut self.cursor,
                budget,
                PRODUCTION_LIMITS,
            ) else {
                self.insert_pending_head(append, text, ledger);
                return text.len();
            };
            match slice.kind {
                SliceKind::Visible => {
                    if !turn.admit(run_length(&text[start..slice.end])) {
                        self.cursor = before;
                        return start;
                    }
                    self.insert_pending_head(append, text, ledger);
                    // A sliced paragraph's delimiter is already in.
                    let skip = std::mem::take(&mut self.delimiter_inserted);
                    buffer.insert(&mut append(), &text[start + skip..slice.end]);
                }
                SliceKind::Head => {
                    self.pending_head = Some(PendingHead {
                        start,
                        end: slice.end,
                        before,
                        mark: None,
                        scan_from: slice.end,
                    });
                }
                SliceKind::HiddenTail => {
                    let one_slice_tail = slice.last_of_tail
                        && self
                            .pending_head
                            .as_ref()
                            .is_some_and(|head| head.mark.is_none());
                    if one_slice_tail {
                        let head = self.pending_head.take().expect("checked above");
                        // The tail is never laid out (A65), so only the head
                        // counts against a dense body's visible characters.
                        let folded = ParagraphLength {
                            bytes: (slice.end - head.start) as u64,
                            chars: run_length(&text[head.start..head.end]).chars,
                        };
                        if !turn.admit(folded) {
                            self.cursor = head.before;
                            return head.start;
                        }
                        buffer.insert(&mut append(), &text[head.start..head.end]);
                        self.insert_tail(&buffer, &mut append(), &text[start..slice.end]);
                        if self.terminate && slice.end == text.len() {
                            append_terminator(&buffer, &mut append());
                        }
                        continue;
                    }
                    if turn.paragraphs() > 0 {
                        // A multi-slice tail goes in alone: stop before it
                        // (before its head, if this would be its first slice).
                        match self.pending_head.as_ref() {
                            Some(head) if head.mark.is_none() => {
                                self.cursor = head.before;
                                let resume = head.start;
                                self.pending_head = None;
                                return resume;
                            }
                            _ => {
                                self.cursor = before;
                                return start;
                            }
                        }
                    }
                    if let Some(head) = self.pending_head.as_mut()
                        && head.mark.is_none()
                    {
                        // The paragraph's delimiter goes in before its tail,
                        // so the paragraph is never the last line while the
                        // tail grows: GTK maps a point below the layout to the
                        // last line's end, and every drawn frame would walk
                        // the hidden run (A79). Finding the delimiter scans
                        // at most `PROTECTED_TAIL_SCAN_BYTES` per turn.
                        let content_end = match paragraph_content_end(
                            text,
                            head.scan_from,
                            PROTECTED_TAIL_SCAN_BYTES,
                        ) {
                            Ok(end) => end,
                            Err(resume) => {
                                head.scan_from = resume;
                                self.cursor = before;
                                return start;
                            }
                        };
                        // The head goes in after the tail, at this mark
                        // (A76), so no tail slice re-lays it out (A77).
                        let mark = buffer.create_mark(None, &append(), true);
                        ledger.begin_paragraph(mark.clone());
                        if content_end < text.len() {
                            let delimiter_end = next_paragraph_break(text, content_end)
                                .map_or(content_end, |found| found.next_start);
                            buffer.insert(&mut append(), &text[content_end..delimiter_end]);
                            self.delimiter_inserted = delimiter_end - content_end;
                        } else if self.terminate {
                            // The body's last paragraph has no delimiter: the
                            // stand-in terminator keeps it off the last line
                            // (A79), from its first tail slice on.
                            append_terminator(&buffer, &mut append());
                        }
                        // Right gravity: each slice goes in before it, after
                        // the head mark and every boundary.
                        self.tail_at =
                            Some(buffer.create_mark(None, &buffer.iter_at_mark(&mark), false));
                        head.mark = Some(mark);
                    }
                    let mut at = self
                        .tail_at
                        .as_ref()
                        .map_or_else(append, |mark| buffer.iter_at_mark(mark));
                    self.insert_tail(&buffer, &mut at, &text[start..slice.end]);
                    let boundary = buffer.create_mark(None, &at, true);
                    ledger.record_slice(boundary, (slice.end - start) as u64);
                    if slice.last_of_tail {
                        self.drop_tail_mark();
                        self.insert_pending_head(append, text, ledger);
                    }
                    return slice.end;
                }
            }
        }
    }

    /// Delete the mark a sliced tail went in at.
    fn drop_tail_mark(&mut self) {
        if let Some(mark) = self.tail_at.take()
            && !mark.is_deleted()
        {
            self.buffer.delete_mark(&mark);
        }
    }

    /// Insert one tail slice hidden at `at`, which then points after it.
    fn insert_tail(&mut self, buffer: &gtk4::TextBuffer, at: &mut gtk4::TextIter, tail: &str) {
        let tag = fold_tag(buffer);
        let started_us = glib::monotonic_time();
        buffer.insert_with_tags(at, tail, &[&tag]);
        crate::ui::responsiveness_probe::record_span(
            "protected_tail_slice",
            started_us,
            glib::monotonic_time(),
        );
        note_tail_inserted();
        let bytes = tail.len() as u64;
        self.metrics.tail_slices = self.metrics.tail_slices.saturating_add(1);
        self.metrics.largest_tail_slice_bytes = self.metrics.largest_tail_slice_bytes.max(bytes);
    }

    /// Insert the head waiting for its tail, untagged: at its paragraph-start
    /// mark, where GTK places it after the mark and before the fold tag's
    /// toggle (A76), so it stays visible and the tail stays hidden; or, for a
    /// head whose tail has no slice at all (a plan the text does not match),
    /// where the install appends, which is still that paragraph's end.
    fn insert_pending_head(
        &mut self,
        append: &dyn Fn() -> gtk4::TextIter,
        text: &str,
        ledger: &BoundaryLedger,
    ) {
        let Some(head) = self.pending_head.take() else {
            return;
        };
        if head.end == head.start {
            return;
        }
        let mut at = match head.mark {
            Some(mark) if !mark.is_deleted() => {
                ledger.record_head((head.end - head.start) as u64);
                self.buffer.iter_at_mark(&mark)
            }
            _ => append(),
        };
        self.buffer.insert(&mut at, &text[head.start..head.end]);
    }
}
