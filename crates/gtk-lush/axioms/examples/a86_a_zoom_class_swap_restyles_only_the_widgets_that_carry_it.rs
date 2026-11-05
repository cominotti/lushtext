// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A86: a window with a dense source view, 200 `.monospace` labels
//! and 1,500 unrelated rows; one button per mechanism zooms between 100 % and
//! 110 % of 11 pt and shows the longest frame-painting dispatch after it, and
//! one button measures every mechanism's median.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a86::{
    FontScopeFixture, Mechanism, REPS, UNRELATED_ROWS, describe, measure_all,
};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(
        "Opens a probe window with a dense source view, 200 .monospace labels and 1,500 \
         unrelated rows. Each button changes the zoom one way and shows that frame's cost.",
    ));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let fixture: Rc<RefCell<Option<FontScopeFixture>>> = Rc::new(RefCell::new(None));
    let readout = Rc::new(RefCell::new(String::from("press a button")));
    let ensure = {
        let fixture = Rc::clone(&fixture);
        move || {
            if fixture.borrow().is_none() {
                *fixture.borrow_mut() = FontScopeFixture::standalone(UNRELATED_ROWS, true);
            }
            fixture.borrow().is_some()
        }
    };
    for mechanism in Mechanism::ALL {
        let (fixture, readout, ensure) = (Rc::clone(&fixture), Rc::clone(&readout), ensure.clone());
        ui.add_control(mechanism.label(), move || {
            if !ensure() {
                *readout.borrow_mut() = "the fixture window never realized".to_owned();
                return;
            }
            let guard = fixture.borrow();
            let Some(fixture) = guard.as_ref() else {
                return;
            };
            fixture.enter(mechanism);
            let micros = fixture.step();
            *readout.borrow_mut() = format!(
                "{}: last frame {micros} µs, zoom {} %, view size {}, label size {}",
                mechanism.label(),
                fixture.zoom(),
                fixture.view_size(),
                fixture.label_size()
            );
        });
    }
    {
        let (fixture, readout) = (Rc::clone(&fixture), Rc::clone(&readout));
        ui.add_control("Measure every mechanism", move || {
            if !ensure() {
                *readout.borrow_mut() = "the fixture window never realized".to_owned();
                return;
            }
            let guard = fixture.borrow();
            if let Some(fixture) = guard.as_ref() {
                *readout.borrow_mut() = format!(
                    "medians of {REPS} (µs): {}",
                    describe(&measure_all(fixture, REPS))
                );
            }
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(86), build)
}
