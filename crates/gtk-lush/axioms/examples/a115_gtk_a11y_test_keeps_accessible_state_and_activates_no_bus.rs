// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A115: a labelled widget with an accessible description. The
//! readout shows `GTK_A11Y`, the widget's AT context type, whether the
//! description is queryable, and whether `org.a11y.Bus` is on the session
//! bus. Run it with `GTK_A11Y=test` and without to compare. `--check` runs
//! the probe, whose control runs in a nested private session.

mod support;

use gtk_lush_axioms::{A11Y_BUS_NAME, AxiomId, session_bus_has_name};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some("A labelled fixture"));
    label.update_property(&[gtk4::accessible::Property::Description(
        "the accessible property the probe reads back",
    )]);
    ui.set_fixture(&label);
    // Changing the description shows the AT context keeping state under
    // every backend; only `atspi` also exports it on a bus.
    let edited = label.clone();
    ui.add_control("Change the description", move || {
        edited.update_property(&[gtk4::accessible::Property::Description(
            "a description changed from the sample",
        )]);
    });
    ui.set_readout(move || {
        let bus = session_bus_has_name(A11Y_BUS_NAME)
            .map_or_else(|error| format!("unknown ({error})"), |present| present.to_string());
        format!(
            "GTK_A11Y={}; AT context: {}; description queryable: {}; {A11Y_BUS_NAME} on the bus: {bus}",
            std::env::var("GTK_A11Y").unwrap_or_else(|_| "(unset)".to_owned()),
            label.at_context().type_().name(),
            gtk4::test_accessible_has_property(&label, gtk4::AccessibleProperty::Description),
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(115), build)
}
