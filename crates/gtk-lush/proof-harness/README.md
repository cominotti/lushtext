# gtk-lush-proof-harness

`gtk-lush-proof-harness` is a `0.0.0` GTK Lush family crate for reusable
headless GTK widget-test harness behavior.

## Pre-Publication Status

This is the first functional in-tree implementation. It is not a Phase 5 publication-ready
crate and is not a crates.io release candidate. The current API exists so
LushText can prove the extraction boundary before publishing any GTK Lush
crate.

Follow the roadmap in `docs/next/gtk-lush.md`.

## Scope

Use this crate for the generic mechanics around GTK widget tests:

- self-supervising relaunch into a private `dbus-run-session` +
  `mutter --headless` session;
- per-test child process isolation;
- bounded retry and loud flake reporting;
- list/filter/skip command-line behavior compatible with simple test harnesses;
- wait helpers that drain the GLib main loop correctly.

The crate does not initialize a consumer application, register GResources, own
the app's test registry, define a UI framework, or expose a state/message
system. Application setup remains caller-owned.

## Host Contract

The parent harness checks for `dbus-run-session` and `mutter` before launching
the private compositor. Missing host tooling exits with
`UNSUPPORTED_HOST_EXIT_CODE` (`77`) and an `UNSUPPORTED-HOST` diagnostic so
wrappers can distinguish environment support from a failing widget test. Normal
test failures keep the Rust test-harness-style `TEST_FAILURE_EXIT_CODE`
(`101`).

Consumers should apply `recommended_pre_gtk_environment()` before GTK
initialization when their startup is still single-threaded. The recommended
values are:

- `NO_AT_BRIDGE=1`
- `GDK_DEBUG=no-portals`
- `GTK_USE_PORTAL=0`
- `GSK_RENDERER=cairo`

`apply_headless_child_environment()` applies the outer relaunch side of the
contract before Mutter starts: `GDK_BACKEND=wayland`, the caller-owned headless
marker, and removal of inherited live `DISPLAY` and `WAYLAND_DISPLAY`.
Per-test children spawned inside the private Mutter session should inherit that
session's private Wayland display from the environment.

## Adoption Sketch

Register stable test names with `RegisteredTest::new`, then call
`run_registered_tests` from a custom test binary main. Keep application-specific
setup local to the consumer:

```rust
use std::process::ExitCode;

use gtk_lush_proof_harness::{HarnessConfig, RegisteredTest, run_registered_tests};

fn opens_window() {
    // Initialize GTK, register resources, construct widgets, and assert state.
}

fn main() -> ExitCode {
    let tests = [RegisteredTest::new("example::opens_window", opens_window)];
    let config = HarnessConfig::new(
        "MY_APP_WIDGET_CHILD",
        "MY_APP_WIDGET_HEADLESS_RUNNER",
        "MY_APP_WIDGET_HEADLESS_MONITOR",
    );
    run_registered_tests(&tests, &config, &std::env::args().skip(1).collect::<Vec<_>>())
}
```
