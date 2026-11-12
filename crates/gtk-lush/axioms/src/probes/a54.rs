// SPDX-License-Identifier: MIT OR Apache-2.0

//! A54: a `GtkTextView` answers its input method's `retrieve-surrounding` with
//! text that starts at the **cursor paragraph's start** (or three words
//! before the cursor, whichever is earlier) and runs to its end, computing the
//! paragraph's word boundaries to do so (A26, A30). Its handler is connected to
//! the view's input-method context — the one its key controller exposes
//! (`gtk_event_controller_key_get_im_context`) — before anything else can
//! connect, and its `TRUE` stops the emission: a handler connected later is
//! **never called**, so an application can only replace the answer by
//! blocking the view's handler.
//!
//! LushText does that, matching the handler by the view as its data
//! (`g_signal_handlers_block_matched`, outside this crate, which forbids
//! `unsafe`), and answers with a bounded window
//! (`explore-editing-long-paragraphs`, design D6): GNOME's text-input path
//! asks on every keystroke, and the whole-paragraph word-boundary walk cost
//! about 13 ms per keystroke in a 50,000-character paragraph. That the match
//! finds the view's handler is pinned by LushText's consumer widget test,
//! whose bounded answer would be the whole paragraph otherwise.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::post_open::median;
use crate::probes::text::{ParagraphKind, TextViewFixture, paragraph, time_us};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in the paragraph the cursor sits in.
pub const PARAGRAPH_CHARS: usize = 20_000;
/// Characters in the paragraph the answer's cost is recorded on.
pub const COST_PARAGRAPH_CHARS: usize = 50_000;
/// Emissions per recorded median.
pub const COST_REPETITIONS: usize = 5;
/// Characters of context the replacement answer gives on each side.
pub const WINDOW_CHARS: i32 = 100;

/// The view's input-method context, from its key controller.
#[must_use]
pub fn view_im_context(view: &gtk4::TextView) -> Option<gtk4::IMContext> {
    let controllers = view.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|item| item.downcast::<gtk4::EventControllerKey>().ok())
        .find_map(|key| key.im_context())
}

/// A view with a short line, one long paragraph, the cursor in its middle.
#[must_use]
pub fn fixture() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::Word);
    fixture.buffer.set_text(&format!(
        "short line\n{}\n",
        paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 54)
    ));
    let middle = fixture
        .buffer
        .iter_at_line_offset(1, i32::try_from(PARAGRAPH_CHARS / 2).unwrap_or(0));
    if let Some(middle) = middle {
        fixture.buffer.place_cursor(&middle);
    }
    fixture
}

/// Microseconds of one `retrieve-surrounding` emission on `im` (the view's
/// own answer).
#[must_use]
pub fn surrounding_us(im: &gtk4::IMContext) -> u128 {
    time_us(|| im.surrounding_with_selection()).1
}

/// The answer's cost on a [`COST_PARAGRAPH_CHARS`]-character paragraph with
/// the cursor in its middle, as medians of [`COST_REPETITIONS`] emissions:
/// cold (each right after the paragraph is set again), repeated with nothing
/// changed, and right after a one-character insert at the cursor. GTK
/// !10646 (in 4.25.0) changes what the word-boundary cache does after an
/// edit; these are the numbers to compare, never a verdict.
#[must_use]
pub fn surrounding_costs(fixture: &TextViewFixture, im: &gtk4::IMContext) -> SurroundingCosts {
    let reset = || {
        fixture
            .buffer
            .set_text(&paragraph(ParagraphKind::Ascii, COST_PARAGRAPH_CHARS, 54));
        let middle = fixture
            .buffer
            .iter_at_offset(i32::try_from(COST_PARAGRAPH_CHARS / 2).unwrap_or(0));
        fixture.buffer.place_cursor(&middle);
    };
    let cold: Vec<u128> = (0..COST_REPETITIONS)
        .map(|_| {
            reset();
            surrounding_us(im)
        })
        .collect();
    let repeat: Vec<u128> = (0..COST_REPETITIONS).map(|_| surrounding_us(im)).collect();
    let after_insert: Vec<u128> = (0..COST_REPETITIONS)
        .map(|_| {
            fixture.buffer.insert_at_cursor("x");
            surrounding_us(im)
        })
        .collect();
    SurroundingCosts {
        cold_us: median(&cold),
        repeat_us: median(&repeat),
        after_insert_us: median(&after_insert),
    }
}

/// Medians of the view's own `retrieve-surrounding` answer, in microseconds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SurroundingCosts {
    /// Right after the paragraph was set.
    pub cold_us: u128,
    /// Asked again with nothing changed.
    pub repeat_us: u128,
    /// Right after a one-character insert at the cursor.
    pub after_insert_us: u128,
}

/// Answer `retrieve-surrounding` on `im` with `WINDOW_CHARS` on each side of
/// the cursor; counts its calls.
#[must_use]
pub fn connect_bounded_answer(im: &gtk4::IMContext, view: &gtk4::TextView) -> Rc<Cell<u32>> {
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let view_weak = view.downgrade();
    im.connect_retrieve_surrounding(move |im| {
        let Some(view) = view_weak.upgrade() else {
            return false;
        };
        counter.set(counter.get() + 1);
        let buffer = view.buffer();
        let insert = buffer.iter_at_mark(&buffer.get_insert());
        let mut start = insert;
        start.backward_chars(WINDOW_CHARS);
        let mut end = insert;
        end.forward_chars(WINDOW_CHARS);
        let before = buffer.slice(&start, &insert, false);
        let cursor = i32::try_from(before.len()).unwrap_or(0);
        im.set_surrounding_with_selection(&buffer.slice(&start, &end, false), cursor, cursor);
        true
    });
    calls
}

/// Probe A54.
#[must_use]
pub fn probe_a54() -> Observation {
    Recorder::run(AxiomId::new(54), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = fixture();
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the view was presented",
        )?;
        settle(LAYOUT_SETTLE);
        let view = fixture.view.clone();
        let im = recorder.require(
            view_im_context(&view),
            "control: the view's key controller exposes its input-method context",
        )?;
        let gtk_answer = im.surrounding_with_selection();
        let gtk_chars = gtk_answer
            .as_ref()
            .map_or(0, |(text, _, _)| text.chars().count());
        recorder.measure("view_answer_chars", gtk_chars);
        recorder.axiom(
            gtk_chars > PARAGRAPH_CHARS / 2,
            "the view answers with text from the cursor paragraph's start, not a window",
        )?;
        let calls = connect_bounded_answer(&im, &view);
        let answer = im.surrounding_with_selection();
        let chars = answer
            .as_ref()
            .map_or(0, |(text, _, _)| text.chars().count());
        recorder.measure("later_handler_calls", calls.get());
        recorder.measure("answer_chars_with_a_later_handler", chars);
        recorder.axiom(
            calls.get() == 0 && chars == gtk_chars,
            "a handler connected after the view's is never called: the view's answer stops the emission",
        )?;
        // Recorded only, after the verdict: what one answer costs on a long
        // paragraph, on a second view whose context has no later handler.
        let costly = TextViewFixture::new(gtk4::WrapMode::Word);
        let _costly_shown = Presented::checked(
            recorder,
            &costly.scrolled,
            "control: the cost view was presented",
        )?;
        let costly_im = recorder.require(
            view_im_context(&costly.view),
            "control: the cost view exposes its input-method context",
        )?;
        let costs = surrounding_costs(&costly, &costly_im);
        recorder.measure("surrounding_50k_cold_median_us", costs.cold_us);
        recorder.measure("surrounding_50k_repeat_median_us", costs.repeat_us);
        recorder.measure(
            "surrounding_50k_after_insert_median_us",
            costs.after_insert_us,
        );
        Ok(())
    })
}
