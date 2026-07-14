## ADDED Requirements

### Requirement: Draft capture rejects source mutation during snapshotting
The system MUST cancel a chunked draft snapshot when its source buffer changes and MUST NOT write or accept the partially captured body. The cancellation path SHALL retain draft-dirty state and coalesce a later attempt for the latest editor generation.

#### Scenario: Edit occurs during a large autosave snapshot
- **WHEN** the user inserts or deletes text while a large draft is being captured across main-loop turns
- **THEN** the in-progress capture produces no draft body or manifest acceptance
- **AND** a later autosave can protect the complete newer contents

#### Scenario: Close flush snapshot changes unexpectedly
- **WHEN** a close-time draft snapshot observes source mutation or lifecycle cancellation
- **THEN** close does not treat that generation as protected
- **AND** the close workflow preserves the editor or reports the unresolved recovery failure
