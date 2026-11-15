## Context

The measured baseline is in [`evidence/baseline.md`](./evidence/baseline.md).
Here is how each spawner starts its private headless session today.

| Kind | Sites |
| --- | --- |
| Rust | `gtk-lush-proof-harness` `run_under_headless_compositor` (`dbus-run-session -- mutter --headless … -- <self>`); `cargo-gtk-proof` `host.rs` (`dbus-run-session`) and `live.rs` (`pipewire`, `wireplumber`, `mutter`) |
| Shell | `run-widget-tests.sh`, `run-gtk-axioms.sh`, `run-portal-sandbox-smoke.sh`, `run-responsiveness-smoke.sh`, `capture-lushtext-xvfb.sh`, and require-checks in `run-visual-smoke.sh`, `run-accessibility-smoke.sh`, `run-command-palette-notes-smoke.sh`, `run-automation-smoke.sh`, `run-crash-recovery-smoke.sh` |
| Python | `visual-geometry-smoke.py`, `crash-recovery-smoke-driver.py`, `automation-smoke-driver.py`, `builder-diagnostics.py`, `editor-glyph-live-smoke.py`, `responsiveness-smoke-driver.py`, `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` (the capture engine behind `visual-smoke` and `accessibility-smoke`) |

Every screenshot lane starts a bare `wireplumber`, which is the `main` profile.
No lane gives WirePlumber a private state home. Each site sets its own
environment, and no site sets it before `dbus-run-session`, which is when the
D-Bus-activated services take their environment.

## Goals / Non-Goals

**Goals:** no host rtkit, bluetoothd, ALSA, v4l2 or libcamera use, and no write
to the host's WirePlumber state, from any test session. No `org.a11y.Bus`
outside AT sessions, and no portals in widget and axiom sessions. One owner
for all of this. A runtime failure when it happens anyway. A static failure
when code starts a session any other way.

**Non-goals:** see proposal.md. They are portals in the screenshot, AT and
sandbox lanes, a session-rate cap, and app behaviour.

## Decisions

### D1. The helper lives in `gtk-lush-proof-harness` and is the only implementation

The `session` module of `gtk-lush-proof-harness` becomes the one place that:

- starts `dbus-run-session`, the private system bus, `mutter --headless`,
  `pipewire` and `wireplumber`;
- writes their configuration;
- checks hygiene at teardown.

The crate ships the module's data files (`session/*.conf`) through
`include_str!` and gains a `[[bin]] gtk-lush-session`. Callers reach it three
ways:

- The widget harness and `cargo-gtk-proof` call the library.
- The axiom runner reaches it through the harness.
- Python, shell and skill lanes call `scripts/headless-session`. This shim
  contains no session command: it runs
  `cargo build --quiet --locked -p gtk-lush-proof-harness --bin gtk-lush-session`
  and then execs the built binary.

*Why here:* the harness already owns the private headless session, which is
its spec requirement "Harness launches an isolated headless GTK session". It
is a leaf crate, so it cannot call a LushText script, and a stock gtk-rs
adopter gets the same hygiene by default. One implementation means there is
nothing to keep in parity.

*Rejected:*

- **A Python helper in `scripts/`.** The proof harness would have to keep its
  own `dbus-run-session` path for generic adopters or depend on a repo script.
  The first is a second implementation; the second breaks the leaf contract.
- **Two sanctioned implementations (Rust and Python) with a parity assertion
  in the gate's self-test.** Parity checks catch only the drift they were
  written for, and the maintainer asked for one helper.
- **Per-language adapters around a shared config file.** The process
  supervision and the hygiene check would still be written three times.

The cost is that Python and shell lanes need the binary built. The shim does
this incrementally, it is small (glib, gtk4 and tempfile are already
dependencies), and every such lane already runs cargo.

### D2. CLI and library contract

Two subcommands share one option set. The library mirrors them with a builder.

- `gtk-lush-session session [--a11y off|atspi] [--media off|quiet] [--portals off|on] [--system-bus private|host] [--log-dir DIR] -- CMD…`
  - Starts `dbus-run-session`, whose environment is set *before* the bus
    starts (D5, D6).
  - Inside the bus, starts the private system bus and its monitor (D4), then
    the media daemons if requested.
  - Exports a session marker, then runs CMD.
  - After CMD exits and before teardown, runs the hygiene check (D7). Then it
    tears down in reverse order.
  - Exits with CMD's status. If the check failed, it exits with the hygiene
    status instead and prints `SESSION-HYGIENE: <class>: <detail>`.
- `gtk-lush-session compositor --monitor WxH [--monitor WxH…] -- CMD…`
  - Refuses to run outside a `session`.
  - Execs `mutter --headless --wayland --no-x11 --virtual-monitor …`, so a
    caller's process handle is mutter itself, as it is today.
  - Lanes that run steps between bus start and mutter (`apply_gsettings`, the
    AT registry) or several compositors per bus call it from their inner
    stage.
- `run` = `session` + `compositor` for the common case: the widget harness, the
  axioms, and simple lanes.
- `check-tools --profile …` prints the missing commands. Lanes call it instead
  of naming the commands in their own require checks.

Profiles are named option bundles, so a lane says what it is:

| Profile | a11y | media | portals |
| --- | --- | --- | --- |
| `widget` | off | off | off |
| `plain` | off | off | on |
| `screencast` | off | quiet | on |
| `accessibility` | atspi | quiet | on |

Every profile uses a private system bus. The one exception is the
portal-sandbox lane, which may need the host bus for `flatpak run`. If it
does, it says so with `--system-bus host` and a recorded reason, and media are
forbidden with `host`.

### D3. Quiet media configuration (measured)

- **WirePlumber profile: `policy`.** It inherits `base` plus
  `metadata.sm-settings`, `metadata.sm-objects` and `policy.standard`, and no
  `hardware.*` feature. A real mutter ScreenCast + `pipewiresrc` capture still
  links and produced a 640x480 PNG. `video-only` was rejected because it
  keeps `hardware.video-capture` (cameras).
- **PipeWire daemon and clients:** a `pipewire.conf.d` and a `client.conf.d`
  fragment, each `context.properties = { module.rt = false }`. Both files
  guard module-rt with `condition = [ { module.rt = !false } ]`. The client
  fragment matters because mutter's ScreenCast stream and `pipewiresrc` are
  PipeWire clients. Before it, a capture added 18 RealtimeKit1 messages (66
  against 48).
- **WirePlumber:** module-rt is unconditional twice, in `context.modules` and
  in the `client-context` component's `arguments.context.modules`.
  WirePlumber 0.5.19 loads `context.modules` itself and has no `condition`
  support. A `module.rt.args` fragment was measured to have **no effect**: the
  module still fell back to RTKit and set nice -11. So the fragment uses the
  two supported override hooks:
  - `override.context.modules = [protocol-native, metadata]`;
  - a `wireplumber.components.rules` entry that matches
    `name = "client-context"` and has an `override` of `arguments` listing the
    distribution's six client modules minus module-rt.

  Measured: no module-rt load, nice 0, no SCHED_RR thread. The cost is that
  the fragment repeats the distribution's module lists. A WirePlumber update
  that adds a module there would be dropped silently, but its absence would
  surface as a broken capture, and D7 checks that module-rt stays absent.
  This is recorded as an accepted maintenance point, re-read on WirePlumber
  updates (task 7.3).
- **Homes:** the daemons get helper-private `XDG_CONFIG_HOME`,
  `XDG_STATE_HOME` and `XDG_DATA_HOME`, so no WirePlumber state reaches
  `~/.local/state/wireplumber`. PipeWire clients read `client.conf.d` from the
  session's `XDG_CONFIG_HOME`. With media on, the helper requires that
  directory to be outside the real home's XDG trees, and refuses to start
  otherwise. It writes the client fragment there in its own file name.

### D4. A private, monitored system bus for the whole session

The helper starts a `dbus-daemon` on a private address, exports it as
`DBUS_SYSTEM_BUS_ADDRESS` to every process in the session, and records all its
traffic with `dbus-monitor`. rtkit and bluetoothd are system-bus services, so
no process in a test session can reach them, whatever its configuration. The
monitor turns any attempt into evidence for D7.

Measured:

- bare daemons: 48 RealtimeKit1 and 19 bluez messages;
- bare daemons plus a capture: 66 RealtimeKit1 and 19 bluez;
- quiet configuration: 0 and 0.

This detects drift: a PipeWire, WirePlumber, mutter or GTK update that starts
talking to rtkit or bluez shows up in the monitor log even though it reaches
nothing on the host.

*Scope (decided from the capture spike):* session-wide, not only the media
daemons, because the PipeWire clients (mutter, `pipewiresrc`) also load
module-rt. The cost is mutter's system-bus lookups. Under the private bus,
mutter asks for ColorManager, locale1, login1 and UPower, gets
`ServiceUnknown`, and logs two warnings with new text:

- `Failed to retrieve locale properties: …ServiceUnknown…`;
- `There is no colord server available`.

They join the shared benign classes in `scripts/smoke_warning_classifiers.py`
and the widget and axiom benign-noise patterns, as exact texts. CI already
runs with no system bus and already classifies the "Could not connect"
variants.

*Rejected:*

- **A nonexistent system-bus address.** It prevents host contact but records
  nothing, so drift would go unseen.
- **The host bus plus configuration alone.** This is today's model: one
  missed client config means real rtkit grants.

### D5. No accessibility bus outside AT sessions (identified, measured)

In a widget-like session, `org.a11y.Bus` is activated by:

1. the GTK 4 client itself, because GTK 4 ignores `NO_AT_BRIDGE`;
2. `xdg-desktop-portal-gtk`, which is GTK 3 with atk-bridge and honours
   `NO_AT_BRIDGE` only if the variable is in the bus's activation environment.

LushText's own `at_listeners` mirror also calls `org.a11y.Bus.GetAddress`
whenever `gtk_uses_atspi()` holds.

The fix sets `GTK_A11Y=test` and `NO_AT_BRIDGE=1` in the environment the
helper gives `dbus-run-session`, so every process sees them, activated
services included. `test` was chosen over `none` by measurement: `none`
creates no AT context, and `gtk4::test_accessible_has_property` and
`has_state` then return false, which breaks the 12 widget modules that assert
accessible state. `test` keeps a `GtkTestATContext` with full state and
contacts no bus.

The proof harness's `recommended_pre_gtk_environment()` gains `GTK_A11Y=test`
and `ADW_DISABLE_PORTAL=1`. The widget harness stops setting variables only in
its child (`configure_widget_test_environment`) and relies on the launcher.
Inside the child it now asserts the values instead of setting them.

*Rejected:*

- **A private service directory masking `org.a11y.Bus`.** Clients would still
  try, and GTK logs a warning per failed activation.
- **`GTK_A11Y=none`.** It breaks the accessible-state assertions.

### D6. Portals off in widget and axiom sessions only

Libadwaita starts `xdg-desktop-portal`, `xdg-desktop-portal-gtk`, the
permission store and the document portal (a FUSE mount in the private runtime
directory) despite `GDK_DEBUG=no-portals`. `ADW_DISABLE_PORTAL=1` stops that,
as measured and as the axiom runner already sets. The widget harness's
documented intent is "keep GTK from starting xdg-desktop-portal", so the
`widget` profile enforces it, and D7 checks it.

The screenshot, accessibility and portal-sandbox lanes keep portals. Their
baselines may depend on portal-provided settings, and portal-sandbox tests
portals. No host load was measured from them. Their artifacts list the
activated names, and the decision is recorded in the deferral list.

### D7. Runtime guard: `SESSION-HYGIENE:`

At teardown, while every service is still alive, the helper evaluates a pure
verdict function over bounded observations:

- monitor-log lines naming `org.freedesktop.RealtimeKit1` or `org.bluez`;
- with media on, `pw-dump` objects whose `device.api` is `alsa`, `v4l2`,
  `libcamera` or `bluez5`, and any `libpipewire-module-rt` load in the
  daemons' logs, read at a pinned `mod.rt` log topic;
- session `ListNames` containing `org.a11y.Bus` when a11y is `off`;
- `org.freedesktop.portal.Desktop` or `.Documents` when portals are `off`;
- a configuration self-check made at start: the profile is `policy`, the
  fragments are present, and the homes are private.

The check keys on `device.api`, not on a text search for `bluez`, because
restored metadata can name bluez devices. That happened in the spike that ran
with the real state home.

Each violation prints `SESSION-HYGIENE: <class>: <bounded detail>`. The process
exits with a dedicated status that the harness's existing statuses do not use.
A failed child keeps its own status, and the hygiene line is still printed.
These consumers report it as a named class:

- the widget harness relaunch;
- `run-widget-tests.sh`;
- `widget-shards.py parallel` (a `SESSION-HYGIENE:` summary line, failing like
  `OOM-KILL:`);
- `run-gtk-axioms.sh`;
- every lane manifest (`failure_status: session-hygiene`).

The verdict function is GTK-free and I/O-free, and unit tests cover every
class plus the clean case. The failing-first proof is runtime: before the
configuration lands, the new helper is run once with the bare configuration
and must fail with all four classes (task 2.6). An opt-in `--expect-dirty`
mode of the binary keeps that proof re-runnable by hand.

### D8. Static guard: `make check-session-hygiene`

`scripts/check-session-hygiene.py` runs with `--self-test` and then a real
scan, wired into `make check-policy`. It needs no build. The forbidden command
names are `dbus-run-session`, `dbus-daemon`, `dbus-launch`, `mutter`,
`pipewire`, `pipewire-pulse` and `wireplumber`.

- **What is scanned:** every tracked file (`git ls-files`), classified by kind,
  not by directory:
  - shell, by `.sh` or a `sh`/`bash` shebang, plus `.githooks/*`;
  - Makefile recipe lines;
  - workflow `run:` blocks;
  - Python (`ast`);
  - Rust (string literals, after stripping comments).

  Only `*.md`, `openspec/**`, the helper module and its data files, and the
  gate's own fixtures are excluded. A new lane under a new path is scanned the
  day it is added.
- **Shell rule:** a forbidden name, or a path ending in `/name`, in
  **command position**. That is the first word of a simple command, after
  `VAR=value` assignments and the wrappers `exec`, `env`, `nohup`, `setsid`,
  `timeout N`, `taskset …`, `stdbuf`, `nice`, `ionice`, `command`,
  `flatpak-spawn --host`, `systemd-run …` and `with-memory-cap.sh` /
  `with-temp-scope.sh … --`. Package lists (`dnf install … pipewire`), `case`
  labels, arguments and comments pass.
- **Python and Rust rule:** any string literal equal to a forbidden name or
  ending in `/name`. Docstrings and comments are excluded. Lanes do not name
  these commands at all after migration: their prerequisite checks call
  `check-tools`, and report strings name the helper. Literals such as
  `pipewiresrc`, `pipewire.log` and `pipewire-gstreamer` pass.
- **Drift rules:** these must hold in the helper's sources and data files:
  - the WirePlumber profile constant is `policy`;
  - the WirePlumber fragment contains no `libpipewire-module-rt` and contains
    both override hooks;
  - both PipeWire fragments set `module.rt = false`;
  - the daemons' state, config and data homes are helper-private;
  - every profile uses a private system bus unless it is marked
    `host-system-bus` with a reason.
- **Helper exemption:** keyed on the helper's path. This is fail-closed: if
  the helper is renamed without updating the gate, the helper becomes a
  violator and the gate turns red rather than silently passing. The gate also
  fails if it finds **zero** sanctioned spawns in the helper, so a gate that
  matches nothing cannot pass, as `testing.md`, "Path-keyed gates", requires.
- **Self-test, failing-first:** each fixture is a small file written to the
  run's temp scope:
  - wrapped shell spawns;
  - a Makefile recipe;
  - a workflow `run:` step;
  - a Python list literal;
  - a Rust `Command::new`;
  - a fixture at a never-listed new path, which must fail;
  - every benign form above, which must pass;
  - each drift mutation of a copy of the real data files, which must fail.

  The self-test is written and run red against the unmigrated tree before any
  lane migrates. That red scan's finding list is the migration worklist.

### D9. Migration scope: every site migrates

All sites in the Context table migrate in this change, including the two skill
scripts and every require check. No site is left with a pragma or allowlist
entry. The gate's red-first scan is the authoritative list. Each migrated lane
runs once on the final tree, or records an explicit host skip.

### D10. No session-rate cap

See proposal.md's Non-goals. The guard makes a session's host cost zero by
construction, so a cap would limit nothing harmful.

## Axioms relied on

- **New: GTK 4 with `GTK_A11Y=test`.** GTK creates a `GtkTestATContext`, keeps
  accessible properties, states and relations queryable through the
  `gtk_test_accessible_*` API, and sends nothing to `org.a11y.Bus`. With
  `none`, no context exists.
- **New: Libadwaita with `ADW_DISABLE_PORTAL=1`.** Libadwaita reads no
  settings portal, so nothing is activated on the session bus. Without it,
  `GDK_DEBUG=no-portals` does not stop Libadwaita from activating
  `org.freedesktop.portal.Desktop`.

Both get probes in `gtk-lush-axioms` that read the session bus's `ListNames`
from inside the probe, plus catalogue entries and samples. The ids are the
next free ones at apply time; the ledger is at A111 today. PipeWire and
WirePlumber facts (D3) are not GTK-family behaviour. The D7 runtime guard pins
them on every run, and the static gate pins their configuration.

## Risks / Trade-offs

- **`GTK_A11Y=test` changes `at_listeners::gtk_uses_atspi()` to false in
  widget tests.** The `background_tab_layout` tests simulate listeners. If a
  simulated answer no longer reads as listening, the fix belongs in the
  simulation hook (a test-utils override), not in the environment. The full
  widget suite runs (task 6.2).
- **`ADW_DISABLE_PORTAL=1` makes `AdwStyleManager` read GSettings instead of
  the portal** in widget tests. This is covered by the full suite run.
- **Mutter, GIO or the app may log more under the private system bus.** The
  new texts are classified exactly. Anything unclassified fails the lanes, as
  it should.
- **The WirePlumber fragment repeats module lists** (D3). This is an accepted
  maintenance point.
- **Proof fingerprints.** The visual and accessibility proofs fingerprint
  lane scripts, so those smokes run last, right before the commit (gate order
  in `testing.md`).

## Measurement

Timing: none (no timed path touched). App code is unchanged. The lanes'
per-case wall time is not a budgeted path. The host-load evidence is
before/after counts from the read-only system journal during one lane run
(task 6.5), not timings.

Kani: none (no proven module touched). The verdict function is set membership
over bounded observations, with no arithmetic and no state machine, and its
unit tests are exhaustive over the violation classes.

Budgets (`model/responsiveness_budget.rs`): unaffected. No exception.
