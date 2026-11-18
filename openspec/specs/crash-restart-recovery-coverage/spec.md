# crash-restart-recovery-coverage Specification

## Purpose
Define LushText's crash/restart recovery smoke coverage so restored drafts, sessions, diagnostics, and confined runtime behavior stay verifiable after abrupt process termination.

## Requirements

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

### Requirement: Crash recovery smoke can kill inside named protocol windows
The crash-recovery smoke SHALL be able to terminate the real application
process deterministically inside named protocol windows, using kill points
that are compiled only into smoke builds behind a feature gate. The windows
SHALL include:

- a draft body written before its manifest commit;
- a durable write renamed before its parent-directory sync;
- a stale draft preserved before its retirement.

For each window, the smoke SHALL relaunch the application and verify that no
user work is lost, preserving diagnostics as for the existing kill point.
Production builds MUST NOT contain kill-point code.

#### Scenario: Kill between a draft body write and its commit
- **WHEN** the smoke kills the process at the body-written-before-commit window and relaunches it
- **THEN** the edits are recoverable and the draft journal is trusted

#### Scenario: Production builds carry no kill points
- **WHEN** a release or Flatpak build is produced
- **THEN** the kill-point feature is disabled and no kill-point code is compiled

### Requirement: Replace All crash windows are exercised by the crash smoke
The crash smoke SHALL drive Replace All over a multi-file fixture through automation and stop the process at each Replace All kill point (`replace-body-before-entry`, `replace-entry-before-target-rename`, `replace-target-renamed-before-finish`, `durable-renamed-before-dirsync@replace`, and `replace-undo-before-target-rename` in a later Undo). After relaunch each target SHALL be original, or replaced with an Undo that restores it, with no orphan body or temp left.

#### Scenario: Killed with the entry armed
- **WHEN** the process is killed at `replace-entry-before-target-rename` and relaunched
- **THEN** that target holds its original bytes and its temp file is removed, and the files written before it are offered for Undo and restored by it
- **AND** the smoke's artifacts record the journal state before and after the relaunch

### Requirement: Replace All kill points land inside their windows
A kill point SHALL accept an occurrence ordinal, so that a kill fires on a later file of the fixture, after the earlier files were replaced. The `replace-entry-before-target-rename` and `replace-undo-before-target-rename` points SHALL sit after the target's temp file holds its bytes, so that recovery has a temp file to remove.

#### Scenario: Killed on the third of five files
- **WHEN** the smoke sets `LUSHTEXT_KILL_AT=replace-entry-before-target-rename@replace#3` and confirms a Replace All over five files
- **THEN** the process stops with the first two files replaced and a written temp beside the third, which keeps its original bytes

### Requirement: Smoke binary roles are explicit
The crash-recovery smoke driver SHALL choose each launch's binary from one
role table: seed (prepares state), victim (is killed), and relaunch (recovers
the work). Seed and relaunch SHALL always be the ordinary binary. A supplied
kill-point binary SHALL be every crash scenario's victim, without
`LUSHTEXT_KILL_AT` in the external-signal scenarios. Each launch's artifacts
SHALL record its role and binary.

#### Scenario: The relaunch is always the ordinary binary
- **WHEN** any crash scenario relaunches the application after a kill
- **THEN** the relaunched process is the ordinary binary, recorded in the scenario's artifacts

#### Scenario: An inert kill-point victim behaves as the ordinary binary
- **WHEN** a kill-point binary is supplied and an external-signal scenario launches it without `LUSHTEXT_KILL_AT`
- **THEN** the scenario's recovery assertions pass exactly as with the ordinary victim

### Requirement: The ordinary binary is checked free of kill points
Before any scenario runs, the crash-recovery smoke SHALL fail if the ordinary
binary, or a release binary when one is present, contains the kill-point
environment variable name or abort message. It SHALL record which binary and
marker were found. A policy check SHALL fail if the `crash-kill-points`
feature gates any code outside the kill-point module.

#### Scenario: Kill-point code in the ordinary binary fails the smoke
- **WHEN** the ordinary relaunch binary contains `LUSHTEXT_KILL_AT` or the kill-point abort message
- **THEN** the smoke fails before any scenario runs and records which binary and marker were found

#### Scenario: A second feature-gated site fails the policy check
- **WHEN** code outside the kill-point module is gated on the `crash-kill-points` feature
- **THEN** `make check-policy` fails and names the offending site
