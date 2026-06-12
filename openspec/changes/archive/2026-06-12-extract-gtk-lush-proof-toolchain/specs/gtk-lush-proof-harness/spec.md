## ADDED Requirements

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
- **THEN** the child process receives the headless runner marker, the configured
  virtual-monitor geometry, `GDK_BACKEND=wayland`, and a private runtime
  directory
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
- **WHEN** a widget test run emits an unexpected GTK warning, critical, renderer
  error, broken-pipe message, or session cleanup warning matched by the
  caller's configured warning policy
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
