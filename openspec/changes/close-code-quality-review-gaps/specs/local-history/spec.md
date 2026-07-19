## ADDED Requirements

### Requirement: Undo Restore bodies retain guarded ownership
Local History Undo Restore SHALL reserve within the existing 64 MiB history ownership policy and move, rather than clone, the pre-restore document body through safety persistence, GTK handoff, undo storage, replacement, cancellation, and supersession. Every document-sized plain-text owner MUST be admitted or disposal-guarded before capture begins or it crosses back to GTK, and its final destructor MUST run off GTK.

#### Scenario: History ownership is at capacity
- **WHEN** a Local History restore cannot reserve its conservative current-buffer ownership
- **THEN** compact current intent is deferred or the operation fails visibly according to existing admission policy
- **AND** snapshot capture, safety persistence, and buffer mutation do not begin before capacity is available

#### Scenario: Safety capture succeeds
- **WHEN** Undo Restore captures the current document and persists its required safety snapshot
- **THEN** the worker returns the same captured body under guarded ownership instead of cloning it for GTK
- **AND** GTK installs only the lightweight owner needed to offer Undo Restore

#### Scenario: User activates Undo Restore
- **WHEN** a guarded undo body is still current and the user requests restoration
- **THEN** ownership moves into the bounded buffer-replacement workflow without a full-text clone on GTK
- **AND** generation, page lifetime, modified-state, cursor, and recovery semantics remain unchanged

#### Scenario: Guarded undo text is replaced or abandoned
- **WHEN** a newer restore supersedes the prior body, the editor closes, cancellation wins, or Undo Restore is cleared
- **THEN** the old plain-text owner is detached immediately
- **AND** its final document-sized destruction occurs on the existing bounded off-GTK disposal path

#### Scenario: Restore is cancelled after guard handoff
- **WHEN** a fresh cancellation wins after the worker returns the guarded safety body
- **THEN** the same guard is retained or returned according to current Undo Restore semantics without cloning
- **AND** a stale cancellation cannot publish or destroy the newer guarded body

#### Scenario: Safety persistence fails
- **WHEN** the pre-restore safety snapshot cannot be durably persisted
- **THEN** the editor buffer is not replaced
- **AND** the captured body is disposed without becoming an unguarded large GTK-owned value
