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

use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::test_isolation;

use gtk_lush_proof_harness::{
    HarnessConfig, RegisteredTest, recommended_pre_gtk_environment, run_registered_tests,
};

include!(concat!(env!("OUT_DIR"), "/widget_test_registry.rs"));

const CHILD_TEST_ENV: &str = "LUSHTEXT_WIDGET_CHILD";
const HEADLESS_RUNNER_ENV: &str = "LUSHTEXT_WIDGET_HEADLESS_RUNNER";
const HEADLESS_MONITOR_ENV: &str = "LUSHTEXT_WIDGET_HEADLESS_MONITOR";
const WIDGET_RUN_SCRATCH_PREFIX: &str = "lushtext-test-";
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
    // - `GTK_IM_MODULE=gtk-im-context-simple` keeps GTK off its Wayland
    //   input-method backend. Headless mutter advertises
    //   `zwp_text_input_manager_v3` with no input method behind it, and
    //   destroying a focused `GtkEntry` — every inline rename confirming — then
    //   races the compositor's reply into a SIGSEGV in `wl_proxy_get_version`.
    //   This one is a crash guard, not warning suppression; see
    //   `docs/next/persistent-format-hardening.md` S7B-6.
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

/// Give this process private app data before anything else runs.
///
/// Every widget-test process — the supervising parent and each child — points
/// `LUSHTEXT_DATA_DIR` and the four XDG base directories at a root inside its
/// own `TMPDIR`. For a child that is the per-attempt directory the harness
/// parent creates and removes, so the isolation exists before the registry
/// dispatches the test and cannot outlive it. Isolation used to be set by
/// `ensure_gtk_init()` alone, so a test that saved a session before
/// initializing GTK overwrote the developer's real `session.json`
/// (2026-10-20).
///
/// A child inherits the parent's values, which lie outside its own `TMPDIR`,
/// and re-roots them; a process whose variables already lie inside its
/// `TMPDIR` keeps them. A child also creates the directories, as a real home
/// has them (tests write fixtures straight into `LUSHTEXT_DATA_DIR`); the
/// parent creates none, so it leaves nothing in the lane's temp scope.
/// `json_store::data_dir()` refuses a real-home path in test builds as the
/// second line of defence.
///
/// A process outside the private headless session runs no test: it only lists
/// tests or relaunches itself under `mutter --headless`, and the compositor it
/// starts keeps the caller's environment, as before.
fn isolate_app_data_environment() {
    if std::env::var_os(HEADLESS_RUNNER_ENV).is_none() {
        return;
    }
    let temp_root = std::env::temp_dir();
    if test_isolation::environment_is_isolated_within(&temp_root) {
        return;
    }
    let root = temp_root.join(format!("{WIDGET_RUN_SCRATCH_PREFIX}{}", std::process::id()));
    let is_child = std::env::var_os(CHILD_TEST_ENV).is_some();
    for (key, value) in test_isolation::isolated_environment(&root) {
        if is_child {
            fixture::create_dir_all(&value);
        }
        // SAFETY: this runs from the first statement of `main`, before any
        // thread exists.
        unsafe { std::env::set_var(key, value) };
    }
}

fn main() -> ExitCode {
    isolate_app_data_environment();
    configure_widget_test_environment();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = HarnessConfig::new(CHILD_TEST_ENV, HEADLESS_RUNNER_ENV, HEADLESS_MONITOR_ENV)
        .with_default_headless_monitor(DEFAULT_HEADLESS_MONITOR)
        .with_runner_label("LushText widget tests")
        // Earlier builds leaked one `lushtext-test-<pid>` directory per child;
        // sharing that prefix lets the harness's startup sweep reclaim them.
        .with_run_scratch_prefix(WIDGET_RUN_SCRATCH_PREFIX);
    let tests: Vec<RegisteredTest> = all_widget_tests()
        .into_iter()
        .map(|(name, test_fn)| RegisteredTest::new(name, test_fn))
        .collect();

    run_registered_tests(&tests, &config, &args)
}
