## ADDED Requirements

### Requirement: Desktop visual smoke records recovery diagnostics when recovery state is exercised
The desktop visual smoke lane SHALL capture recovery-related runtime diagnostics when a visual or real-session run intentionally exercises corrupted, repaired, restored, or partially unavailable recovery state.

#### Scenario: Recovery warning is captured visually
- **WHEN** a visual smoke run launches with a fixture that produces a grouped startup recovery warning
- **THEN** the screenshot artifact includes the warning or status surface in its intended layout
- **AND** the assertion log records the underlying recovery diagnostic summary

#### Scenario: Quarantine summary is preserved as an artifact
- **WHEN** recovery metadata is quarantined or repaired during a visual smoke run
- **THEN** the smoke artifacts include a bounded quarantine or repair summary
- **AND** the artifact does not include unbounded user document contents

#### Scenario: Unexpected recovery warnings fail normal visual smoke
- **WHEN** a visual smoke state not intended to exercise recovery emits recovery diagnostics
- **THEN** the lane fails or marks the diagnostics as unexpected
- **AND** logs and screenshots are preserved for review

### Requirement: Recovery-focused visual captures stay stable and inspectable
Recovery-focused visual smoke captures SHALL use stable readiness checks before screenshot capture and SHALL preserve enough environment context to distinguish UI regressions from host capture limitations.

#### Scenario: Recovery capture waits for visible state
- **WHEN** a recovery-focused screenshot is requested
- **THEN** the smoke driver waits for the expected recovery warning, restored document content, or diagnostic state before capture
- **AND** it fails clearly if the expected state never appears

#### Scenario: Recovery capture is nonblank and bounded
- **WHEN** the recovery-focused screenshot is captured
- **THEN** it satisfies the same nonblank, monitor-bounded, and chrome-visibility invariants as ordinary visual smoke captures

#### Scenario: Capture tooling gaps skip clearly
- **WHEN** the host lacks screenshot, compositor, D-Bus, or accessibility tooling required for the recovery capture
- **THEN** the visual smoke lane reports a clear skip reason
- **AND** the unsupported recovery visual coverage is not counted as verified
