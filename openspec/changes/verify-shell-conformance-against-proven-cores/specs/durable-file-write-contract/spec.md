## ADDED Requirements

### Requirement: Durable-write fault injection is per-action, observes the shell, and is absent from shipping builds
The durable-write shell SHALL expose, only under `cfg(test)` or the
`test-utils` feature, one backend tap for the atomic-write, copy, move, and
rename protocols. For every protocol action the tap SHALL record three things:
the action, the outcome the shell fed back to the core, and the number of
fallible backend calls the shell made for that action. The tap SHALL be able to
replace any single action's backend call with an injected `Failed` or
`AlreadyExists` outcome, and to stop the shell before or after any action to
model a crash. Stopping leaves the disk exactly as the executed calls left it.
Tap state SHALL be installed through a guard that removes it when dropped, and
it SHALL NOT be visible to other threads. The existing single-point test hooks
SHALL be expressed through the tap. A default-feature release build SHALL
contain no tap code.

#### Scenario: One fallible backend call per action
- **WHEN** a conformance run executes any atomic write, copy, move, or rename through the tap
- **THEN** every action other than `Finish` made exactly one fallible backend call
- **AND** the outcome fed back to the core equals that call's outcome, except for `RemoveTemp`, whose outcome the core provably ignores

#### Scenario: A crash at any action leaves an allowed destination
- **WHEN** the tap stops an atomic write immediately after any action
- **THEN** the destination holds the complete previous bytes or the complete new bytes, as the disk model's live state predicts
- **AND** any leftover temp file carries a name the startup leftover sweep recognises as LushText's

#### Scenario: The release binary has no tap
- **WHEN** the release binary is built
- **THEN** it contains none of the tap's symbols or messages
