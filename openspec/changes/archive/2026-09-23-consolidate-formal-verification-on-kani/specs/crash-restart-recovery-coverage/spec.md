## ADDED Requirements

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
