## MODIFIED Requirements

### Requirement: Window construction has deterministic and measured responsiveness coverage
The test lanes SHALL cover the window-construction and composition path with deterministic
pull-request gates and a measured lane.

The widget gate SHALL count GSettings backend writes during construction under a private keyfile
backend:
- zero on a steady-state profile;
- zero before the first presented frame on a fresh profile, followed by one coalesced flush whose
  persisted values equal the pure migration oracle;
- at most one write per persistence window during a resize burst;
- exact window state after close.

A composition widget gate SHALL prove:
- that `open` and `activate` return without a window, hold the application, and merge a second
  request made before the window exists;
- the stage order, and that no stage from the content stage on starts before the frame showing its
  predecessor has painted;
- that each forced-reveal path (accelerator, window action, activation file open, session-restore
  completion, and automation action, each issued before each stage) produces the same state as on
  a fully composed window, and that reads and projections force no stage;
- that the `window-composition` readiness blocker clears exactly when composition completes, and
  when the window goes away;
- that closing or destroying the window during any stage runs no later stage and keeps unrestored
  session descriptors;
- that the initial focus equals today's composed window;
- that revealing staged content never changes the rail or pane allocation, and that the chrome
  bounds are constant at every painted frame of composition.

The measured lane SHALL record, on the release build, for the fresh and steady-state profiles under
the keyfile and dconf backends:
- the toolkit startup, LushText startup, window-construction, first-frame, and
  composition-complete spans, and the content fill-in time between the last two;
- one span per composition stage;
- the layout and paint phases of each window's first frames;
- the largest watchdog lateness between the `open` handler and composition complete.

The lane SHALL assert each dispatch against B_settle, and SHALL include the build profile, the
backend, the machine load, the reference-precondition check (load, CPU-heavy processes, and other
agents' build, test, or proof processes), and `btrfs-stall-report` evidence for the run window. The baseline SHALL
be recorded failing-first before the change.

#### Scenario: Write-count gate runs on pull requests
- **WHEN** the widget shard that owns the construction gate runs
- **THEN** it fails if any GSettings backend write occurs before the first presented frame or on a steady-state construction

#### Scenario: Composition gate runs on pull requests
- **WHEN** the widget shard that owns the composition gate runs
- **THEN** it fails if a stage runs out of order, a forced path differs from the composed result, a projection forces a stage, or any chrome bound changes between painted frames

#### Scenario: Measured lane reports construction spans
- **WHEN** the responsiveness smoke lane runs the construction scenario
- **THEN** its report contains every span, stage, and frame phase, the profile state, the backend, the build profile, the load, and stall-report evidence
- **AND** with `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` it fails when any dispatch between the `open` handler and composition complete exceeds B_settle

#### Scenario: Failing-first baseline exists
- **WHEN** the change is reviewed
- **THEN** the stream record contains the pre-change per-dispatch measurement showing the constructing and first-frame dispatches above B_settle, and the post-change measurement of every composition dispatch
