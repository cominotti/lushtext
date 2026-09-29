# gtk-lush-proof-harness

`gtk-lush-proof-harness` is a `0.0.0` GTK Lush family crate for reusable
headless GTK widget-test harness behavior.

## Internal Platform Status

This is a functional in-tree `0.0.0` implementation for LushText's internal
platform. It is not a stable external dependency and is not a crates.io release
candidate. The current API exists so LushText can keep proof harness behavior
small, local, and reviewable.

Follow the current posture in `docs/next/gtk-lush.md`. Baseline adoption
evidence for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

Use this crate for the generic mechanics around GTK widget tests:

- self-supervising relaunch into a private `dbus-run-session` +
  `mutter --headless` session;
- per-test child process isolation, including harness-owned per-test
  temporary directories;
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
- `GTK_IM_MODULE=gtk-im-context-simple`

Each one disables a desktop-session subsystem the private compositor advertises
but cannot provide. The input-method entry is the least obvious and the most
load-bearing: headless Mutter advertises `zwp_text_input_manager_v3`, so GTK
enables the Wayland text-input protocol for every focused editable, and
destroying a focused entry races the compositor's reply into a **SIGSEGV** inside
`wl_proxy_get_version`. Measured at 9 failures in 40 isolated runs before, 0 in
40 after. It is a statement about the environment, not a claim that the
underlying GTK race is fixed.

`apply_headless_child_environment()` applies the outer relaunch side of the
contract before Mutter starts: `GDK_BACKEND=wayland`, the caller-owned headless
marker, and removal of inherited live `DISPLAY` and `WAYLAND_DISPLAY`.
Per-test children spawned inside the private Mutter session should inherit that
session's private Wayland display from the environment.

## Temporary Files

Every selected test runs in its own child process, and the supervising parent
owns every child's temporary files. It creates one
`<prefix><pid>-<random>` root in the system temp dir per run (prefix from
`HarnessConfig::with_run_scratch_prefix`, default
`DEFAULT_RUN_SCRATCH_PREFIX`), passes each child attempt its own empty
subdirectory as `TMPDIR`, removes that subdirectory as soon as the child exits,
and removes the root when the run ends, whether tests passed or failed. Tests
keep calling `std::env::temp_dir()`, `tempfile`, or GLib's `g_get_tmp_dir()`
unchanged; a child that panics, aborts, or crashes cannot leak past its
attempt. Removal happens in the parent after the child exits rather than in the
child, because worker threads a test leaves behind may still be writing there.

A parent killed outright cannot clean up, so the next run sweeps entries named
`<prefix><pid>` or `<prefix><pid>-<suffix>` that are real directories owned by
the current user, whose PID no longer exists, and which have been idle for an
hour. The headless relaunch's runtime directory follows the same pattern and is
removed only once no process still uses it as `XDG_RUNTIME_DIR`:
`dbus-run-session` returns while `xdg-document-portal` still holds its FUSE
mount on `doc/`, so removing it at once fails and leaks it.

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
