## ADDED Requirements

### Requirement: Real-process crash recovery smoke verifies restored user work
The project SHALL provide a real-process crash/restart smoke lane that launches LushText with isolated app state, creates recoverable unsaved user work, terminates the app process abruptly, relaunches with the same state, and verifies that recovery restores the expected work.

#### Scenario: File-backed draft survives abrupt termination
- **WHEN** the crash recovery smoke lane modifies a file-backed document and terminates LushText after draft recovery data is persisted
- **THEN** relaunching LushText restores the file-backed draft content
- **AND** the user-visible restored-draft feedback is present

#### Scenario: Untitled draft survives abrupt termination
- **WHEN** the crash recovery smoke lane creates an untitled modified document and terminates LushText after draft recovery data is persisted
- **THEN** relaunching LushText recreates the untitled tab
- **AND** the untitled draft content is restored

#### Scenario: Session selection survives abrupt termination
- **WHEN** the crash recovery smoke lane has multiple tabs and a selected tab before termination
- **THEN** relaunching LushText restores the expected tab set and selected tab when session metadata was persisted before termination

### Requirement: Crash recovery smoke uses isolated state and preserves diagnostics
The crash recovery smoke lane SHALL run against isolated data, config, cache, and runtime directories and SHALL preserve logs, metadata snapshots, environment reports, and recovery assertions as artifacts.

#### Scenario: Smoke state is isolated from the user environment
- **WHEN** the crash recovery smoke lane runs
- **THEN** it uses isolated XDG and LushText data directories
- **AND** it does not read or write the user's normal LushText recovery state

#### Scenario: Smoke artifacts include before and after state
- **WHEN** the crash recovery smoke lane completes
- **THEN** it stores before-crash and after-relaunch metadata snapshots or summaries
- **AND** it preserves stdout, stderr, runtime logs, environment details, and assertion results

#### Scenario: Unexpected runtime warnings fail the lane
- **WHEN** crash recovery smoke logs contain unexpected GTK, GDK, Libadwaita, GIO, portal, accessibility, or filesystem warnings
- **THEN** the smoke lane fails with those logs preserved

### Requirement: Crash recovery smoke avoids coordinate-only driving
The crash recovery smoke lane SHALL drive the app through stable actions, accessibility-visible controls, debug-only test actions, or a deterministic helper interface. It MUST NOT depend only on fixed sleeps and pointer coordinates for core correctness assertions.

#### Scenario: Driver waits for persisted recovery data
- **WHEN** the smoke lane creates modified content
- **THEN** it waits for draft or session metadata evidence through a deterministic check before sending the abrupt termination signal
- **AND** the check fails clearly if recovery data never appears

#### Scenario: Relaunch assertions inspect stable state
- **WHEN** the app is relaunched after termination
- **THEN** the smoke lane verifies restored content and session state through stable text, actions, accessibility information, or app-owned metadata
- **AND** a screenshot alone is not the only proof of recovery correctness

#### Scenario: Host support gaps skip clearly
- **WHEN** the host lacks required compositor, D-Bus, screenshot, or driver tooling
- **THEN** the crash recovery smoke lane reports a clear skip reason
- **AND** the unsupported run is not counted as verified recovery coverage

### Requirement: Crash recovery smoke is integrated with developer and scheduled validation
The project SHALL expose the crash recovery smoke through documented local commands and scheduled or manual CI once stable. It MUST remain outside the default fast pull-request gate unless it becomes cheap and deterministic enough for routine PR feedback.

#### Scenario: Local command exists
- **WHEN** a maintainer lists development validation commands
- **THEN** there is a documented command for running crash recovery smoke locally

#### Scenario: Scheduled smoke preserves artifacts
- **WHEN** scheduled or manually triggered end-user smoke includes crash recovery
- **THEN** the lane uploads crash recovery artifacts regardless of pass or fail

#### Scenario: Pull request CI stays bounded
- **WHEN** default pull-request CI runs
- **THEN** host-sensitive crash recovery smoke is not required unless explicitly promoted after stability review

### Requirement: Confined crash recovery is covered when runtimes support it
The project SHALL extend crash/restart recovery verification to confined Flatpak or Snap runs when the relevant package can be built, installed, launched, and granted the required app state access on the test host.

#### Scenario: Confined runtime restores recovery state
- **WHEN** a confined runtime smoke lane supports crash/restart testing
- **THEN** it verifies draft and session recovery within the confined app data location
- **AND** it records runtime permissions and denials as artifacts

#### Scenario: Unsupported confined crash recovery skips clearly
- **WHEN** Flatpak, Snap, portal, or platform support is unavailable for confined crash recovery
- **THEN** the lane reports a clear skip reason
- **AND** native crash recovery coverage remains distinct from confined coverage

