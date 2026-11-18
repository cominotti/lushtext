## Purpose

Define the reusable GTK Lush proof harness crate for running GTK widget tests
in isolated, reviewable, headless sessions without depending on LushText app
code.

## Requirements

### Requirement: Proof harness crate is an independently adoptable family member
`gtk-lush-proof-harness` SHALL be a GTK Lush family crate under
`crates/gtk-lush/proof-harness` with package name
`gtk-lush-proof-harness`. It MUST remain a leaf crate: it SHALL NOT depend on
LushText crates or on another `gtk-lush-*` family crate, and it SHALL be usable
as a dev-dependency by a stock gtk-rs application without restructuring that
application.

#### Scenario: Family policy accepts the proof harness crate
- **WHEN** `make check-gtk-lush-policy`, `make gtk-lush-doctests`,
  `make gtk-lush-examples`, `make gtk-lush-msrv`, and
  `make gtk-lush-api-advisory` run after the crate is added
- **THEN** `gtk-lush-proof-harness` is included in the same family checks as
  the existing GTK Lush crates
- **AND** no policy check requires it to depend on LushText or another family
  crate

#### Scenario: Stock gtk-rs example adopts the harness
- **WHEN** the crate's adoption example is compiled
- **THEN** it demonstrates registering ordinary Rust test functions, launching
  them through the harness, and waiting for GTK work without importing any
  LushText crate
- **AND** the README documents the minimal setup needed by a generic gtk-rs
  application

### Requirement: Harness launches an isolated headless GTK session
The proof harness SHALL provide reusable APIs for launching tests under a
private `dbus-run-session` and `mutter --headless` session. The session runner
MUST isolate `XDG_RUNTIME_DIR`, remove inherited live-display variables unless
explicitly overridden, support configurable virtual-monitor geometry, and
report unsupported host tooling as a stable harness status rather than as a
test failure.

#### Scenario: Missing compositor tooling is distinct from test failure
- **WHEN** the harness cannot find or launch required session tooling such as
  `dbus-run-session` or `mutter`
- **THEN** it exits nonzero with a stable unsupported-host status and a bounded
  diagnostic
- **AND** it does not report an individual widget test as failed

#### Scenario: Headless session does not use the live desktop display
- **WHEN** the parent harness relaunches into the private session
- **THEN** the child process receives the headless runner marker, the
  configured virtual-monitor geometry, `GDK_BACKEND=wayland`, and a private
  runtime directory
- **AND** inherited `DISPLAY` and `WAYLAND_DISPLAY` are absent unless a
  documented debug override is enabled

### Requirement: Harness runs each widget test in a subprocess
The proof harness SHALL run each selected widget test in a fresh child process
inside the headless session. It MUST support list mode, name filters,
`--exact`, `--skip`, stable display names, bounded retry attempts, and loud
flake reporting for tests that pass only after retry.

#### Scenario: Selected test failure is isolated
- **WHEN** one selected widget test panics or exits unsuccessfully
- **THEN** later selected tests still run in their own child processes
- **AND** the final harness result lists the failed test names and exits with a
  stable failure code

#### Scenario: Retry pass is reported as flaky
- **WHEN** a child test fails on the first attempt and passes on a later
  allowed attempt
- **THEN** the overall run may succeed
- **AND** stderr includes a bounded flake summary that names the affected tests
  without using GTK warning keywords that would trip warning scans

### Requirement: Harness wait helpers preserve GTK main-loop semantics
The proof harness SHALL provide documented wait helpers for GTK tests,
including immediate event draining, delay-then-drain, presentation/realization
settling, and `wait_until` polling. `wait_until` MUST drain ready main-loop
sources to exhaustion after each poll so low-priority `glib::idle_add_once`
callbacks from background completions cannot be starved by the wait loop.

#### Scenario: Background idle completion is observed
- **WHEN** a test schedules a low-priority GLib idle completion and then calls
  the harness `wait_until`
- **THEN** the wait helper dispatches the idle source before the timeout when
  no higher-priority sources remain
- **AND** the test does not need a fixed sleep to observe the completion

#### Scenario: Wait timeout is actionable
- **WHEN** a `wait_until` predicate remains false through the configured
  timeout
- **THEN** the helper panics or returns the documented timeout result with the
  configured duration
- **AND** it does not block the GTK main loop indefinitely

### Requirement: Harness exposes warning and artifact hooks
The proof harness SHALL provide hooks for callers or wrappers to capture stdout
and stderr, scan for GTK/session warnings, attach per-test artifacts, and
distinguish harness warnings from app warnings. Warning filtering MUST be
reviewable and must not silently hide unexpected GTK warnings.

#### Scenario: Unexpected GTK warning fails the wrapper
- **WHEN** a widget test run emits an unexpected GTK warning, critical,
  renderer error, broken-pipe message, or session cleanup warning matched by
  the caller's configured warning policy
- **THEN** the wrapper fails the lane and points to the captured log artifact
- **AND** the harness does not relabel the warning as a passing test result

#### Scenario: Harness bookkeeping stays bounded
- **WHEN** the harness writes retry, skip, unsupported-host, or session-launch
  diagnostics
- **THEN** diagnostics are bounded human-readable lines and optional structured
  summaries
- **AND** they do not include user document contents or unbounded logs

### Requirement: LushText consumes the extracted harness without command drift
LushText's widget test target SHALL become a consumer of
`gtk-lush-proof-harness` while preserving existing Makefile entry points,
default monitor behavior, child-test isolation, retry behavior, warning
filtering, and custom app setup.

#### Scenario: Existing widget commands still work
- **WHEN** a developer runs `make test-widget` or `make test-widget-headless`
- **THEN** the command runs the LushText widget suite through the extracted
  harness
- **AND** existing filters, list mode, retries, headless monitor defaults, and
  warning-scan behavior remain documented and functional

#### Scenario: LushText app setup stays local
- **WHEN** the widget suite constructs a LushText application or window
- **THEN** LushText-specific GResource registration, GSettings isolation,
  test-data directory setup, filesystem fixtures, and widget registry
  generation remain in LushText adapter code or explicit app callbacks
- **AND** `gtk-lush-proof-harness` does not import `lushtext-core`

### Requirement: Widget-test window presentation is a single shared helper
Widget tests SHALL present test windows through one shared presentation
helper that presents the window, waits for realization/allocation with an
async-scale budget, and drains pending main-loop work. Per-module private
copies of the presentation helper MUST NOT exist, and the widget-wiring
documentation MUST name the helper's real home so the shared-helper claim
stays true. The helper MAY live in the proof-harness crate or in the
LushText widget-test common module; wherever it lives, all widget-test
modules import it from that one place.

#### Scenario: All widget-test modules share one presentation path
- **WHEN** a widget-test module needs a presented, realized window
- **THEN** it calls the shared presentation helper
- **AND** no module-local `present_window` (or equivalent divergent copy)
  remains in the widget-test tree

#### Scenario: Presentation waits on realization, not on luck
- **WHEN** the shared helper presents a window under the headless harness
- **THEN** it waits on an allocation/realization predicate with a generous
  async budget before returning
- **AND** a loaded machine delays completion without flaking the caller

#### Scenario: Documentation matches the helper's real home
- **WHEN** the widget-wiring rules describe shared widget-test wait and
  presentation helpers
- **THEN** the named module actually exports the presentation helper
- **AND** a future divergent copy is caught by review against the documented
  single-home contract

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
