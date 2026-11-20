// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A134: a popover repositioned right after its first buffer is
//! committed. GTK 4.24 prints `gdk_frame_timings_presented() called on
//! discarded frame.` once per process when the compositor discards that
//! buffer's feedback and the toplevel's feedback of the same frame is
//! presented after it. The control runs the fixture (a toplevel with a row
//! column and a peek-like popover, 60 popup/content/popdown cycles, each
//! repositioned in the frame clock's `after-paint`) in a child of this sample
//! with `WAYLAND_DEBUG=client`, so the once-per-process warning is the
//! child's; the readout shows what its protocol trace and its stderr held.
//! The control blocks the sample while the child runs, a few seconds.
//! `--check` runs the probe.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::{AxiomId, popover_frames_in_child};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some(
        "The fixture runs in its own window, in a child process",
    ));
    ui.set_fixture(&label);
    let last = Rc::new(RefCell::new(None::<String>));
    ui.add_control("Run the fixture in a traced child", {
        let last = Rc::clone(&last);
        move || {
            let outcome = popover_frames_in_child().map_or_else(
                |error| format!("did not start ({error})"),
                |outcome| outcome.summary(),
            );
            last.replace(Some(outcome));
        }
    });
    ui.set_readout(move || {
        format!(
            "GTK {}.{}.{}; {}",
            gtk4::major_version(),
            gtk4::minor_version(),
            gtk4::micro_version(),
            last.borrow()
                .clone()
                .unwrap_or_else(|| "no child run yet".to_owned())
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(134), build)
}
