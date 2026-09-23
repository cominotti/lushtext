## ADDED Requirements

### Requirement: Journal harnesses distinguish window actors from process actors
The draft-journal Kani model SHALL represent a **window actor** and a
**process actor** as distinct things. Windows of one process SHALL share the
model disk, the process-wide manifest write lock, the process-local target
write guard, and any process-wide journal coordinator. Each window SHALL keep
its own editors, manifest copy, authority, restore holds, tombstones, and
in-flight flags. Processes SHALL share only the disk. The two-process harness
that pins axiom A6 (`a_second_writer_breaks_the_journal_invariants`) SHALL stay
a `should_panic` harness, using separate process actors, until the programme
record changes the A6 decision. The multi-window harness SHALL be assigned to
exactly one shard of the Kani shard table.

#### Scenario: The multi-window harness shares process-wide state
- **WHEN** the two-window harness runs
- **THEN** both window actors observe one manifest lock, one target guard, and one coordinator state, and each keeps its own per-window state

#### Scenario: The A6 pin is unchanged
- **WHEN** the Kani lane runs after this change
- **THEN** `a_second_writer_breaks_the_journal_invariants` still finds a counterexample and passes as `should_panic`
- **AND** `make check-kani-shards` reports every harness, including the new one, in exactly one shard
