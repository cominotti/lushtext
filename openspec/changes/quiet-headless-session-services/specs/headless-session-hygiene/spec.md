## ADDED Requirements

### Requirement: One helper starts every private headless session
Every private headless test session SHALL be started through the session helper of `gtk-lush-proof-harness`. That covers widget, axiom, smoke, proof, skill and diagnostic sessions. Callers reach the helper through its library API or through `scripts/headless-session`. No other tracked code SHALL spawn `dbus-run-session`, `dbus-daemon`, `dbus-launch`, `mutter`, `pipewire`, `pipewire-pulse` or `wireplumber`.

#### Scenario: A lane needs a compositor and screenshots
- **WHEN** a screenshot lane starts a case
- **THEN** it runs `scripts/headless-session session --profile screencast -- <inner stage>`, and the inner stage runs `scripts/headless-session compositor --monitor WxH -- <child>`
- **AND** the lane's process handle for the compositor is mutter itself, as before

#### Scenario: A lane checks its prerequisites
- **WHEN** a lane checks whether the host can run it
- **THEN** it calls the helper's `check-tools` for its profile and reports the missing commands through its existing skip path
- **AND** it does not name the session commands in its own sources

#### Scenario: A compositor is requested outside a session
- **WHEN** `compositor` runs without the helper's session marker
- **THEN** it exits nonzero with a bounded diagnostic and starts nothing

### Requirement: Media daemons run quiet
With media on, the helper SHALL run WirePlumber with the `policy` profile, which has no hardware monitors. PipeWire, WirePlumber and every PipeWire client in the session SHALL load no `libpipewire-module-rt`. The daemons SHALL use helper-private config, state and data homes, and SHALL never read or write the user's WirePlumber state.

#### Scenario: Screen capture still works
- **WHEN** a lane captures a frame through mutter's ScreenCast and `pipewiresrc` in a `screencast` session
- **THEN** the frame is captured as before, which was measured at 640x480 with one buffer
- **AND** no thread of the session runs at SCHED_RR or SCHED_FIFO, and no process changed its nice level

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
- **THEN** it gets `ServiceUnknown`, and its two resulting warnings are classified as benign by exact text in the shared smoke classifiers and in the widget and axiom noise patterns

### Requirement: No accessibility bus or portals unless the profile asks
In a profile with a11y off, the helper SHALL set `GTK_A11Y=test` and `NO_AT_BRIDGE=1` before the session bus starts. A caller may set `GTK_A11Y=none` instead; any value that selects AT-SPI is refused. A profile with portals off SHALL also set `ADW_DISABLE_PORTAL=1`. The `accessibility` profile sets neither.

#### Scenario: A widget session runs the suite
- **WHEN** the widget suite runs in a `widget` session
- **THEN** at teardown the session bus has no `org.a11y.Bus`, `org.freedesktop.portal.Desktop` or `org.freedesktop.portal.Documents` name
- **AND** `gtk4::test_accessible_has_property`, `has_state` and `has_relation` still observe the state the tests set

#### Scenario: The accessibility smoke runs
- **WHEN** `make accessibility-smoke` runs in an `accessibility` session
- **THEN** AT-SPI is available to the app and the lane's assertions pass as before

### Requirement: Runtime guard fails a dirty session
At teardown, before stopping services, the helper SHALL evaluate the session's observations. Any violation fails the run. The helper prints `SESSION-HYGIENE: <class>: <detail>` and exits with a dedicated status, or keeps a failed child's status. The classes are `rtkit`, `bluez`, `hardware-device`, `module-rt`, `accessibility-bus`, `portal` and `config`.

#### Scenario: The bare configuration is caught
- **WHEN** the helper runs a `screencast` session with the previous configuration: no profile, module-rt loaded, and the host's state home
- **THEN** the run fails with the `rtkit`, `bluez`, `hardware-device` and `module-rt` classes
- **AND** the quiet configuration in the same scenario passes

#### Scenario: A toolkit update reactivates the accessibility bus
- **WHEN** a GTK, Libadwaita or portal update makes a session with a11y off activate `org.a11y.Bus`
- **THEN** that run fails with `SESSION-HYGIENE: accessibility-bus` and names the activated service

#### Scenario: Lanes report the class
- **WHEN** a lane's session fails the guard
- **THEN** the lane's manifest records `failure_status: session-hygiene`, and its runner prints the `SESSION-HYGIENE:` line

### Requirement: Static gate forbids session spawns outside the helper
`make check-policy` SHALL run `make check-session-hygiene`. It needs no build and fails on any tracked file outside the helper that spawns a session command, whatever its path. It also fails when the helper's quiet configuration drifts. The gate SHALL be self-tested failing-first, and SHALL fail if it finds no sanctioned spawn in the helper.

#### Scenario: A new lane under a new path spawns WirePlumber
- **WHEN** a tracked shell, Python, Rust, Makefile or workflow file at a path no list names runs `wireplumber`, `pipewire`, `dbus-run-session` or `mutter`, directly or through `exec`, `env`, `timeout`, `taskset` or `nohup`
- **THEN** `make check-session-hygiene` fails and names the file, the line and the command

#### Scenario: Benign mentions pass
- **WHEN** a file installs `pipewire` or `wireplumber` packages, uses a `case` label, names `pipewiresrc`, `pipewire.log` or `pipewire-gstreamer`, or names a command in a comment, docstring or Markdown
- **THEN** the gate reports nothing for it

#### Scenario: The helper's configuration drifts
- **WHEN** the helper's WirePlumber profile is `main` or enables a `hardware.*` feature, a PipeWire fragment lacks `module.rt = false`, the WirePlumber fragment lists `libpipewire-module-rt`, or the daemons' state home is the user's
- **THEN** the gate fails and names the drifted setting
