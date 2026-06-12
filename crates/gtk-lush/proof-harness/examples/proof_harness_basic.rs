// SPDX-License-Identifier: MIT OR Apache-2.0

use std::process::ExitCode;
use std::time::Duration;

use gtk_lush_proof_harness::{HarnessConfig, RegisteredTest, run_registered_tests, wait_until};

/// Child-test environment used only by this standalone adoption example.
const CHILD_TEST_ENV: &str = "GTK_LUSH_PROOF_EXAMPLE_CHILD";

/// Runner marker used only by this standalone adoption example.
const HEADLESS_RUNNER_ENV: &str = "GTK_LUSH_PROOF_EXAMPLE_HEADLESS";

/// Optional virtual-monitor override used by this standalone adoption example.
const HEADLESS_MONITOR_ENV: &str = "GTK_LUSH_PROOF_EXAMPLE_MONITOR";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = HarnessConfig::new(CHILD_TEST_ENV, HEADLESS_RUNNER_ENV, HEADLESS_MONITOR_ENV)
        .with_runner_label("GTK Lush proof harness example");
    let tests = [RegisteredTest::new("example::idle_wait", idle_wait)];

    run_registered_tests(&tests, &config, &args)
}

fn idle_wait() {
    let complete = std::rc::Rc::new(std::cell::Cell::new(false));
    glib::idle_add_local_once({
        let complete = std::rc::Rc::clone(&complete);
        move || complete.set(true)
    });
    wait_until(Duration::from_secs(1), || complete.get());
}
