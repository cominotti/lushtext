## ADDED Requirements

### Requirement: Bookmark sidecars use canonical saved-document identity in app data
The system SHALL persist bookmark sidecars under `$XDG_DATA_HOME/lushtext/bookmarks/` using a saved-document identity derived from the document's canonical path rather than by modifying the source file. The persisted sidecar identity MUST remain separate from the source document bytes and MUST be recomputed for a new Save As destination instead of copying the prior bookmark set automatically.

#### Scenario: Reopening the same saved document restores bookmarks from app data
- **WHEN** the user reopens a saved document that already has persisted bookmarks
- **THEN** the bookmark set is restored from bookmark sidecar data stored under the app data directory
- **AND** the source file itself remains unchanged by bookmark persistence

### Requirement: Empty bookmark state removes its sidecar file
The system SHALL remove a bookmark sidecar file when a document no longer has any persisted bookmarks, instead of leaving an empty bookmark sidecar behind indefinitely.

#### Scenario: Removing the final bookmark deletes the bookmark sidecar
- **WHEN** the user removes the last remaining bookmark for a saved document
- **THEN** the persisted bookmark sidecar for that document is deleted from the app data directory
- **AND** reopening the document no longer restores an empty bookmark sidecar payload
