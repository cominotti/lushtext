## MODIFIED Requirements

### Requirement: The ordinary binary is checked free of kill points
Before any scenario runs, the crash-recovery smoke SHALL fail if the ordinary
binary, or a release binary when one is present, contains the kill-point
environment variable name or abort message, or any marker of the test-only
conformance tap. It SHALL record which binary and marker were found. A
policy check SHALL fail if the `crash-kill-points` feature gates any code
outside the kill-point module.

#### Scenario: Kill-point code in the ordinary binary fails the smoke
- **WHEN** the ordinary relaunch binary contains `LUSHTEXT_KILL_AT` or the kill-point abort message
- **THEN** the smoke fails before any scenario runs and records which binary and marker were found

#### Scenario: A second feature-gated site fails the policy check
- **WHEN** code outside the kill-point module is gated on the `crash-kill-points` feature
- **THEN** `make check-policy` fails and names the offending site

#### Scenario: Conformance-tap code in the ordinary binary fails the smoke
- **WHEN** the ordinary relaunch binary, or a present release binary, contains a conformance-tap marker
- **THEN** the smoke fails before any scenario runs and records which binary and marker were found
