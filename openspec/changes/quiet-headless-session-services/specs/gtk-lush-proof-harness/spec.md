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
`recommended_pre_gtk_environment()` SHALL include `GTK_A11Y=test` and `ADW_DISABLE_PORTAL=1` beside its current entries. The helper SHALL apply the recommended environment to `dbus-run-session` itself, so D-Bus-activated services inherit it. A child process SHALL only verify those values and SHALL NOT set them.

#### Scenario: An activated portal backend inherits the environment
- **WHEN** a GTK 3 service such as `xdg-desktop-portal-gtk` is activated inside a session with a11y off
- **THEN** it sees `NO_AT_BRIDGE=1` and does not activate `org.a11y.Bus`

#### Scenario: A child finds the environment missing
- **WHEN** a widget-test child starts without the recommended values
- **THEN** it fails before GTK initializes, with a diagnostic that names the missing variable
