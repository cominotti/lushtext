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
environment. Some set part of it before `dbus-run-session`, which is when the
D-Bus-activated services take their environment: `run-widget-tests.sh` exports
`NO_AT_BRIDGE`, `GDK_DEBUG` and `GTK_USE_PORTAL`, and the widget and axiom
binaries' `main` sets the recommended values before the library relaunch. None
sets `GTK_A11Y=test` or, for widget sessions, `ADW_DISABLE_PORTAL=1` there,
and that is the gap D5 and D6 close.

The table is the author's grep. The red scan of task 1.3 is the authoritative
list, and it also finds non-spawn mentions that D8 must resolve: version
queries (`responsiveness-smoke-driver.py`'s `["mutter", "--version"]` and its
`"mutter"` toolkit-report key, `run-responsiveness-smoke.sh`'s
`mutter --version`), capability and report data (`cargo-gtk-proof`
`artifacts.rs` test data, `lushtext-automation.py`'s `session_bus` value), the
harness's own tooling check and its tests in `proof-harness/src/lib.rs`, the
`for command_name in dbus-run-session mutter` loops of
`run-portal-sandbox-smoke.sh`, `setup-dev-tools.sh`'s `required_commands`, and
messages whose first word is a command name (`cargo-gtk-proof` `runner.rs`).

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
this incrementally, and the binary is small (glib, gtk4 and tempfile are
already dependencies). Not every lane runs cargo today: `portal-sandbox-smoke`
and the xvfb skill script do not. So the shim honours `GTK_LUSH_SESSION_BIN`,
a prebuilt binary path, and fails with the lane's unsupported-host status
when neither that nor cargo is available. The helper also needs
`dbus-monitor`, which Fedora ships in `dbus-tools`, not in `dbus-daemon`, the
only D-Bus package CI installs. Task 4.6 audits every install list.

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
  - The binary exports its own path as `GTK_LUSH_SESSION_BIN` into the
    session, so code inside it that needs a *nested* session (the new axioms'
    control runs, below) can start one without a Cargo dependency on the
    harness. A `session` started inside another is allowed and fully private:
    its own bus, system bus, runtime directory and guard.
- `gtk-lush-session compositor --monitor WxH [--monitor WxH…] -- CMD…`
  - Refuses to run outside a `session`.
  - Execs `mutter --headless --wayland --no-x11 --virtual-monitor …`, so a
    caller's process handle is mutter itself, as it is today.
  - Lanes that run steps between bus start and mutter (`apply_gsettings`, the
    AT registry) or several compositors per bus call it from their inner
    stage.
- `run` = `session` + `compositor` for the common case: the widget harness, the
  axioms, and simple lanes.
- `check-tools --profile … [--json]` prints the missing commands and, with
  `--json`, each session command's resolved path and `--version` line. Lanes
  call it instead of naming the commands in their own require checks, and
  lanes that record toolkit versions (the responsiveness manifest's
  `toolkit.mutter`, `run-responsiveness-smoke.sh`'s `mutter=` line) copy them
  from its JSON, so the static gate (D8) needs no exception for version
  queries.

Profiles are named option bundles, so a lane says what it is:

| Profile | a11y | media | portals |
| --- | --- | --- | --- |
| `widget` | off | off | off |
| `plain` | off | off | on |
| `screencast` | off | quiet | on |
| `accessibility` | atspi | quiet | on |

The responsiveness lane runs every variant in one session and sets each
app's environment per variant: `GTK_A11Y=none` and `NO_AT_BRIDGE=1` except
for the `at` variant, and always `ADW_DISABLE_PORTAL=1`. So it uses the
`accessibility` profile whenever the `at` variant is selected, which is the
default, and `plain` otherwise, **always with `--media off`**: the lane starts
no PipeWire or WirePlumber today, and a timing lane must not gain two daemons
(decided at spec review). Its app-side environment does not change.

Explicit options override a profile's values, and the session's artifacts
record the effective values, not only the profile name.

In a profile with a11y off, `GTK_A11Y` defaults to `test`. A caller may pass
`none` instead (the responsiveness lane's non-AT app processes do, per
process). Any other value, or an unset variable after the helper applied its
environment, is refused at start with a `config` verdict.

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

In portal-on sessions the `xdg-desktop-portal` frontend adds a third (spec
review spike, `evidence/baseline.md`), three times per session:

- `Failed to load RealtimeKit property: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.RealtimeKit1 was not provided by any .service files`.

These exact texts join the **existing** shared family "Headless container
session services" in `scripts/smoke_warning_classifiers.py`, which already
holds the no-system-bus "Could not connect" variants of the same services
(locale1, colord, the portal's RealtimeKit proxy) that CI produces. No
parallel family is added. Any further text the task 2.5 portal-on run shows is
added the same way, by exact text and with a sample line in the module's
self-test, and nothing broader.

The widget and axiom runners stop keeping their own copies of the mutter
system-bus lines and read the family from the module, as the
`smoke-warning-classification` requirement asks of a pattern shared by more
than one lane. The module's patterns use Python syntax (`(?:…)`), and the
runners feed `grep -E`, so the new `--print-regex <family>` entry point emits a
POSIX ERE rendering of the family, and the module's self-test runs `grep -E`
with that output over its benign and defect samples to prove the dialect. The
`cargo-gtk-proof` Rust scanner keeps its own copy under its existing
Python-oracle parity test. Copies are caught mechanically: the static gate
(D8) fails when a tracked file other than the module and that parity-tested
scanner contains one of the family's distinctive message texts.

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
`has_state` then return false, which breaks the twelve files that assert
accessible state (`evidence/baseline.md`). `test` keeps a `GtkTestATContext` with full state and
contacts no bus.

The proof harness's `recommended_pre_gtk_environment()` gains `GTK_A11Y=test`
and `ADW_DISABLE_PORTAL=1` (its fixed-size array return type changes, which
the API snapshot records). Today the widget and axiom binaries set the
recommended values in `main`, which covers the library relaunch but not
`run-widget-tests.sh`, whose `dbus-run-session` gets only three of them. After
the change the launcher (library or binary) applies them before the bus starts,
and the binaries stop setting them. The **per-test child** (the process that
runs one test or probe, `LUSHTEXT_WIDGET_CHILD` /
`GTK_LUSH_AXIOMS_PROBE_CHILD` set) verifies them before GTK initializes; list
mode and the relaunching parent do not, because they are outside the session
by design. A caller-set `GSK_RENDERER` still wins, and `GTK_A11Y=none` is
accepted as an a11y-off value (D2).

Mutter 51 owns `org.freedesktop.a11y.Manager` on the session bus (spike). That
is mutter's own interface, not the AT-SPI bus, so the guard keys on the exact
name `org.a11y.Bus`, never on an `a11y` substring.

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

- system-bus monitor records naming `org.freedesktop.RealtimeKit1` or
  `org.bluez`, each with its sender's executable, resolved while the sender is
  still connected (`GetConnectionUnixProcessID` on the private system bus,
  then `/proc/<pid>/exe`);
- with media on, `pw-dump` objects whose `device.api` is `alsa`, `v4l2`,
  `libcamera` or `bluez5`, and any `libpipewire-module-rt` load in the
  daemons' logs, read at a pinned `mod.rt` log topic;
- every name acquired on the session bus during the session (a
  `NameOwnerChanged` match started with the session), not only those still
  owned at teardown, so a service that exits early is still seen:
  `org.a11y.Bus` when a11y is `off`, and `org.freedesktop.portal.Desktop` or
  `.Documents` when portals are `off`;
- a configuration self-check made at start: the profile is `policy`, the
  fragments are present, the homes are private, and `GTK_A11Y` has an
  accepted value (D2).

**The `rtkit` and `bluez` classes (decided at spec review).** The
`xdg-desktop-portal` frontend reads rtkit's three properties at startup in
every portal-on session (`evidence/baseline.md`, review-stage spike: 18
messages, 3 of them `Properties.Get` to `org.freedesktop.RealtimeKit1`, no
`MakeThread*` call). Failing on any message that names rtkit would fail every
`plain`, `screencast` and `accessibility` session, so the classes are:

- `rtkit` fires on any `MakeThreadRealtime`, `MakeThreadRealtimeWithPID`,
  `MakeThreadHighPriority` or `MakeThreadHighPriorityWithPID` call, from any
  sender. These are the calls that consume the host's burst limit.
- `rtkit` also fires on any other message naming
  `org.freedesktop.RealtimeKit1` (a call to it, or an `AddMatch`,
  `GetNameOwner` or `StartServiceByName` naming it) unless its sender resolves
  to the `xdg-desktop-portal` executable and the profile has portals on. That
  one exception is recorded with its reason: on the host it is a property
  read that grants nothing and counts toward no limit, and portal-off profiles
  never start the frontend. It is bounded by shape as well as by sender: the
  lookups (`AddMatch`, `GetNameOwner`, `StartServiceByName`) and at most one
  `Properties.Get` of each of `MaxRealtimePriority`, `MinNiceLevel` and
  `RTTimeUSecMax` per frontend process. The frontend is also a PipeWire
  client, and a client-side module-rt load in it would repeat those reads, so
  any record beyond that set fires. A regressed PipeWire module-rt under the private
  bus never reaches a `MakeThread*` call (its property reads fail first), and
  this clause is what catches it.
- A sender that cannot be resolved, because it disconnected, gets no
  exception (fail closed).
- `bluez` fires on any message naming `org.bluez`, from any sender, with no
  exception.

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
class plus the clean case, the portal-frontend exception, and the cases that
must still fire next to it: the same lookup from another executable, a
`MakeThread*` call from the frontend, an unresolved sender, and a portal-off
profile. The failing-first proof is runtime: before the
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
  - Rust (string literals, after stripping comments);
  - `Containerfile`/`Dockerfile` `RUN`, `CMD` and `ENTRYPOINT` instructions, as
    shell.

  Only `*.md`, `openspec/**`, the helper module and its data files, the gate
  script itself (`scripts/check-session-hygiene.py`, whose forbidden-name list
  is a set of equal literals) and the gate's own temp-scope fixtures are
  excluded. A new lane under a new path is scanned the day it is added.
- **Shell rule (fail-closed, decided at spec review):** a word equal to a
  forbidden name, or a path ending in `/name`, **in any position**, fails. A
  command-position rule with a wrapper list was rejected: it is fail-open
  (every omitted wrapper, such as `scripts/heavy.sh … --`, `measure.sh`,
  `if`/`!`/`&&`, or a function like `require_command mutter`, would escape),
  and a line-based parser cannot tell a multi-line array element from a
  command. The exempt contexts are explicit and few:
  - comments;
  - a package-manager install command (`dnf`, `microdnf`, `yum`, `apt`,
    `apt-get`, `zypper`, `pacman` with `install`/`-S`) and its backslash
    continuation lines;
  - the elements of an array assigned to a variable whose name ends in
    `packages` (any case), the package-inventory convention
    `setup-dev-tools.sh` already follows;
  - a `case` pattern label;
  - text inside a quoted string, except a string passed to `sh -c`,
    `bash -c` or `eval`, whose words are scanned as shell;
  - a heredoc body (usage text, as in `run-widget-tests.sh`), except one fed
    to `sh`, `bash` or `eval`.

  So `taskset -c 0 pipewire`, `scripts/heavy.sh -- mutter`,
  `require_command mutter`, `for c in dbus-run-session mutter; do` and
  `sh -c 'wireplumber &'` fail, and `echo "running under mutter --headless"`
  passes.
- **Python and Rust rule:** a string literal fails when it equals a forbidden
  name, ends in `/name`, or has as its first whitespace-separated word such a
  name or path (`"mutter --headless"` for `shell=True`). Docstrings and
  comments are excluded. Messages such as `"missing required command: mutter"`
  pass. A message whose first word is a command name is reworded
  (`cargo-gtk-proof` `runner.rs`'s `"mutter child completed"` and
  `"mutter child failed"` become compositor-child wording; the
  `--mutter-child` flag itself passes). Lanes do not name these commands as
  data either: prerequisite checks
  and version records come from `check-tools` (D2), the `session_bus` report
  value names the helper, and test data that needs a command name (the
  `cargo-gtk-proof` missing-capability tests) takes it from the helper's
  exported constant or uses a name outside the list. Literals such as
  `pipewiresrc`, `pipewire.log` and `pipewire-gstreamer` pass.
- **Classifier copies:** any tracked file other than
  `scripts/smoke_warning_classifiers.py` and the parity-tested
  `cargo-gtk-proof` warning scanner that contains a distinctive message text
  of the shared session-service family (D4) fails, so a runner cannot regrow
  its own copy.
- **Drift rules:** these must hold in the helper's sources and data files:
  - the WirePlumber profile constant is `policy`;
  - the WirePlumber fragment contains no `libpipewire-module-rt` and contains
    both override hooks;
  - both PipeWire fragments set `module.rt = false`;
  - the daemons' state, config and data homes are helper-private;
  - every profile uses a private system bus unless it is marked
    `host-system-bus` with a reason.
- **Helper and gate exemptions:** keyed on their paths. This is fail-closed:
  if the helper or the gate is renamed without updating the gate, the renamed
  file becomes a violator and the gate turns red rather than silently passing.
  The gate also fails if it finds **zero** sanctioned spawns in the helper, so
  a gate that matches nothing cannot pass, as `testing.md`, "Path-keyed
  gates", requires; the gate joins that rule's list of path-keyed gates.
- **Self-test, failing-first:** each fixture is a small file written to the
  run's temp scope:
  - shell spawns, wrapped and unwrapped, and argument forms (a require-check
    function, a `for … in` list, `heavy.sh --`, `sh -c '…'`);
  - a Makefile recipe;
  - a workflow `run:` step;
  - a `Containerfile` `RUN` line;
  - a Python list literal and a multi-word `shell=True` string;
  - a Rust `Command::new`;
  - a runner that regrows a copy of a family text;
  - a fixture at a never-listed new path, which must fail;
  - every benign form above, which must pass, including a **multi-line**
    install list (a name alone on a backslash continuation line, as
    `ci.yml` has it) and a multi-line `packages=(…)` array;
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
next free ones at apply time; the ledger is at A111 today.

*Control runs (decided at spec review).* Each probe also needs a control that
shows the name **does** appear without the variable, or it could pass
vacuously, and that control must not dirty the axioms session's own guard.
`gtk-lush-axioms` is a leaf family crate (`check-gtk-lush-policy` forbids a
`gtk-lush-*` dependency) and the static gate forbids it to spawn a session,
so the probe runs its control through the helper binary that the enclosing
session exports as `GTK_LUSH_SESSION_BIN` (D2): a nested `run` with a11y on, or
with portals on, around a small control child. When the variable is unset (the
axiom runner started by `cargo test` directly, through the library) or a
control prerequisite is missing (`at-spi-bus-launcher`, `xdg-desktop-portal`),
the probe returns `FixtureInvalid`, which fails the lane, never `Holds`. `make
gtk-axioms` starts its session through the shim, so the variable is set. Both
axiom CI jobs install with weak dependencies off, so `at-spi2-core`,
`xdg-desktop-portal`, `xdg-desktop-portal-gtk` and `dbus-tools` are added to
their package lists (task 4.6). In an SDK sandbox (`make gtk-axioms-runtimes`)
the samples report the existing no-session-bus skip form. PipeWire and
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
- **Proof fingerprints.** The accessibility proof fingerprints the capture
  and atspi skill scripts, `run-accessibility-smoke.sh`, `run-visual-smoke.sh`,
  `smoke_warning_classifiers.py`, `docs/automation*.md`,
  `docs/end-user-coverage.md`, `crates/lushtext-core/src/ui/` and the widget
  tests, and `check-accessibility-policy` demands a fresh smoke for every
  commit that leaves one of them changed. `check-visual-proof-policy` does the
  same for its visual-sensitive set, which includes
  `crates/cargo-gtk-proof/src/`, `scripts/visual-geometry-smoke.py` and
  `scripts/lushtext-automation.py`. So every such edit lands in group 4, whose
  commit is preceded by the accessibility, visual and visual-geometry smokes,
  and groups 5–8 touch none of them (tasks.md).
- **The axiom runner moves from `GTK_A11Y=none` to `test`.** Every probe was
  verified under `none`. A test context per widget is a small, uniform cost,
  but a cost-ratio probe could move. Task 6.3 runs every probe, slow ones
  included; a probe whose verdict changes is investigated, not loosened, and if
  its cause is the test context the runner keeps `none` for that probe's child
  (D2 accepts it) with the reason recorded in the ledger row.

## Measurement

Timing: no budgeted path changes, and the responsiveness lane's per-variant
app environment is unchanged (D2). Apply added two application-code edits,
neither on a budgeted path: `ui::accessibility::announce` returns early for a
`GtkTestATContext` (A117), which no shipped session creates, and Protected
View's edit-entry capture connects `insert-text` and `delete-range`
after-handlers that only set a flag (`evidence/runtime-proof.md`, decisions 13
and 14). The lanes' per-case wall
time is not a budgeted path. The responsiveness lane's *session* does change:
a private system bus with its monitor (media stay off, as today), so an
app-side system-bus lookup on a timed path would fail fast instead of
succeeding. Because that is the measurement lane, the check is unconditional
(decided at spec review): task 6.4 runs an interleaved main-vs-branch quick-tier
measurement of the affected cases through `scripts/measure.sh`
(`verification-runs.md`; a verdict run needs a 1-minute load under 4), and
also checks the app stderr and probe logs for system-bus access. Any budget
shift is a blocker, investigated before merge, and this section records the
result. The host-load evidence is before/after counts from the read-only system journal during one lane run
(task 6.5), not timings.

Kani: none (no proven module touched). The verdict function is set membership
over bounded observations, with no arithmetic and no state machine, and its
unit tests are exhaustive over the violation classes.

Budgets (`model/responsiveness_budget.rs`): unaffected. No exception.

**Result (apply stage): no budget shift.** Four interleaved rounds through
`scripts/measure.sh`, `main` `14332d69` (baseline) against the branch, cases
`baseline/empty`, `baseline/small`, `keystroke/3500/*` and `dense/2000`,
variants `plain` and `at`. The verdict rounds are `main-2` → `branch-2` →
`main-3` → `branch-3`, each started at a 1-minute load under 4 (2.73 to
3.00). The first branch round started at 4.03, so it is non-verdict and the
pair was retried. The lane's side-labelled report (`merge-runs`, `report`)
flagged one stage: `dense/2000/wrap-off@plain` background
`post_open_install_slice`, branch median 12,570 µs against 2,782. That median
is of two reps, and one of them is a single 22,373 µs dispatch. In the other
reps the same dispatch is attributed to `validation`, as on `main`. Two more
`dense/2000` pairs, run as attribution rounds and non-verdict because three
of them started at a load of 4.02 to 4.54, took the merged report to four
reps a side, which flags no regression. The worst dispatch per rep is the same
on both sides (29.4 to 30.6 ms, `main` up to 39.3 ms), and so is the settle
time (289 to 306 ms). `post_open_publish` is bimodal on both sides: about
2 ms, or about 13 ms when a validation pass lands inside it. Across all 440
phase records a side, the median worst lateness is 3.7 ms on `main` and 4.1 ms
on the branch, under one 2 ms tick apart, and the maximum is 53.0 against
37.5 ms. The over-budget `dense/2000/wrap-off` validation dispatches, 22 to
39 ms, are on both sides. They predate this change and belong to the
long-line programme (C6 is "not met" on `main` too). The functional
quick-tier run passed, and the private system bus carried only bus
housekeeping, plus Mutter's start-up lookups of `locale1` and `colord`, which
fail fast there. The guard's verdict was `clean`. The host's `jetbrains-toolbox`
ran at about 150 % CPU through every round, on both sides alike. The runbook's
reference precondition asks for no process at 80 % or more, so this is
recorded as a deviation that both sides shared, not hidden.
(`evidence/runtime-proof.md` 6.4; raw runs in
`.claude/handoffs/quiet-headless-session-services/measure/`.)

## Stage 4 review decisions (2026-11-15)

User experience and robustness came first in each decision. The evidence is
in `evidence/runtime-proof.md`, "6.2 Stage 4".

1. **The two FLAKY lines.** Both are pre-existing on `main`. Under parallel
   stress, `main` failed a first attempt 44 times in 669 markdown runs and
   3 times in 667 workspace runs. The branch with the fixes had 0 in 559 and
   0 in 556 at a comparable load. Both causes are fixed here:
   - the preview widget never repaired its code blocks after its own width
     changed. In the app the shell masks most of this: it queues a repair
     after its transitions and when the side-by-side width clamp moves. What
     is left is a resize that changes the preview's width with no such
     transition, such as a window resize in preview-only mode (inferred from
     the code, not verified live);
   - the workspace test sampled a transient state.
   Task 6.2 is done.
2. **The 2,011 MiB `editor-page` peak.** Not reproduced: four parallel runs
   peaked at 384 to 393 MiB. Closed as not reproduced and unattributed. The
   shard keeps its 2 GiB scope, and an `OOM-KILL:` would still fail the run.
3. **A70 and A79 violated once at load ~25.** These are load-sensitive
   timing probes, not regressions: they hold at normal load, and this change
   does not touch what they measure. Recorded; the programme's open item on
   load-sensitive probes owns any recurrence.
4. **The A117 upstream GTK report.** It stays drafted
   (`docs/upstream-reports.md`). Filing it needs the maintainer's explicit
   approval.
5. **Responsiveness.** Accepted as no budget shift. The shared
   `jetbrains-toolbox` deviation is recorded above.
6. **`openspec validate --all --strict` on CLI 2.0.0.** This is
   pre-existing on `main`: 87 legacy specs fail. It is already a programme
   item, the OpenSpec strict legacy cleanup, not this change's.
   `openspec validate --all` and this change's strict validation pass.
7. **Task 8.1.** Written after the merge, with the merge SHA.
8. **The CST `sccache` server.** Belongs to CST and goes into the 8.1 note.
   The review added one more facet: started inside a `with-memory-cap.sh`
   scope, the server also stays in that scope. One started at 14:09 by a
   cargo-mutants measurement held a 36 GiB scope in `lushtext-heavy.slice`
   (48 GiB) all afternoon, and admission counted it, so a 16 GiB job waited.

Findings this stage fixed:

- **Session start could exec a replaced binary** (branch-only):
  - every `cargo build` re-uplifts `target/debug/gtk-lush-session`;
  - concurrent sessions then failed, 2 in 152 under a build loop;
  - the helper now re-executes itself through `/proc/<pid>/exe`, and the
    shim retries `exec`;
  - after the fix, 0 failures in 441 sessions.
- **No test can touch the live desktop:**
  - a per-test child refuses to run without `GTK_LUSH_SESSION_ACTIVE`, which
    only the helper's inner stage sets;
  - this adds to the existing guards: the relaunch through the helper with
    `DISPLAY` and `WAYLAND_DISPLAY` removed, the recommended-environment
    check, and the refusal of the real `XDG_RUNTIME_DIR`.
- **The AT-SPI registry critical**
  (`Unable to register the application … org.a11y.atspi.Registry: unit
  failed`):
  - this change removes the class from widget and screencast sessions,
    because `GTK_A11Y=test` contacts no AT-SPI bus;
  - accessibility sessions start `at-spi2-registryd` themselves;
  - the widget lane no longer allowlists the line, so it would now fail the
    lane;
  - the round-4 suite proves zero.
- **`cargo-gtk-proof` could retry a hygiene exit** as an AT-SPI start-up
  failure. It no longer does.

Gates and the merge are recorded in the handoff (`merge-record.md`).
