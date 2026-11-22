# headless-session-hygiene Specification

## Purpose
Keep every private headless session (widget, axiom, smoke, proof, skill and diagnostic) inside one session helper, so that no test lane reaches the host system bus, rtkit, bluez, real devices, the user's WirePlumber state or an AT-SPI bus it did not ask for, and so that a violation fails the session instead of passing silently.

## Requirements

### Requirement: One helper starts every private headless session
Every private headless test session SHALL be started through the session helper of `gtk-lush-proof-harness`. That covers widget, axiom, smoke, proof, skill and diagnostic sessions. Callers reach the helper through its library API or through `scripts/headless-session`. No other tracked code SHALL spawn `dbus-run-session`, `dbus-daemon`, `dbus-launch`, `mutter`, `pipewire`, `pipewire-pulse` or `wireplumber`.

#### Scenario: A lane needs a compositor and screenshots
- **WHEN** a screenshot lane starts a case
- **THEN** it runs `scripts/headless-session session --profile screencast -- <inner stage>`, and the inner stage runs `scripts/headless-session compositor --monitor WxH -- <child>`
- **AND** the lane's process handle for the compositor is mutter itself, as before

#### Scenario: A lane checks its prerequisites
- **WHEN** a lane checks whether the host can run it, or records the versions of the session tools
- **THEN** it calls the helper's `check-tools` for its profile and reports the missing commands through its existing skip path, and takes any version it records from `check-tools --json`
- **AND** it does not name the session commands in its own sources

#### Scenario: A compositor is requested outside a session
- **WHEN** `compositor` runs without the helper's session marker
- **THEN** it exits nonzero with a bounded diagnostic and starts nothing

### Requirement: Media daemons run quiet
With media on, the helper SHALL run WirePlumber with the `policy` profile, which has no hardware monitors. PipeWire, WirePlumber and every PipeWire client in the session SHALL load no `libpipewire-module-rt`. The daemons SHALL use helper-private config, state and data homes, and SHALL never read or write the user's WirePlumber state.

#### Scenario: Screen capture still works
- **WHEN** a lane captures a frame through mutter's ScreenCast and `pipewiresrc` in a `screencast` session
- **THEN** the frame is captured as before, which was measured at 640x480 with one buffer
- **AND** no thread of the session runs at SCHED_RR or SCHED_FIFO, and no process raised its priority

#### Scenario: Host devices stay untouched
- **WHEN** a `screencast` session's services are running
- **THEN** `pw-dump` lists no object whose `device.api` is `alsa`, `v4l2`, `libcamera` or `bluez5`
- **AND** `~/.local/state/wireplumber` is not opened by any session process

#### Scenario: Client configuration needs a private config home
- **WHEN** media are on and the session's `XDG_CONFIG_HOME` is unset or inside the real home's XDG trees
- **THEN** the helper refuses to start and prints a bounded diagnostic

### Requirement: Sessions reach no host system service
The helper SHALL give every session a private, monitored system bus that is exported as `DBUS_SYSTEM_BUS_ADDRESS` to every process in the session. A profile may use the host system bus only when it is marked for that with a recorded reason. Such a profile SHALL NOT start media.

#### Scenario: A process asks for rtkit or bluez
- **WHEN** any session process sends a message naming `org.freedesktop.RealtimeKit1` or `org.bluez`
- **THEN** the message reaches no host service, because the bus has no such name
- **AND** the monitor log records it for the runtime guard

#### Scenario: Mutter's system-bus lookups
- **WHEN** mutter asks the private bus for locale1, ColorManager, login1 or UPower
- **THEN** it gets `ServiceUnknown`, and its two resulting warnings are classified as benign by exact text, defined once in the shared smoke classifier module, which the widget and axiom runners read instead of keeping copies

### Requirement: No accessibility bus or portals unless the profile asks
In a profile with a11y off, the helper SHALL set `GTK_A11Y=test` and `NO_AT_BRIDGE=1` before the session bus starts. A caller may set `GTK_A11Y=none` instead; any other value is refused with a `config` verdict. A profile with portals off SHALL also set `ADW_DISABLE_PORTAL=1`. The `accessibility` profile sets neither.

#### Scenario: A widget session runs the suite
- **WHEN** the widget suite runs in a `widget` session
- **THEN** no `org.a11y.Bus`, `org.freedesktop.portal.Desktop` or `org.freedesktop.portal.Documents` name was acquired on the session bus at any point of the session, which the guard reads from a `NameOwnerChanged` record and not only from the names left at teardown
- **AND** `gtk4::test_accessible_has_property`, `has_state` and `has_relation` still observe the state the tests set

#### Scenario: The accessibility smoke runs
- **WHEN** `make accessibility-smoke` runs in an `accessibility` session
- **THEN** AT-SPI is available to the app and the lane's assertions pass as before

### Requirement: Runtime guard fails a dirty session
At teardown, before stopping services, the helper SHALL evaluate the session's observations. Any violation fails the run. The helper prints `SESSION-HYGIENE: <class>: <detail>` and exits with a dedicated status, or keeps a failed child's status. The classes are `rtkit`, `bluez`, `hardware-device`, `module-rt`, `accessibility-bus`, `portal` and `config`.

#### Scenario: The bare configuration is caught
- **WHEN** the helper runs a `screencast` session with the previous configuration: no profile and no fragments, so module-rt loads, with a scratch state home that is never the user's
- **THEN** the run fails with the `rtkit`, `bluez`, `hardware-device` and `module-rt` classes
- **AND** the quiet configuration in the same scenario passes

#### Scenario: A toolkit update reactivates the accessibility bus
- **WHEN** a GTK, Libadwaita or portal update makes a session with a11y off activate `org.a11y.Bus`
- **THEN** that run fails with `SESSION-HYGIENE: accessibility-bus` and names the activated service

#### Scenario: Lanes report the class
- **WHEN** a lane's session fails the guard
- **THEN** the lane's manifest records `failure_status: session-hygiene`, and its runner prints the `SESSION-HYGIENE:` line

### Requirement: Static gate forbids session spawns outside the helper
`make check-policy` SHALL run `make check-session-hygiene`. It needs no build and fails on any tracked file outside the helper and the gate itself that names a session command in a shell, Makefile, workflow, container-build, Python or Rust context, whatever its path, in any position except the explicit benign contexts. It also fails when the helper's quiet configuration drifts. The gate SHALL be self-tested failing-first, and SHALL fail if it finds no sanctioned spawn in the helper.

#### Scenario: A new lane under a new path spawns WirePlumber
- **WHEN** a tracked shell, Python, Rust, Makefile, workflow or container-build file at a path no list names runs `wireplumber`, `pipewire`, `dbus-run-session` or `mutter`, directly, through any wrapper (`exec`, `env`, `timeout`, `taskset`, `nohup`, `scripts/heavy.sh --`, `sh -c`), or names one as an argument such as a prerequisite check or a `for` list
- **THEN** `make check-session-hygiene` fails and names the file, the line and the command

#### Scenario: Benign mentions pass
- **WHEN** a file installs `pipewire` or `wireplumber` packages (on one line or on backslash continuation lines), lists them in an array whose name ends in `packages`, uses a `case` label, names `pipewiresrc`, `pipewire.log` or `pipewire-gstreamer`, or names a command in a comment, docstring, message string or Markdown
- **THEN** the gate reports nothing for it

#### Scenario: The helper's configuration drifts
- **WHEN** the helper's WirePlumber profile is `main` or enables a `hardware.*` feature, a PipeWire fragment lacks `module.rt = false`, the WirePlumber fragment lists `libpipewire-module-rt`, or the daemons' state home is the user's
- **THEN** the gate fails and names the drifted setting

### Requirement: The rtkit and bluez classes key on what can reach the services
The `rtkit` class SHALL fire on any `MakeThread*` call to RealtimeKit1 from any sender, and on any other message naming RealtimeKit1 unless its sender resolves to the `xdg-desktop-portal` executable in a profile with portals on and the message is one of that frontend's startup lookups or its single read of each rtkit property. An unresolved sender, or a frontend record beyond that set, SHALL get no exception. The `bluez` class SHALL fire on any message naming `org.bluez`.

#### Scenario: The portal frontend reads rtkit's properties
- **WHEN** a session with portals on activates `xdg-desktop-portal`, which looks up `org.freedesktop.RealtimeKit1` and reads its properties at startup
- **THEN** the lookups and reads get `ServiceUnknown` from the private bus, the guard reports no `rtkit` violation for them, and the frontend's resulting `Failed to load RealtimeKit property` warning is classified as benign by exact text in the same shared family
- **AND** the same lookup from any other executable, or any `MakeThread*` call from any sender, still fails the run with `rtkit`

### Requirement: Static gate forbids copies of the shared session-warning texts
`make check-session-hygiene` SHALL fail when a tracked file other than the shared classifier module and the parity-tested proof-tool warning scanner contains one of the shared session-service family's message texts.

#### Scenario: A runner regrows a copy of a shared warning text
- **WHEN** `run-widget-tests.sh`, `run-gtk-axioms.sh` or any other tracked file outside the shared classifier module and the parity-tested scanner contains one of the session-service family's message texts
- **THEN** the gate fails and names the file and the text

### Requirement: Quiet media are ready before the command starts
With media on, the helper SHALL start the session's command only once WirePlumber's client object, read from `pw-dump`, lists `policy.linking.standard` as a member of its `session.services` property. A pure function of `session::observe` SHALL decide this from the dump text. The helper SHALL NOT wait for an object that `pw-dump` may omit, such as a metadata object without entries.

#### Scenario: WirePlumber's policy is loaded
- **WHEN** a quiet `screencast` session starts and WirePlumber's client lists `policy.linking.standard` in `session.services`
- **THEN** the command starts at the next poll, not after `READY_TIMEOUT`
- **AND** a ScreenCast capture through `pipewiresrc` links and yields its first buffer on its first try

#### Scenario: Member order does not matter
- **WHEN** `session.services` lists `policy.linking.standard` at any position of its bracketed list
- **THEN** the readiness function reports ready
- **AND** a member that only starts with `policy.linking.standard`, or a dump without a `session.services` value, reports not ready

#### Scenario: The bare configuration still reaches its verdict
- **WHEN** the helper runs with `--expect-dirty`, which starts WirePlumber's `main` profile
- **THEN** readiness is decided by the same signal, and the mode reaches the runtime guard's verdict with all four classes (`rtkit`, `bluez`, `hardware-device`, `module-rt`) reported

### Requirement: A media readiness miss fails the session
If quiet media are not ready within `READY_TIMEOUT`, the helper SHALL NOT start the command. It SHALL save the last successful dump, if any, to the log directory as `pw-dump-not-ready.json`, print one bounded line naming WirePlumber, `session.services` and `policy.linking.standard` that says where the dump was kept, that it could not be saved there, that no `pw-dump` succeeded, or that `--log-dir` keeps it, and exit with `TEST_FAILURE_EXIT_CODE`.

#### Scenario: WirePlumber never loads its linking policy
- **WHEN** WirePlumber's `policy` profile is configured without `policy.standard`, or `session.services` is never set, in a session given `--log-dir`
- **THEN** after `READY_TIMEOUT` the helper exits `TEST_FAILURE_EXIT_CODE` without starting the command
- **AND** the log directory holds `pw-dump-not-ready.json` and the session's daemons are stopped
- **AND** the exit status is neither the unsupported-host code (77) nor `SESSION_HYGIENE_EXIT_CODE` (93)

### Requirement: Lanes fail on a WirePlumber readiness miss
A smoke lane SHALL report a case whose session failed because WirePlumber did not load its linking policy as failed. No lane SHALL classify that failure as a host-support skip.

#### Scenario: A WirePlumber readiness miss fails the lane
- **WHEN** a smoke case's session fails because WirePlumber did not load its linking policy
- **THEN** the lane reports the case as failed and exits nonzero
- **AND** the lane prints no `SKIP:` line for that case

### Requirement: The exported helper path survives a rebuild
Inside a session, the `GTK_LUSH_SESSION_BIN` the helper exports SHALL name the helper's file path without the ` (deleted)` suffix that Linux gives a replaced executable, `scripts/headless-session` SHALL retry briefly before it reports that path as not executable, and the GTK axiom probes that spawn that path for a nested control run (A115, A116) SHALL retry the spawn the same way while the path is missing or busy.

#### Scenario: A rebuild replaces the helper while a session runs
- **WHEN** `target/debug/gtk-lush-session` is removed and linked again while a session runs, and code inside the session starts a nested `scripts/headless-session` stage
- **THEN** the exported path names `target/debug/gtk-lush-session`
- **AND** the nested stage starts instead of exiting as an unsupported host

#### Scenario: A rebuild relinks the helper while an axiom probe starts its control
- **WHEN** the A115 or A116 probe spawns `GTK_LUSH_SESSION_BIN` for its nested control run while a `cargo build` has removed the helper and not yet linked it again
- **THEN** the probe retries the spawn briefly and its control run starts once the helper is linked
- **AND** any spawn error other than a missing or busy executable is reported at once
