## ADDED Requirements

### Requirement: File-backed saves preserve the destination's identity metadata
The system SHALL preserve a file-backed document's on-disk identity metadata
across a save that overwrites an existing file. After a successful save, the file
MUST retain its prior permission (mode) bits and SHALL retain ownership, POSIX
ACLs, and extended attributes on a best-effort basis, so editing a file does not
silently change who can read or execute it.

#### Scenario: Saving an executable script keeps it executable
- **WHEN** the user edits and saves a file that was marked executable
- **THEN** the saved file on disk is still marked executable

#### Scenario: Saving a private file keeps its restrictive permissions
- **WHEN** the user edits and saves a file whose mode is `0600`
- **THEN** the saved file on disk is still `0600` and is not widened to be group- or world-readable

### Requirement: Save failures distinguish unwritten changes from undurable writes
The system SHALL report a save that failed before the destination was replaced
differently from a save whose bytes reached the destination but whose directory
durability could not be confirmed. A before-rename failure MUST tell the user the
changes were not written and keep the document modified. An after-rename
durability failure MUST tell the user the changes are on disk but not yet
confirmed durable, and MUST keep the document modified so a retry can re-attempt
the directory flush, rather than presenting a generic lost-save error.

#### Scenario: Pre-rename failure reports unwritten changes
- **WHEN** a save fails while writing or renaming the temp file
- **THEN** the editor reports that the changes were not written
- **AND** the document remains marked modified

#### Scenario: Post-rename durability failure reports a distinct warning
- **WHEN** a save replaces the destination but the directory durability sync fails
- **THEN** the editor surfaces a durability warning that the changes are on disk but not yet confirmed durable
- **AND** the document remains marked modified so the user can retry
- **AND** the failure is not presented as an indistinguishable lost save
