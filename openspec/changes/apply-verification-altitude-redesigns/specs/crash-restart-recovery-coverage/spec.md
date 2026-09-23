## ADDED Requirements

### Requirement: Smoke binary roles are explicit and the ordinary binary is checked free of kill points
The crash-recovery smoke driver SHALL select the binary for every launch from
one explicit role table with at least a victim role (the process that is killed)
and a relaunch role (the process that must recover the user's work). The
relaunch role SHALL always be the ordinary binary built without the
`crash-kill-points` feature. When a kill-point binary is supplied it SHALL be
the victim of every crash scenario, including the external-signal scenarios,
launched without `LUSHTEXT_KILL_AT` in those scenarios, so that each run also
shows the kill-point binary behaves as the ordinary binary when no window is
named. The smoke SHALL fail if the ordinary relaunch binary, or a release binary
when one is present, contains the kill-point environment variable name or abort
message.

#### Scenario: The relaunch is always the ordinary binary
- **WHEN** any crash scenario relaunches the application after a kill
- **THEN** the relaunched process is the ordinary binary, recorded in the scenario's artifacts

#### Scenario: An inert kill-point victim behaves as the ordinary binary
- **WHEN** a kill-point binary is supplied and an external-signal scenario launches it without `LUSHTEXT_KILL_AT`
- **THEN** the scenario's recovery assertions pass exactly as with the ordinary victim

#### Scenario: Kill-point code in the ordinary binary fails the smoke
- **WHEN** the ordinary relaunch binary contains `LUSHTEXT_KILL_AT` or the kill-point abort message
- **THEN** the smoke fails before any scenario runs and records which binary and marker were found
