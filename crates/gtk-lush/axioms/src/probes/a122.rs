// SPDX-License-Identifier: MIT OR Apache-2.0

//! A122: `GtkTextView::extend-selection`, which GTK emits on a double or
//! triple click to grow the selection to a word or a line, returns
//! `gboolean`, runs its class handler last (`G_SIGNAL_RUN_LAST`) and stops at
//! the first handler that returns TRUE (`_gtk_boolean_handled_accumulator`).
//! So a handler connected before the class handler that returns **FALSE**
//! lets the emission go on to every later handler and to GtkTextView's own,
//! which extends the selection and returns TRUE, after which no
//! after-handler runs; a handler that returns **TRUE** claims the emission:
//! no later handler runs, and neither does GtkTextView's.
//!
//! LushText's Protected View answers TRUE, so a double click in a caret-free
//! document selects nothing, and FALSE everywhere else, so GTK selects the
//! word (`ui/editor_page/protected_view/interaction_execution.rs`). The
//! answer must be a `gboolean`: a glib-rs handler that answers nothing is not
//! FALSE, it aborts the process inside the emission
//! (`fix-extend-selection-abort`).
//!
//! The probe emits the signal itself with word granularity, as GTK does on a
//! double click: what the emission reaches is observable through which
//! handlers run, while the iterators it fills are copies the emitter cannot
//! read.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use super::text::TextViewFixture;
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The text the probe's view holds.
pub const DOCUMENT: &str = "hello world\nsecond line of the sample\n";
/// The character the emission starts from: inside "hello".
pub const LOCATION_OFFSET: i32 = 1;

/// An unwrapped text view holding [`DOCUMENT`].
#[must_use]
pub fn document_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    fixture.buffer.set_text(DOCUMENT);
    fixture
}

/// Connect an `extend-selection` handler that answers `answer` (TRUE claims
/// the emission, FALSE passes it on), before the class handler or `after` it.
/// Returns the handler id and a count of the emissions it saw.
pub fn connect_answer(
    view: &gtk4::TextView,
    answer: Rc<Cell<bool>>,
    after: bool,
) -> (glib::SignalHandlerId, Rc<Cell<u32>>) {
    let calls = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&calls);
    let handler = view.connect_local("extend-selection", after, move |_| {
        counter.set(counter.get() + 1);
        Some(answer.get().to_value())
    });
    (handler, calls)
}

/// Emit `extend-selection` with word granularity at [`LOCATION_OFFSET`], as
/// GTK does on a double click, and return the emission's answer.
#[must_use]
pub fn emit_word_extension(fixture: &TextViewFixture) -> bool {
    let location = fixture.buffer.iter_at_offset(LOCATION_OFFSET);
    fixture.view.emit_by_name::<bool>(
        "extend-selection",
        &[
            &gtk4::TextExtendSelection::Word,
            &location,
            &location,
            &location,
        ],
    )
}

/// Probe A122.
#[must_use]
pub fn probe_a122() -> Observation {
    Recorder::run(AxiomId::new(122), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = document_view();
        let (signal, _) = recorder.require(
            glib::subclass::SignalId::parse_name(
                "extend-selection",
                gtk4::TextView::static_type(),
                false,
            ),
            "control: GtkTextView has an extend-selection signal",
        )?;
        let query = signal.query();
        let declared = glib::Type::from(query.return_type());
        recorder.measure("return_type", declared);
        recorder.measure("flags", format!("{:?}", query.flags()));
        recorder.control(
            declared == glib::Type::BOOL && query.flags().contains(glib::SignalFlags::RUN_LAST),
            "control: extend-selection returns gboolean and runs its class handler last",
        )?;
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        settle(LAYOUT_SETTLE);

        // A FALSE handler first, then a later one, then an after-handler.
        let first_answer = Rc::new(Cell::new(false));
        let (first, first_calls) = connect_answer(&fixture.view, first_answer.clone(), false);
        let (later, later_calls) = connect_answer(&fixture.view, Rc::new(Cell::new(false)), false);
        let (after, after_calls) = connect_answer(&fixture.view, Rc::new(Cell::new(false)), true);
        let passed = emit_word_extension(&fixture);
        recorder.measure("passing_answer", passed);
        recorder.measure(
            "passing_calls_first_later_after",
            format!(
                "{} {} {}",
                first_calls.get(),
                later_calls.get(),
                after_calls.get()
            ),
        );
        recorder.axiom(
            passed && first_calls.get() == 1 && later_calls.get() == 1 && after_calls.get() == 0,
            "a handler returning FALSE lets extend-selection go on to later handlers and to \
             GtkTextView's handler, whose TRUE ends the emission before any after-handler",
        )?;

        // The same first handler, now answering TRUE.
        first_answer.set(true);
        let passed_calls = (first_calls.get(), later_calls.get(), after_calls.get());
        let claimed = emit_word_extension(&fixture);
        let claiming_calls = (
            first_calls.get() - passed_calls.0,
            later_calls.get() - passed_calls.1,
            after_calls.get() - passed_calls.2,
        );
        recorder.measure("claiming_answer", claimed);
        recorder.measure(
            "claiming_calls_first_later_after",
            format!(
                "{} {} {}",
                claiming_calls.0, claiming_calls.1, claiming_calls.2
            ),
        );
        for handler in [first, later, after] {
            fixture.view.disconnect(handler);
        }
        recorder.axiom(
            claimed && claiming_calls == (1, 0, 0),
            "a handler returning TRUE claims extend-selection: no later handler runs, and \
             neither does GtkTextView's",
        )
    })
}
