// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A54: a view with the cursor in the middle of a long paragraph.
//! "Ask for surrounding text" queries the view's input-method context; GTK's
//! own answer starts at the paragraph's start. "Connect a bounded answer"
//! connects a handler answering with a short window after the view's; the
//! readout shows it is never called and the answer stays the paragraph.
//! "Time the answer on 50k characters" records, on a second view, the median
//! of five answers on a 50,000-character paragraph: cold, repeated, and right
//! after a one-character insert.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::TextViewFixture;
use gtk_lush_axioms::fixtures::a54::{
    COST_PARAGRAPH_CHARS, connect_bounded_answer, fixture, surrounding_costs, view_im_context,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = fixture();
    ui.set_fixture(&fixture.scrolled);
    let note = Rc::new(RefCell::new(String::from("not asked yet")));
    {
        let (view, note) = (fixture.view.clone(), note.clone());
        ui.add_control("Ask for surrounding text", move || {
            let answer = view_im_context(&view).and_then(|im| im.surrounding_with_selection());
            *note.borrow_mut() = answer.map_or_else(
                || "no answer".to_owned(),
                |(text, cursor, _)| {
                    format!(
                        "{} characters, cursor at byte {cursor}",
                        text.chars().count()
                    )
                },
            );
        });
    }
    let costs = Rc::new(RefCell::new(String::from("not timed yet")));
    {
        let costs = costs.clone();
        let costly = TextViewFixture::new(gtk4::WrapMode::Word);
        ui.add_control("Time the answer on 50k characters", move || {
            *costs.borrow_mut() = view_im_context(&costly.view).map_or_else(
                || "no input-method context".to_owned(),
                |im| {
                    let measured = surrounding_costs(&costly, &im);
                    format!(
                        "{COST_PARAGRAPH_CHARS} characters, median of 5: cold {} µs, repeat {} µs, \
                         after an insert {} µs",
                        measured.cold_us, measured.repeat_us, measured.after_insert_us
                    )
                },
            );
        });
    }
    let calls: Rc<RefCell<Option<Rc<Cell<u32>>>>> = Rc::default();
    {
        let (view, calls) = (fixture.view, calls.clone());
        ui.add_control("Connect a bounded answer", move || {
            if calls.borrow().is_none()
                && let Some(im) = view_im_context(&view)
            {
                *calls.borrow_mut() = Some(connect_bounded_answer(&im, &view));
            }
        });
    }
    ui.set_readout(move || {
        let later = calls.borrow().as_ref().map_or_else(
            || "not connected".to_owned(),
            |calls| format!("called {} times", calls.get()),
        );
        format!(
            "last answer: {}; bounded handler: {later}\ncost: {}",
            note.borrow(),
            costs.borrow()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(54), build)
}
