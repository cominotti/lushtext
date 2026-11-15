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

The harness starts its private headless session through the `session`
module, the crate's one implementation of it (see "Session Hygiene" below).
Missing host tooling exits with `UNSUPPORTED_HOST_EXIT_CODE` (`77`) and an
`UNSUPPORTED-HOST` diagnostic so wrappers can distinguish environment support
from a failing widget test. Normal test failures keep the Rust
test-harness-style `TEST_FAILURE_EXIT_CODE` (`101`), and a session that failed
its runtime guard exits with `session::SESSION_HYGIENE_EXIT_CODE` (`93`).

The launcher applies `recommended_pre_gtk_environment()` to `dbus-run-session`
itself, **before the session bus starts**, so services the bus activates
(`xdg-desktop-portal-gtk`, the AT-SPI launcher) inherit it. A per-test child
only verifies the values (`verify_recommended_environment()`) and fails before
GTK initializes, naming the variable, when one is missing; list mode and the
relaunching parent verify nothing. The recommended values are:

- `NO_AT_BRIDGE=1`
- `GDK_DEBUG=no-portals`
- `GTK_USE_PORTAL=0`
- `GSK_RENDERER=cairo` (a caller's value wins)
- `GTK_IM_MODULE=gtk-im-context-simple`
- `GTK_A11Y=test` (a caller's `none` is accepted)
- `ADW_DISABLE_PORTAL=1`

Each one disables a desktop-session subsystem the private compositor advertises
but cannot provide. The input-method entry is the least obvious and the most
load-bearing: headless Mutter advertises `zwp_text_input_manager_v3`, so GTK
enables the Wayland text-input protocol for every focused editable, and
destroying a focused entry races the compositor's reply into a **SIGSEGV** inside
`wl_proxy_get_version`. Measured at 9 failures in 40 isolated runs before, 0 in
40 after. It is a statement about the environment, not a claim that the
underlying GTK race is fixed.

`GTK_A11Y=test` and `ADW_DISABLE_PORTAL=1` exist because GTK 4 ignores
`NO_AT_BRIDGE` and activates `org.a11y.Bus` from every client, and Libadwaita
starts `xdg-desktop-portal`, its GTK backend and the document portal despite
`GDK_DEBUG=no-portals`. `test`, not `none`: `none` creates no AT context, so
`gtk4::test_accessible_has_property` and `has_state` return false.

`apply_headless_child_environment()` applies the outer relaunch side of the
contract before Mutter starts: `GDK_BACKEND=wayland`, the caller-owned headless
marker, and removal of inherited live `DISPLAY` and `WAYLAND_DISPLAY`.
Per-test children spawned inside the private Mutter session should inherit that
session's private Wayland display from the environment.

## Session Hygiene

A test session must not load the developer's desktop. Before this module,
every screenshot case started a bare WirePlumber (the `main` profile, whose
ALSA, bluez, v4l2 and libcamera monitors opened the host's devices and
registered 15 endpoints with the host's bluetoothd), PipeWire's module-rt asked
the host's rtkit for priorities until its burst limit denied real audio, the
daemons rewrote the user's `~/.local/state/wireplumber`, and every widget
session activated `org.a11y.Bus` and the portals. `session` is the one place
that starts a session's processes, and it fails a run that does any of that.

**Profiles** (`session::profiles`), with explicit `--a11y`, `--media` and
`--portals` overriding them:

| Profile | a11y | media | portals | Used by |
| --- | --- | --- | --- | --- |
| `widget` | off | off | off | widget and axiom harnesses |
| `plain` | off | off | on | application lanes without screenshots |
| `screencast` | off | quiet | on | screenshot lanes |
| `accessibility` | atspi | quiet | on | AT-SPI lanes |

Every profile gets a **private, monitored system bus** (a `dbus-daemon` with
no service directory, exported as `DBUS_SYSTEM_BUS_ADDRESS`): rtkit and
bluetoothd cannot be reached, and `dbus-monitor` records every attempt.
**Quiet media** is PipeWire plus WirePlumber `--profile=policy` (no hardware
monitors), module-rt off in the daemons and in every client (the
`session/*.conf` fragments, compiled in), and helper-private config, state and
data homes. With media on, the session's `XDG_CONFIG_HOME` must be private.

**Runtime guard.** At teardown, while every service is alive, a pure verdict
(`session::verdict`) reads the observations and prints
`SESSION-HYGIENE: <class>: <detail>` for each violation:

- `rtkit`: any `MakeThread*` call, or any other message naming RealtimeKit1,
  except the `xdg-desktop-portal` frontend's startup lookups and its single
  read of each rtkit property in a portals-on session (resolved by PID and
  executable; an unresolved sender is not exempt);
- `bluez`: any message naming `org.bluez`;
- `hardware-device`: a PipeWire object whose `device.api` is `alsa`, `v4l2`,
  `libcamera` or `bluez5`;
- `module-rt`: `libpipewire-module-rt` mapped in a daemon or logged by it;
- `accessibility-bus`: `org.a11y.Bus` acquired (at any point) with a11y off;
- `portal`: a portal acquired with portals off;
- `config`: `GTK_A11Y` other than `test`/`none` with a11y off, or the quiet
  media configuration missing.

A violation exits with `93`, or keeps a failed command's own status. The
session's `session-hygiene.txt` (under `--log-dir`) records the effective
options, the names acquired and the verdict.

**Binary.** `gtk-lush-session` exposes the same helper:
`session [OPTIONS] -- CMD` (the command runs inside the bus and may start
`compositor` itself), `compositor --monitor WxH -- CMD` (execs
`mutter --headless`, so the caller's handle is mutter; refused outside a
session), `run` (both), and `check-tools [--profile P] [--json]` (missing
commands, and with `--json` each command's path and `--version` line). It
exports its own path as `GTK_LUSH_SESSION_BIN` inside the session, so code
there can start a nested, fully private session without depending on this
crate. `--expect-dirty` reruns the previous bare media configuration and
succeeds only when the guard catches it, for by-hand proof.

**Static gate.** LushText's `make check-session-hygiene` fails on any tracked
code outside this module that names a session command, and on drift of the
profile and the fragments.

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
