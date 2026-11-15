// SPDX-License-Identifier: MIT OR Apache-2.0

//! A116: with `ADW_DISABLE_PORTAL=1`, Libadwaita reads its settings without
//! the settings portal, so presenting a window activates nothing on the
//! session bus: `org.freedesktop.portal.Desktop` never appears. Without it,
//! `GDK_DEBUG=no-portals` and `GTK_USE_PORTAL=0` do not stop Libadwaita from
//! activating `org.freedesktop.portal.Desktop` at initialization.
//!
//! The probe presents a window in the axioms session (where the session
//! helper set `ADW_DISABLE_PORTAL=1`), reads the style manager's color scheme
//! (a setting the portal would answer), waits out an activation window and
//! reads `ListNames`. Its control runs the same client with portals allowed,
//! `ADW_DISABLE_PORTAL` unset and `GDK_DEBUG=no-portals` kept, in a nested
//! private session (`session_names`), and requires the portal to appear.

use crate::AxiomId;
use crate::observation::{Observation, Recorder};
use crate::probes::session_names::{
    PORTAL_DESKTOP_NAME, SESSION_BIN_ENV, control_prerequisite, data_dirs, labelled_fixture,
    name_present, present_and_wait, run_nested_control,
};

/// Probe A116. See the module documentation.
#[must_use]
pub fn probe_a116() -> Observation {
    Recorder::run(AxiomId::new(116), |recorder| {
        let disabled = std::env::var("ADW_DISABLE_PORTAL").unwrap_or_default();
        recorder.measure("adw_disable_portal", &disabled);
        recorder.control(
            disabled == "1",
            "control: the session sets ADW_DISABLE_PORTAL=1",
        )?;
        let helper = std::env::var_os(SESSION_BIN_ENV);
        let helper =
            match control_prerequisite(helper.as_deref(), PORTAL_DESKTOP_NAME, &data_dirs()) {
                Ok(helper) => helper.to_path_buf(),
                Err(missing) => return recorder.control(false, missing),
            };

        let label = labelled_fixture();
        let shown = present_and_wait(&label);
        let scheme = libadwaita::StyleManager::default().color_scheme();
        let portal_present = recorder.require(
            name_present(PORTAL_DESKTOP_NAME).ok(),
            "control: the probe reads the session bus's ListNames",
        )?;
        drop(shown);
        recorder.measure("color_scheme", format!("{scheme:?}"));
        recorder.measure("portal_with_variable", portal_present);

        let control = run_nested_control(
            &helper,
            AxiomId::new(116),
            PORTAL_DESKTOP_NAME,
            &["--portals", "on"],
        );
        if let Err(error) = &control {
            recorder.measure("control_error", error);
        }
        let control = recorder.require(
            control.ok(),
            "control: the nested control session ran and printed its result",
        )?;
        recorder.measure("portal_without_variable", control.appeared);
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
            "control: without ADW_DISABLE_PORTAL, GDK_DEBUG=no-portals does not stop Libadwaita \
             activating the settings portal",
        )?;
        recorder.axiom(
            !portal_present,
            "A116: with ADW_DISABLE_PORTAL=1 presenting a window does not activate the settings \
             portal",
        )
    })
}
