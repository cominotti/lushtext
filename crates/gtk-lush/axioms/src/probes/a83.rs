// SPDX-License-Identifier: MIT OR Apache-2.0

//! A83: `gtk_text_view_set_buffer` to another (empty) buffer **stops a hidden
//! view's validation at once**: the layout is moved to the new buffer
//! (`gtk_text_layout_set_buffer`), which drops every line's layout data for
//! the old one, and an empty buffer has nothing left to validate, so a
//! `G_PRIORITY_DEFAULT_IDLE` source armed after it dispatches at once. The
//! document buffer itself is untouched: its text, character count, modified
//! flag, undo state and caret offset are what they were.
//!
//! Setting the document buffer back **lays the whole buffer out again from
//! nothing**, as on a first load: the lower source waits for the whole
//! document again, however often it had been validated before. The view also
//! comes back at the **start of the buffer** (`set_buffer` recreates its
//! `first_para_mark` at offset 0). A `scroll_to_mark` to a kept mark restores
//! the old top line once the view is laid out, but one queued **right after**
//! the reattach lands at another line: the view validates only around the
//! target (`gtk_text_view_flush_scroll`, ±2 view heights), takes its y from
//! the heights known so far, and keeps that y as the lines above get their
//! heights. Scrolling to the target again (`gtk_text_view_scroll_to_iter`, which
//! scrolls at once) after every validation batch (each grows the adjustment's
//! `upper`) keeps the top near the target while the view lays out (a few to
//! a few tens of lines off, by timing), and one more scroll once it is laid
//! out lands on it exactly.
//!
//! While a hidden view validates, every 2000 px batch grows its vertical
//! adjustment's `upper` (A31, read here on a hidden page), so a `notify::upper`
//! on an unmapped view is evidence that it is laying out its buffer.
//!
//! Recorded magnitudes, never verdicts: iterations before the lower source
//! dispatched and the upper notifications counted.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::a82::{
    AT_ONCE_ITERATIONS, LowerDelivery, TabPageView, describe, load_and_trace, trace_lower_delivery,
};
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

use super::{LAYOUT_SETTLE, SCROLL_SETTLE};

/// Character offset the probe places the caret at before detaching.
pub const CARET_OFFSET: i32 = 1 << 12;
/// Line the probe scrolls to the top of the view before detaching.
pub const TOP_LINE: i32 = 1 << 10;

/// A presented tab view with a label page (selected) and one document page.
pub struct DetachFixture {
    /// The tab view.
    pub tab_view: libadwaita::TabView,
    /// The label page.
    pub front: libadwaita::TabPage,
    /// The document page.
    pub page: libadwaita::TabPage,
    /// The document view and its buffer.
    pub document: TabPageView,
    /// The empty buffer the view is given while detached.
    pub placeholder: sourceview5::Buffer,
    _shown: Presented,
}

impl DetachFixture {
    fn new(recorder: &mut Recorder) -> Result<Self, Stop> {
        let tab_view = libadwaita::TabView::new();
        let front = tab_view.append(&gtk4::Label::new(Some("front page")));
        let document = TabPageView::new(true);
        let page = tab_view.append(&document.page_child);
        let shown = Presented::checked(
            recorder,
            &tab_view,
            "control: the tab view's window was realized",
        )?;
        settle(LAYOUT_SETTLE);
        Ok(Self {
            tab_view,
            front,
            page,
            document,
            placeholder: sourceview5::Buffer::new(None),
            _shown: shown,
        })
    }

    /// The fixture for a sample; `None` when the window never realized.
    #[must_use]
    pub fn standalone() -> Option<Self> {
        Self::new(&mut Recorder::detached()).ok()
    }

    /// Give the view the empty placeholder buffer.
    pub fn detach(&self) {
        self.document.view.set_buffer(Some(&self.placeholder));
    }

    /// Give the view its document buffer back.
    pub fn attach(&self) {
        self.document.view.set_buffer(Some(&self.document.buffer));
    }

    /// The line at the top of the view's visible area.
    #[must_use]
    pub fn top_line(&self) -> i32 {
        let view = &self.document.view;
        let rect = view.visible_rect();
        view.line_at_y(rect.y()).0.line()
    }
}

/// What the document buffer looked like, compared across a detach.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferState {
    chars: i32,
    modified: bool,
    can_undo: bool,
    caret: i32,
    text_digest: u64,
}

impl BufferState {
    /// Capture `buffer`'s observable state.
    #[must_use]
    pub fn of(buffer: &sourceview5::Buffer) -> Self {
        use std::hash::{Hash, Hasher};
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.as_str().hash(&mut hasher);
        Self {
            chars: buffer.char_count(),
            modified: buffer.is_modified(),
            can_undo: buffer.can_undo(),
            caret: buffer.iter_at_mark(&buffer.get_insert()).offset(),
            text_digest: hasher.finish(),
        }
    }
}

fn waited(delivery: Option<LowerDelivery>) -> bool {
    delivery.is_some_and(|d| d.iteration > AT_ONCE_ITERATIONS)
}

fn at_once(delivery: Option<LowerDelivery>) -> bool {
    delivery.is_some_and(|d| d.iteration <= AT_ONCE_ITERATIONS)
}

/// Probe A83.
#[must_use]
pub fn probe_a83() -> Observation {
    Recorder::run(AxiomId::new(83), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = DetachFixture::new(recorder)?;
        let document = &fixture.document;

        // Hidden and validating: count `upper` notifications, then detach
        // with the backlog still pending.
        let uppers = Rc::new(Cell::new(0u32));
        let vadjustment = document.view.vadjustment();
        let handler = vadjustment.as_ref().map(|adjustment| {
            let uppers = Rc::clone(&uppers);
            adjustment.connect_upper_notify(move |_| uppers.set(uppers.get() + 1))
        });
        let full = load_and_trace(&document.buffer);
        recorder.measure("hidden_full_validation", describe(full));
        recorder.measure("hidden_upper_notifications", uppers.get());
        recorder.control(
            waited(full),
            "control: the hidden page validated the loaded document",
        )?;
        recorder.axiom(
            uppers.get() > 1 && !document.view.is_mapped(),
            "an unmapped view validating its buffer notifies its vertical adjustment's upper",
        )?;
        if let (Some(adjustment), Some(handler)) = (vadjustment, handler) {
            adjustment.disconnect(handler);
        }

        let mut caret = document.buffer.iter_at_offset(CARET_OFFSET);
        document.buffer.place_cursor(&caret);
        let before = BufferState::of(&document.buffer);
        // Append more text, so a backlog is pending, then detach at once.
        let mut end = document.buffer.end_iter();
        document.buffer.insert(&mut end, "\n");
        caret = document.buffer.iter_at_offset(CARET_OFFSET);
        document.buffer.place_cursor(&caret);
        let pending = BufferState::of(&document.buffer);
        fixture.detach();
        let detached = trace_lower_delivery();
        recorder.measure("after_detach", describe(detached));
        recorder.axiom(
            at_once(detached),
            "giving a hidden validating view an empty buffer stops its validation at once",
        )?;
        recorder.axiom(
            BufferState::of(&document.buffer) == pending && pending.chars == before.chars + 1,
            "the document buffer's text, modified flag, undo state and caret are unchanged by \
             the detach",
        )?;

        fixture.attach();
        let reattached = trace_lower_delivery();
        recorder.measure("after_reattach_hidden", describe(reattached));
        recorder.axiom(
            waited(reattached),
            "setting the document buffer back lays the whole buffer out again",
        )?;
        recorder.control(
            BufferState::of(&document.buffer) == pending,
            "control: the buffer is unchanged by the reattach",
        )?;

        // Shown, scrolled, then detached and reattached while shown.
        fixture.tab_view.set_selected_page(&fixture.page);
        settle(LAYOUT_SETTLE);
        if let Some(target) = document.buffer.iter_at_line(TOP_LINE) {
            let mark = document.buffer.create_mark(None, &target, true);
            document.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            settle(SCROLL_SETTLE);
            let scrolled_top = fixture.top_line();
            recorder.measure("scrolled_top_line", scrolled_top);
            recorder.control(
                scrolled_top == TOP_LINE,
                "control: the shown view scrolled to the target line",
            )?;
            fixture.detach();
            fixture.attach();
            settle(SCROLL_SETTLE);
            let reset_top = fixture.top_line();
            recorder.measure("reattached_top_line", reset_top);
            recorder.axiom(
                reset_top == 0,
                "a reattached view comes back at the start of the buffer",
            )?;
            document.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            settle(SCROLL_SETTLE);
            let restored_top = fixture.top_line();
            recorder.measure("restored_top_line", restored_top);
            recorder.axiom(
                restored_top == TOP_LINE,
                "scrolling to a kept mark once the reattached view is laid out restores the old \
                 top line",
            )?;

            // The same scroll queued at once after the reattach, before the
            // lines above the target have heights, lands elsewhere ...
            fixture.detach();
            fixture.attach();
            document.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            settle(SCROLL_SETTLE);
            let early_top = fixture.top_line();
            recorder.measure("early_scroll_top_line", early_top);
            recorder.axiom(
                early_top != TOP_LINE,
                "a scroll queued right after the reattach lands at another line once the lines \
                 above the target are laid out",
            )?;
            // ... unless the view scrolls to it again (`scroll_to_iter`, at
            // once) after every validation batch (each grows `upper`).
            fixture.detach();
            fixture.attach();
            document.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            let adjustment = document.view.vadjustment();
            let reassert = adjustment.as_ref().map(|adjustment| {
                let (view, mark) = (document.view.clone(), mark.clone());
                adjustment.connect_upper_notify(move |_| {
                    let mut target = view.buffer().iter_at_mark(&mark);
                    view.scroll_to_iter(&mut target, 0.0, true, 0.0, 0.0);
                })
            });
            settle(SCROLL_SETTLE);
            if let (Some(adjustment), Some(reassert)) = (adjustment, reassert) {
                adjustment.disconnect(reassert);
            }
            let reasserted_top = fixture.top_line();
            recorder.measure("reasserted_scroll_top_line", reasserted_top);
            recorder.axiom(
                reasserted_top.abs_diff(TOP_LINE) < early_top.abs_diff(TOP_LINE),
                "scrolling to the target again on every upper change while the view lays the \
                 document out keeps the top much nearer the target than one early scroll",
            )?;
            let mut target = document.buffer.iter_at_mark(&mark);
            document
                .view
                .scroll_to_iter(&mut target, 0.0, true, 0.0, 0.0);
            settle(SCROLL_SETTLE);
            let final_top = fixture.top_line();
            recorder.measure("final_scroll_top_line", final_top);
            recorder.axiom(
                final_top == TOP_LINE,
                "one more scroll once the view is laid out lands on the target line",
            )?;
            document.buffer.delete_mark(&mark);
        }
        fixture.tab_view.set_selected_page(&fixture.front);
        drop(fixture);
        flush_events();
        Ok(())
    })
}
