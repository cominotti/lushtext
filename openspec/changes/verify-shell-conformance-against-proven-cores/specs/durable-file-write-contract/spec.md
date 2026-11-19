## ADDED Requirements

### Requirement: The shared protocol driver hosts one test-only conformance tap
Only under `cfg(test)` or the `test-utils` feature, the shared protocol
driver SHALL let a thread-local tap record each action and its fed-back
outcome, tagged with the protocol kind and nesting. The tap SHALL be able to
run a hook or simulate a crash before or after any action. It SHALL be
installed through a guard that removes it on drop and SHALL NOT be visible to
other threads. Shipping builds SHALL compile it to nothing.

#### Scenario: Every action of every shell is observed
- **WHEN** a tap is installed and any of the six proven-core shells runs
- **THEN** the tap's record lists each executed action and the outcome fed back to the core, with a move's copy recorded as a nested run

#### Scenario: A crash leaves the disk as the executed calls left it
- **WHEN** the tap stops a directly called durable-write primitive before or after an action
- **THEN** no further action or cleanup runs, and the disk holds exactly what the executed backend calls left

#### Scenario: The release binary has no tap
- **WHEN** the release binary is built with default features
- **THEN** it contains none of the tap's code or marker strings

### Requirement: Durable-write faults are injected at the backend call
Typed faults SHALL be injected by making one fallible backend call return an
error, addressed within a protocol run, by a run's destination, or by call
order across the armed scope. The shell SHALL build its outcome and recorded
error through its ordinary error path, so an injected fault is handled
exactly as a real one. The existing durable-write test hooks SHALL be
expressed through this injection, with their setters and callers
unchanged.

#### Scenario: An injected collision is a real collision
- **WHEN** the temp-file creation call is made to fail with `AlreadyExists`
- **THEN** the shell feeds `AlreadyExists` back to the core and tries the next temp name, as it does for a real collision

#### Scenario: Existing hook callers are unchanged
- **WHEN** an existing test arms a durable-write fault through its `*_for_test` setter, in any module
- **THEN** the same backend call fails as before, and the test passes without any edit to its source

#### Scenario: An injected cross-device rename takes the real path
- **WHEN** the rename call of a durable rename is made to fail with a cross-device error
- **THEN** the error reaches the caller through the rename protocol's classification, not through a path that bypasses the protocol

### Requirement: The durable-write shell conforms to its disk model
A deterministic test SHALL run every durable-write primitive over a temporary
directory for every combination of destination state, action, backend call,
and injection. For each case it SHALL step the shared disk model with the
recorded outcomes. After every action it SHALL check that the outcome fed
back matches the action's backend results and that the real disk matches the
model's live state. Each action's call count SHALL be pinned.

#### Scenario: Outcomes reflect the backend
- **WHEN** any action of an atomic write, copy, move, or rename completes in the enumeration
- **THEN** the outcome fed back is success exactly when every fallible call of that action succeeded, except a temp removal, whose outcome the core provably ignores, a metadata probe that finds an absent destination it does not require, which succeeds, and content writing, whose outcome is the content stream's
- **AND** the action's number of fallible calls equals the pinned table's entry

#### Scenario: The real disk matches the live model
- **WHEN** any action completes
- **THEN** the destination bytes equal the disk model's visible content, the temp file exists exactly while the model's temp is linked, and the temp's mode is no wider than the destination's

#### Scenario: A crash at any action leaves an allowed destination
- **WHEN** the tap stops an atomic write before or after any action
- **THEN** the destination holds the complete previous bytes or the complete new bytes, as the disk model's live state predicts
- **AND** any leftover temp file is one the startup leftover sweep recognises as LushText's
