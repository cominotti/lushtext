## ADDED Requirements

### Requirement: Window construction has deterministic and measured responsiveness coverage
The test lanes SHALL cover the window-construction path with a deterministic
pull-request gate and a measured lane. The widget gate SHALL count GSettings
backend writes during construction under a private keyfile backend: zero on a
steady-state profile, zero before the first presented frame on a fresh profile
followed by one coalesced flush whose persisted values equal the pure
migration oracle, at most one write per persistence window during a resize
burst, and exact window state after close. The measured lane SHALL record the
toolkit startup, LushText startup, window-construction, and first-frame spans
and the largest dispatch before the first frame on the release build for the
fresh and steady-state profiles under the keyfile and dconf backends, assert
them against B_settle, and include the build profile, backend, and
`btrfs-stall-report` evidence for the run window. The baseline SHALL be
recorded failing-first before the fix.

#### Scenario: Write-count gate runs on pull requests
- **WHEN** the widget shard that owns the construction gate runs
- **THEN** it fails if any GSettings backend write occurs before the first presented frame or on a steady-state construction

#### Scenario: Measured lane reports construction spans
- **WHEN** the responsiveness or performance smoke lane runs the construction scenario
- **THEN** its report contains every span, the profile state, the backend, the build profile, and stall-report evidence
- **AND** it fails when a LushText-owned span or a pre-first-frame dispatch exceeds B_settle on the release build

#### Scenario: Failing-first baseline exists
- **WHEN** the change is reviewed
- **THEN** the stream record contains the pre-fix measurement showing the construction stall above B_settle and the post-fix measurement within it
