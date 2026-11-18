## ADDED Requirements

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
