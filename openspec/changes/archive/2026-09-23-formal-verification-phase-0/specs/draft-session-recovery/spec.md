## ADDED Requirements

### Requirement: A draft body is never written for an unregistered id
The system SHALL durably register a manifest entry for a draft id before writing
that id's first draft body, so that at every crash point each draft body on disk
either has a persisted manifest entry or carries an untitled draft id. The
registered entry MUST carry the same original path and backing-file mtime that
the body's eventual acceptance would record. An entry whose body was never
written MUST be handled by the existing missing-body path and MUST NOT authorize
deleting any other body.

#### Scenario: Crash between a first file-backed body write and the batch commit
- **WHEN** a saved file becomes dirty for the first time, its draft body is written, and the process is killed before the autosave pass commits its manifest update
- **THEN** the next startup finds a persisted manifest entry for that body
- **AND** the draft is offered through the normal file-backed restore path
- **AND** the manifest remains trusted so later autosaves and orphan cleanup proceed normally

#### Scenario: Crash after registration but before the body write
- **WHEN** the entry for a new draft id was registered and the process is killed before the body was written
- **THEN** the next startup treats the entry as a missing-body draft
- **AND** no other draft body or manifest entry is removed because of it

### Requirement: An unattributable draft body never wedges the draft journal
The system SHALL keep reconciliation able to reach a complete inventory when the
drafts directory contains a body without a manifest entry. It SHALL first
attribute an unregistered stable-path-hash body to a session tab whose path
hashes to that id, and reconstruct its entry from that tab with an unproven
backing mtime. Such a draft MUST NOT be restored over the file, and SHALL be
preserved through the stale-draft preservation path, which keeps a set-aside
copy plus a local-history snapshot. It SHALL move a body
that still cannot be attributed into a preserved set-aside location outside the
recoverable inventory and report it with a bounded diagnostic. Such a body MUST
NOT be deleted, and its presence MUST NOT keep every later manifest update
failing.

#### Scenario: An existing wedged installation recovers on upgrade
- **WHEN** startup finds an unregistered stable-path-hash body whose id matches the stable path hash of a file-backed session tab
- **THEN** reconciliation reconstructs that draft's manifest entry from the session tab
- **AND** a byte-identical copy of the body is kept in the set-aside location
- **AND** when the tab opens, the edits are preserved as stale-draft content and are not applied over the file
- **AND** the reconciled manifest is complete and trusted

#### Scenario: An unattributable body is preserved without blocking
- **WHEN** startup finds an unregistered stable-path-hash body that matches no session tab
- **THEN** the body is moved, not deleted, into the set-aside location and a diagnostic reports it
- **AND** later autosave commits succeed and clear dirty state normally
- **AND** orphan cleanup never deletes bodies in the set-aside location

#### Scenario: The wedge window is covered by a crash-window test
- **WHEN** the automated suite simulates a crash between body writes and the manifest commit for a pass containing more than one new file-backed draft
- **THEN** every body written before the crash is recoverable at the next startup
