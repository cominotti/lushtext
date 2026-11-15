// SPDX-License-Identifier: MIT OR Apache-2.0

//! A115: with `GTK_A11Y=test`, GTK 4 gives each widget a test AT context
//! (`GtkTestATContext`) whose accessible properties stay queryable through
//! the `gtk_test_accessible_*` API, and puts nothing on the session bus:
//! `org.a11y.Bus` is never activated. Without the variable (GTK 4 ignores
//! `NO_AT_BRIDGE`), presenting the same widget activates `org.a11y.Bus`.
//!
//! The probe presents a labelled widget in the axioms session (where the
//! session helper set `GTK_A11Y=test`), reads the widget's AT context type
//! and a property it set, waits out an activation window and reads the
//! session bus's `ListNames`. Its control runs the same client with the
//! accessibility bus allowed and `GTK_A11Y` unset, in a nested private session
//! (`session_names`), and requires `org.a11y.Bus` to appear there.

use gtk4::prelude::*;

use crate::AxiomId;
use crate::observation::{Observation, Recorder};
use crate::probes::session_names::{
    A11Y_BUS_NAME, SESSION_BIN_ENV, control_prerequisite, data_dirs, labelled_fixture,
    name_present, present_and_wait, run_nested_control,
};

/// Probe A115. See the module documentation.
#[must_use]
pub fn probe_a115() -> Observation {
    Recorder::run(AxiomId::new(115), |recorder| {
        let gtk_a11y = std::env::var("GTK_A11Y").unwrap_or_default();
        recorder.measure("gtk_a11y", &gtk_a11y);
        recorder.control(
            gtk_a11y == "test",
            "control: the session sets GTK_A11Y=test",
        )?;
        let helper = std::env::var_os(SESSION_BIN_ENV);
        let helper = match control_prerequisite(helper.as_deref(), A11Y_BUS_NAME, &data_dirs()) {
            Ok(helper) => helper.to_path_buf(),
            Err(missing) => return recorder.control(false, missing),
        };

        let label = labelled_fixture();
        let shown = present_and_wait(&label);
        let context_type = label.at_context().type_().name().to_owned();
        let property_queryable =
            gtk4::test_accessible_has_property(&label, gtk4::AccessibleProperty::Description);
        let bus_present = recorder.require(
            name_present(A11Y_BUS_NAME).ok(),
            "control: the probe reads the session bus's ListNames",
        )?;
        drop(shown);
        recorder.measure("at_context_type", &context_type);
        recorder.measure("description_queryable", property_queryable);
        recorder.measure("a11y_bus_with_test", bus_present);

        let control = run_nested_control(
            &helper,
            AxiomId::new(115),
            A11Y_BUS_NAME,
            &["--a11y", "atspi"],
        );
        if let Err(error) = &control {
            recorder.measure("control_error", error);
        }
        let control = recorder.require(
            control.ok(),
            "control: the nested control session ran and printed its result",
        )?;
        recorder.measure("a11y_bus_without_variable", control.appeared);
        recorder.measure(
            "control_session_status",
            control
                .status
                .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
        );
        recorder.control(
            control.status == Some(0),
            "control: the nested control session's own guard was clean",
        )?;
        recorder.control(
            control.appeared,
            "control: without GTK_A11Y, presenting the widget activates org.a11y.Bus",
        )?;
        recorder.axiom(
            context_type == "GtkTestATContext" && property_queryable,
            "A115: with GTK_A11Y=test a widget has a test AT context whose properties stay \
             queryable",
        )?;
        recorder.axiom(
            !bus_present,
            "A115: with GTK_A11Y=test presenting the widget does not activate org.a11y.Bus",
        )
    })
}
