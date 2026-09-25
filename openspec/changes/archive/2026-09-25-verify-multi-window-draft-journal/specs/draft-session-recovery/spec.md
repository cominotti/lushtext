## ADDED Requirements

### Requirement: Draft journal invariants hold across windows of one process
The system SHALL keep the draft journal's safety invariants (S1 acceptance
durability, S2 cleanup safety, S3 delete ordering, S4 trust) when more than one
`LushtextWindow` of one application process uses the same data directory. It
SHALL make process-wide every piece of journal state whose per-window copy lets
one window act on the persisted manifest or bodies while another window's
registration, body write, commit, or deletion is in flight. That covers at
least the journal mutation lane that orders registration, body writes,
manifest commits, and deletions against orphan cleanup; orphan-cleanup
scheduling and its in-flight state; and startup session/draft restore. A
draft id MUST NOT be restored into, or autosaved from, more than one window
of the process at a time. Orphan cleanup in any window MUST NOT delete a body,
or remove a manifest entry, that another window of the process has registered,
is writing, or has not yet committed. A Kani harness SHALL check S1–S4 over
bounded action sequences, including crashes and restarts, for two in-process
windows that share the disk, the manifest write lock, and the target write
guard, with separate editors and per-window state. The harness SHALL state
its bounds. Every counterexample it finds SHALL either be fixed behind a
multi-window widget test that failed before the fix, or be recorded, with
reachability evidence and a maintainer decision, as a `should_panic` harness.
Single-window behaviour MUST NOT change.

#### Scenario: Invariants hold with two windows under crashes
- **WHEN** the two-window Kani harness explores action sequences within its stated bounds, with either window acting at each step and crashes and restarts possible at every step
- **THEN** S1–S4 hold in every reachable state
- **AND** the single-window journal harness and the bounded-liveness harness remain proved at their existing bounds

#### Scenario: One window's cleanup does not remove another window's registration
- **WHEN** window A has registered a new file-backed draft id and its body write has not completed, and window B's orphan cleanup pass runs in the same process
- **THEN** the manifest entry A registered is not removed as a missing-body entry
- **AND** A's body write and commit complete with a manifest entry describing the body

#### Scenario: One window's cleanup does not delete another window's uncommitted body
- **WHEN** window A has written an untitled draft body whose manifest commit has not completed, and window B's orphan cleanup inspects the drafts directory
- **THEN** the body is retained
- **AND** after A's commit the draft is recoverable at the next startup

#### Scenario: A second window does not restore the same drafts again
- **WHEN** a second window is created after the first window's startup restore has run in the same process
- **THEN** the second window does not restore the session's draft-backed tabs a second time
- **AND** no draft id is open and autosaved in both windows at once

#### Scenario: The same file opened in two windows keeps one draft owner
- **WHEN** a file is open in one window and the user opens the same file in another window of the same process
- **THEN** the draft id derived from that path is owned by exactly one window's autosave and deletion at any time
- **AND** a discard or save in either window never deletes a body the other window accepted and has not resolved

#### Scenario: Single-window behaviour is unchanged
- **WHEN** only one window exists in the process
- **THEN** autosave, restore, deletion, and orphan cleanup behave exactly as before this change, and the existing draft widget and integration tests pass unchanged
