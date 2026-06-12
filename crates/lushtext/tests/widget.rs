// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for LushText GTK4 UI components.
//!
//! These tests always run under a private headless Mutter compositor. Plain
//! `cargo test` is still allowed, but the harness re-launches itself into that
//! isolated compositor before any GTK widget can be shown on the live desktop.
//! They verify widget construction, property behavior, and signal wiring.
//!
//! GTK widgets must remain on one stable thread for the lifetime of the test
//! process, so this test target uses a custom single-threaded harness instead
//! of Rust's default libtest runner.

use std::process::ExitCode;

use gtk_lush_proof_harness::{
    HarnessConfig, RegisteredTest, recommended_pre_gtk_environment, run_registered_tests,
};

include!(concat!(env!("OUT_DIR"), "/widget_test_registry.rs"));

const CHILD_TEST_ENV: &str = "LUSHTEXT_WIDGET_CHILD";
const HEADLESS_RUNNER_ENV: &str = "LUSHTEXT_WIDGET_HEADLESS_RUNNER";
const HEADLESS_MONITOR_ENV: &str = "LUSHTEXT_WIDGET_HEADLESS_MONITOR";
const DEFAULT_HEADLESS_MONITOR: &str = gtk_lush_proof_harness::DEFAULT_HEADLESS_MONITOR;

fn configure_widget_test_environment() {
    // Widget tests need deterministic process-wide backends before any GTK
    // initialization path runs.
    //
    // - `NO_AT_BRIDGE=1` disables the accessibility bus, which is absent in
    //   the headless mutter session and otherwise emits AT-SPI warnings.
    // - `GDK_DEBUG=no-portals` and `GTK_USE_PORTAL=0` keep GTK from starting
    //   xdg-desktop-portal just to discover headless settings.
    // - `GSK_RENDERER=cairo` keeps GTK on the CPU fallback renderer so
    //   headless containers do not probe missing Mesa/EGL devices. The runner
    //   can still override this when a renderer-specific bug is being chased.
    // SAFETY: the widget harness sets these variables before any test code
    // initializes GTK, and they stay local to the per-test child process.
    unsafe {
        for setting in recommended_pre_gtk_environment() {
            if setting.key == "GSK_RENDERER" && std::env::var_os(setting.key).is_some() {
                continue;
            }
            std::env::set_var(setting.key, setting.value);
        }
    }
}

fn main() -> ExitCode {
    configure_widget_test_environment();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = HarnessConfig::new(CHILD_TEST_ENV, HEADLESS_RUNNER_ENV, HEADLESS_MONITOR_ENV)
        .with_default_headless_monitor(DEFAULT_HEADLESS_MONITOR)
        .with_runner_label("LushText widget tests");
    let tests: Vec<RegisteredTest> = all_widget_tests()
        .into_iter()
        .map(|(name, test_fn)| RegisteredTest::new(name, test_fn))
        .collect();

    run_registered_tests(&tests, &config, &args)
}
