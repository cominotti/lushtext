// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A116: a plain Libadwaita window. The readout shows
//! `ADW_DISABLE_PORTAL`, `GDK_DEBUG`, the style manager's color scheme, and
//! whether `org.freedesktop.portal.Desktop` is on the session bus. Run it
//! with `ADW_DISABLE_PORTAL=1` and without to compare. `--check` runs the
//! probe, whose control runs in a nested private session.

mod support;

use gtk_lush_axioms::{AxiomId, PORTAL_DESKTOP_NAME, session_bus_has_name};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some(
        "Libadwaita initialized for this window; with ADW_DISABLE_PORTAL unset it asked the \
         settings portal for the color scheme.",
    ));
    label.set_wrap(true);
    ui.set_fixture(&label);
    // Libadwaita follows the portal's color scheme only while it uses the
    // portal; forcing a scheme shows the style manager answering locally.
    ui.add_control("Follow the system scheme", || {
        libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::Default);
    });
    ui.add_control("Force dark", || {
        libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    });
    ui.set_readout(|| {
        let bus = session_bus_has_name(PORTAL_DESKTOP_NAME)
            .map_or_else(|error| format!("unknown ({error})"), |present| present.to_string());
        let variable = |key: &str| std::env::var(key).unwrap_or_else(|_| "(unset)".to_owned());
        format!(
            "ADW_DISABLE_PORTAL={}; GDK_DEBUG={}; color scheme: {:?}; {PORTAL_DESKTOP_NAME} on the bus: {bus}",
            variable("ADW_DISABLE_PORTAL"),
            variable("GDK_DEBUG"),
            libadwaita::StyleManager::default().color_scheme(),
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(116), build)
}
