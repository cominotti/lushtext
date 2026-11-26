// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A114: two directory rows, the first expanded over 4,096
//! children so the second lies far outside the realized band. "Hold far row"
//! fetches the second row's object and connects a counting
//! `notify::expanded` handler to it; "Drop and toggle" drops it and toggles
//! the row through a fresh fetch, which the dead object's handler never sees.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a114::row_object_fixture;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = Rc::new(row_object_fixture());
    ui.set_fixture(&fixture.scroller);
    let held: Rc<RefCell<Option<gtk4::TreeListRow>>> = Rc::new(RefCell::new(None));
    let notified = Rc::new(Cell::new(0u32));
    let dead = Rc::new(Cell::new(false));
    {
        let (fixture, held, notified) =
            (Rc::clone(&fixture), Rc::clone(&held), Rc::clone(&notified));
        ui.add_control("Hold far row", move || {
            if let Some(row) = fixture.second_row() {
                let notified = Rc::clone(&notified);
                row.connect_notify_local(Some("expanded"), move |_, _| {
                    notified.set(notified.get() + 1);
                });
                held.replace(Some(row));
            }
        });
    }
    {
        let (fixture, held, dead) = (Rc::clone(&fixture), Rc::clone(&held), Rc::clone(&dead));
        ui.add_control("Drop and toggle", move || {
            let weak = held.take().map(|row| row.downgrade());
            dead.set(weak.is_some_and(|weak| weak.upgrade().is_none()));
            if let Some(row) = fixture.second_row() {
                row.set_expanded(!row.is_expanded());
            }
        });
    }
    ui.set_readout(move || {
        format!(
            "held {} · handler fired {} · dropped object died {}",
            held.borrow().is_some(),
            notified.get(),
            dead.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(114), build)
}
