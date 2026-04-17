# draft-session-recovery Specification

## Purpose
TBD - created by archiving change data-home-persistence-contracts. Update Purpose after archive.
## Requirements
### Requirement: Dirty editors persist draft content under the app data directory
The system SHALL persist unsaved content for modified editors under `$XDG_DATA_HOME/lushtext/drafts/`. File-backed drafts MUST store draft content as UTF-8 text plus manifest metadata that includes the original path and backing-file mtime, and untitled tabs MUST store draft content using a stable generated draft ID.

#### Scenario: Autosave persists a file-backed dirty tab
- **WHEN** a file-backed tab remains modified long enough for the background draft sweep to run
- **THEN** the system writes or updates a draft file for that tab under the drafts directory
- **AND** the draft manifest records the tab's original path and original file mtime

#### Scenario: Autosave persists an untitled dirty tab
- **WHEN** an untitled tab remains modified long enough for the background draft sweep to run
- **THEN** the system writes or updates a draft file for that tab under the drafts directory
- **AND** the draft manifest records the untitled tab's generated draft ID without requiring a backing file path

#### Scenario: Window close flushes dirty drafts before exit
- **WHEN** the user closes the window while modified editors still have unsaved draft state
- **THEN** the system flushes those dirty drafts to the drafts directory before the window finishes closing
- **AND** crash recovery data remains available for a later restart

### Requirement: Session snapshots persist open-tab restore state independently from draft content
The system SHALL persist the global tab set to `$XDG_DATA_HOME/lushtext/session.json` independently from draft content. The persisted session MUST record each tab's file path or untitled draft ID, cursor position, scroll position, pinned state, and the selected tab index.

#### Scenario: Session snapshot stores restore position for a file-backed tab
- **WHEN** the app persists session state while a file-backed tab is open
- **THEN** the stored session entry includes that tab's file path, cursor position, scroll position, and pinned state

#### Scenario: Session snapshot stores an untitled tab by draft ID
- **WHEN** the app persists session state while an untitled tab is open
- **THEN** the stored session entry includes that tab's draft ID and restore position
- **AND** the system does not require a backing file path for the untitled tab to survive restart

### Requirement: Startup restore rebuilds tabs from session and draft state together
The system SHALL load session state, the draft manifest, and any prevalidated draft-restore outcomes together before rebuilding startup tabs. Matching file-backed drafts MUST restore into their reopened file-backed tabs, untitled drafts MUST restore from their stored draft IDs, and missing draft content MUST not block the tab itself from being restored. The system MUST NOT silently drop file-backed session entries solely because a backing path is temporarily unavailable during the preload step.

#### Scenario: Startup restore reapplies a matching file-backed draft
- **WHEN** startup restore rebuilds a file-backed tab whose recorded draft is still valid to restore
- **THEN** the tab reopens for that file path
- **AND** the restored draft content is applied after the file-backed editor is available

#### Scenario: Startup restore reapplies an untitled draft
- **WHEN** startup restore rebuilds an untitled tab whose draft ID still has saved draft content
- **THEN** the tab is recreated as an untitled editor
- **AND** the saved draft content is restored into that tab

#### Scenario: Missing draft content does not erase the tab restore attempt
- **WHEN** startup restore rebuilds a session tab whose draft manifest entry exists but the corresponding draft file is already missing
- **THEN** the system still restores the tab itself from session state
- **AND** the missing draft content is skipped without preventing the rest of startup restore

### Requirement: Draft cleanup waits for a safe user-visible resolution
The system SHALL keep draft recovery data until the document reaches a safe resolution such as successful save or explicit discard. A failed `Save As` or failed save-on-close path MUST leave the prior draft identity and draft content available for later recovery.

#### Scenario: Successful Save As cleans the old untitled draft identity
- **WHEN** an untitled document is successfully saved through `Save As`
- **THEN** the editor adopts the new file path
- **AND** the previous untitled draft recovery data is deleted

#### Scenario: Failed Save As keeps the prior draft available
- **WHEN** a `Save As` write fails for an untitled document that already has draft recovery data
- **THEN** the editor keeps its prior untitled identity
- **AND** the existing draft recovery data remains available

#### Scenario: Explicit discard removes draft recovery data
- **WHEN** the user explicitly discards a modified document's unsaved changes
- **THEN** the draft recovery data for that document is deleted
- **AND** reopening the document does not restore the discarded draft

#### Scenario: Close-discarded editors are not recreated during close flush
- **WHEN** the user explicitly discards selected modified editors during a close flow
- **THEN** the subsequent close-time draft flush does not recreate draft recovery files for those discarded editors

