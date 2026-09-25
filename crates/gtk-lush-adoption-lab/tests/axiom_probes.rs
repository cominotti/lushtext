// SPDX-License-Identifier: GPL-3.0-or-later

//! The headless probe runner for `gtk-lush-axioms`.
//!
//! One test per catalogue entry that has a probe, each in its own child
//! process under a private `mutter --headless` session (the
//! `gtk-lush-proof-harness` contract), so a probe inherits the harness's
//! isolation and loud `FLAKY` reporting. Each test prints its observation as
//! one JSON line and fails unless the axiom holds. Adding an axiom to the
//! catalogue adds its test here with no edit.
//!
//! This binary lives in the adoption lab rather than in the axioms crate
//! because GTK Lush family crates are strict leaves: none may depend on the
//! harness, even as a dev-dependency. Run it with `make gtk-axioms`, never
//! against a live desktop.

use std::process::ExitCode;

use gtk_lush_axioms::{Verdict, catalogue, init_toolkit};
use gtk_lush_proof_harness::{
    HarnessConfig, RegisteredTest, recommended_pre_gtk_environment, run_registered_tests,
};

const CHILD_TEST_ENV: &str = "GTK_LUSH_AXIOMS_PROBE_CHILD";
const HEADLESS_RUNNER_ENV: &str = "GTK_LUSH_AXIOMS_PROBE_HEADLESS_RUNNER";
const HEADLESS_MONITOR_ENV: &str = "GTK_LUSH_AXIOMS_PROBE_HEADLESS_MONITOR";

fn configure_probe_environment() {
    // SAFETY: `main` runs this before GTK initializes and before any thread
    // is spawned; every later process is a fresh child that inherits it.
    unsafe {
        for setting in recommended_pre_gtk_environment() {
            if setting.key == "GSK_RENDERER" && std::env::var_os(setting.key).is_some() {
                continue;
            }
            std::env::set_var(setting.key, setting.value);
        }
        // GTK 4 registers with the AT-SPI registry regardless of
        // `NO_AT_BRIDGE`, and the private session has none, which GTK reports
        // as a `Gtk-CRITICAL`. The probes measure geometry, not accessibility.
        std::env::set_var("GTK_A11Y", "none");
        // Libadwaita reads the settings portal directly unless told not to;
        // the private session has none.
        std::env::set_var("ADW_DISABLE_PORTAL", "1");
    }
}

/// Run the probe the harness named in this child process.
fn run_named_probe() {
    let name = std::env::var(CHILD_TEST_ENV).expect("the harness names the probe to run");
    let axiom = catalogue()
        .iter()
        .find(|axiom| axiom.name == name)
        .expect("the named probe is catalogued");
    let probe = axiom.probe.expect("only probed axioms are registered");
    init_toolkit().expect("GTK and Libadwaita initialize in the headless session");
    let observation = probe();
    println!("{}", observation.to_json_line());
    assert_eq!(
        observation.verdict,
        Verdict::Holds,
        "{} ({}) did not hold: {}",
        axiom.id,
        axiom.statement,
        observation.to_json_line()
    );
}

fn main() -> ExitCode {
    configure_probe_environment();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = HarnessConfig::new(CHILD_TEST_ENV, HEADLESS_RUNNER_ENV, HEADLESS_MONITOR_ENV)
        .with_runner_label("GTK axiom probes");
    let tests: Vec<RegisteredTest> = catalogue()
        .iter()
        .filter(|axiom| axiom.probe.is_some())
        .map(|axiom| RegisteredTest::new(axiom.name, run_named_probe))
        .collect();
    run_registered_tests(&tests, &config, &args)
}
