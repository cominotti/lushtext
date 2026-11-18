## ADDED Requirements

### Requirement: Headless no-AT and no-portal environment are ledger axioms
The ledger SHALL gain two entries with new stable ids. First, with `GTK_A11Y=test`, GTK 4 gives a widget a test AT context, keeps its properties, states and relations queryable, and sends nothing to `org.a11y.Bus`, while `none` gives it no context. Second, with `ADW_DISABLE_PORTAL=1`, Libadwaita activates no portal. Each entry has a probe, a catalogue entry and a sample.

#### Scenario: The test backend keeps state without a bus
- **WHEN** the probe sets an accessible description and `hidden` state on a widget in a process started with `GTK_A11Y=test`
- **THEN** both are observed through `gtk_test_accessible_has_property` and `has_state`, the context type is the test context, and the session bus has no `org.a11y.Bus` name afterwards
- **AND** a control run in a nested helper session with a11y on (so the axioms session's own guard stays clean), with `GTK_A11Y` unset, shows the name, and a run with `none` shows no context

#### Scenario: Libadwaita stays off the portal
- **WHEN** the probe initializes Libadwaita and presents a window in a process started with `ADW_DISABLE_PORTAL=1`
- **THEN** the session bus has no `org.freedesktop.portal.Desktop` name afterwards
- **AND** a control run in a nested helper session with portals on, with the variable unset, shows the name, so the probe cannot pass vacuously

#### Scenario: A toolkit update changes either behaviour
- **WHEN** either probe fails after a GTK or Libadwaita update
- **THEN** the helper's environment (design D5 and D6) and the runtime guard are revisited before the failure is resolved

### Requirement: The new axioms' controls run through the helper binary
Because `gtk-lush-axioms` stays a leaf crate, each of the two probes SHALL run its control through the helper binary that the enclosing session exports as `GTK_LUSH_SESSION_BIN`, and SHALL return `FixtureInvalid`, never `Holds`, when that binary or a control prerequisite is missing.

#### Scenario: A control cannot run
- **WHEN** either probe runs with `GTK_LUSH_SESSION_BIN` unset, or on a host without `at-spi-bus-launcher` or `xdg-desktop-portal`
- **THEN** the probe's verdict is `FixtureInvalid` with a bounded diagnostic naming what is missing, and the runner fails as for any other control-step failure
