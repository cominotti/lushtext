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

The construction gate SHALL also prove that the first frame its evidence records was painted while
the window was mapped.

A composition widget gate SHALL prove:
- that `open` and `activate` return without a window, hold the application, and merge a second
  request made before the window exists;
- that realize and present run in one dispatch, and that every `after-paint` of the window's frame
  clock fires while the window is mapped;
- the stage order, that no reveal stage starts before the window's first frame has painted, that
  one dispatch reveals every pending reveal stage, and that composition completes on the next
  painted frame;
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
the keyfile and dconf backends, and for a steady-state populated profile with a restored workspace
folder and the properties pane shown:
- the toolkit startup, LushText startup, window-construction, first-frame, and
  composition-complete spans, the activation-shown span, and the content fill-in time from the
  first frame to the later of composition complete and activation shown;
- the first presentation time, from the probe's frame records;
- one span per composition stage;
- the layout and paint phases of each window's first frames;
- the largest watchdog lateness between the `open` handler and composition complete.

The lane SHALL assert each dispatch until the later of composition complete and activation shown
against B_settle, and each run's content fill-in against the accepted 40 ms (within five reference
frames), SHALL fail the run on either only under `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` and never
in CI, SHALL report the fill-in median against three reference frames, SHALL derive the first
painted frame the same way for every build (the first frame painted after the present stage's
dispatch ended), so a build that ran an unseen frame is compared honestly, and SHALL
include the build profile, the backend, the machine load, the reference-precondition check (load,
CPU-heavy processes, and other agents' build, test, or proof processes), and `btrfs-stall-report`
evidence for the run window. The baseline SHALL be recorded failing-first before the change.

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

#### Scenario: An unmapped first frame fails the gate
- **WHEN** a change lets a frame of the window run between its realize and its map
- **THEN** the composition gate fails, and the construction gate fails if its evidence records that frame as the first frame

#### Scenario: Fill-in above five reference frames fails the measured lane
- **WHEN** the lane runs with `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` and a run's content fill-in exceeds the accepted 40 ms
- **THEN** the lane fails and names the case and the measured fill-in
