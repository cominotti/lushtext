// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A71: the first write through a keyfile backend over a missing
//! file. Make the write (in an existing directory and in a missing one) and
//! read the changed emissions on each instance.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a71::first_write;

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some("A private keyfile backend; no widget is involved."));
    ui.set_fixture(&label);
    let lines = Rc::new(RefCell::new(Vec::<String>::new()));
    {
        let lines = lines.clone();
        ui.add_control("Make the first write", move || {
            for create_dir in [true, false] {
                if let Some(run) = first_write(create_dir) {
                    lines.borrow_mut().push(format!(
                        "directory existed {create_dir}: peer changed inside/after {}/{}, {} \
                         changed inside/after {}/{}",
                        run.peer.0, run.peer.1, run.other.3, run.other.0, run.other.1
                    ));
                }
            }
        });
    }
    ui.set_readout(move || lines.borrow().join("\n"));
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(71), build)
}
