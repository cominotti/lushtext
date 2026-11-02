// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A68: turning highlighting on refreshes the whole buffer inside
//! the call, and inserts into a language-attached buffer only invalidate. Run
//! the toggle at each size and the sliced inserts, and read the costs.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a68::{TOGGLE_SIZES, insert_run, toggle_run};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some("No fixture view: the probe uses bare buffers."));
    ui.set_fixture(&label);
    let lines = Rc::new(RefCell::new(Vec::<String>::new()));
    {
        let lines = lines.clone();
        ui.add_control("Turn highlighting on at each size", move || {
            for bytes in TOGGLE_SIZES {
                if let Some(run) = toggle_run(bytes) {
                    lines.borrow_mut().push(format!(
                        "{bytes} B: {} us, {} emission(s) inside the call",
                        run.toggle_us, run.emissions_inside
                    ));
                }
            }
        });
    }
    {
        let lines = lines.clone();
        ui.add_control("Insert 1 MiB in 256 KiB slices", move || {
            if let Some(run) = insert_run(true) {
                lines.borrow_mut().push(format!(
                    "attach {} us; inserts {:?} us; {} emissions inside, {} after",
                    run.attach_us, run.insert_us, run.emissions_inside, run.emissions_after
                ));
            }
        });
    }
    ui.set_readout(move || lines.borrow().join("\n"));
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(68), build)
}
