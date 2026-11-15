## ADDED Requirements

### Requirement: Harness owns the session hygiene helper
`gtk-lush-proof-harness` SHALL contain the only implementation of the private headless session: bus, private system bus, compositor, quiet media, environment and runtime guard. It exposes that implementation as a library module and as a `gtk-lush-session` binary. The crate SHALL stay a leaf: its data files are compiled in, and it depends on no LushText script.

#### Scenario: The widget harness relaunches
- **WHEN** a widget-test binary built on the harness runs outside a private session
- **THEN** it relaunches itself through the library's `widget` profile rather than through a private `dbus-run-session` command line
- **AND** a `SESSION-HYGIENE:` verdict makes the run exit with the dedicated status after the child's own results are printed

#### Scenario: A stock gtk-rs adopter
- **WHEN** an application outside LushText registers tests with the harness
- **THEN** its sessions get the same quiet environment and runtime guard, with no LushText file required

### Requirement: Recommended environment is set before the session bus starts
`recommended_pre_gtk_environment()` SHALL include `GTK_A11Y=test` and `ADW_DISABLE_PORTAL=1` beside its current entries. The helper SHALL apply the recommended environment to `dbus-run-session` itself, so D-Bus-activated services inherit it. The per-test child (the process that runs one test or probe) SHALL only verify those values and SHALL NOT set them; list mode and the relaunching parent, which run outside the session, verify nothing.

#### Scenario: An activated portal backend inherits the environment
- **WHEN** a GTK 3 service such as `xdg-desktop-portal-gtk` is activated inside a session with a11y off
- **THEN** it sees `NO_AT_BRIDGE=1` and does not activate `org.a11y.Bus`

#### Scenario: A child finds the environment missing
- **WHEN** a widget-test child starts without the recommended values
- **THEN** it fails before GTK initializes, with a diagnostic that names the missing variable

#### Scenario: List mode outside a session
- **WHEN** a widget-test binary runs with `--list` outside any session
- **THEN** it lists the tests without verifying the recommended environment and without starting a session

#### Scenario: A caller overrides the renderer or the a11y value
- **WHEN** a caller sets `GSK_RENDERER`, or sets `GTK_A11Y=none` in place of `test`
- **THEN** the launcher keeps the caller's value, and the per-test child accepts it

### Requirement: The session binary supports nested sessions
The `gtk-lush-session` binary SHALL export its own path as `GTK_LUSH_SESSION_BIN` into every session it starts, so code inside a session can start a nested, fully private session without a Cargo dependency on the crate.

#### Scenario: A probe needs a control session
- **WHEN** code inside a helper session runs `$GTK_LUSH_SESSION_BIN run` with different options
- **THEN** the nested session gets its own bus, system bus, runtime directory and guard, and the outer session's guard sees none of its names or messages
