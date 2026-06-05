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

### Requirement: Startup draft preload is bounded and non-destructive
The system SHALL bound eager draft-content preloading during startup restore. Before loading draft bodies into memory, the system MUST inspect draft file sizes and enforce a total preload cap of `64 * 1024 * 1024` bytes. Drafts skipped because of the cap MUST NOT be deleted or removed from the manifest solely because eager preload was bounded.

#### Scenario: Oversized draft is skipped without deletion
- **WHEN** startup restore finds a draft file whose size would exceed the eager preload limit
- **THEN** the draft body is not loaded into the preload map
- **AND** the draft file and manifest entry remain available for later recovery handling

#### Scenario: Total preload cap prevents startup memory spike
- **WHEN** startup restore finds multiple draft files whose combined size exceeds the total preload cap
- **THEN** the system preloads only drafts within the cap
- **AND** the remaining drafts are reported as skipped without deleting recovery data

#### Scenario: Skipped draft does not block session tab restore
- **WHEN** a session tab has draft recovery data that was not eagerly preloaded because of the cap
- **THEN** startup restore still attempts to recreate the tab
- **AND** the absence of preloaded body text does not erase the tab or draft identity

### Requirement: Startup restore reports malformed session and draft metadata
The system SHALL load session state and draft manifest state through recovery-aware metadata handling. If either metadata file is malformed, unreadable, or an unsupported file kind, startup MUST preserve the problematic metadata when possible, return diagnostics to the window, and avoid silently treating the failure as ordinary empty restore state.

#### Scenario: Malformed session is preserved and reported
- **WHEN** `session.json` exists but cannot be parsed during startup restore
- **THEN** the original session metadata is preserved through quarantine or left untouched when quarantine fails
- **AND** the window receives a recovery diagnostic instead of silently restoring an empty session

#### Scenario: Malformed draft manifest is preserved and reported
- **WHEN** `drafts/manifest.json` exists but cannot be parsed during startup restore
- **THEN** the original manifest metadata is preserved through quarantine or left untouched when quarantine fails
- **AND** the window receives a recovery diagnostic instead of silently treating all drafts as absent

#### Scenario: Unrelated valid restore state still loads
- **WHEN** session metadata is malformed but some draft files or sidecar recovery data remain valid
- **THEN** startup preserves and loads the valid recoverable subset
- **AND** the malformed session diagnostic does not erase unrelated recovery data

### Requirement: Draft recovery attempts conservative manifest repair
The system SHALL attempt conservative draft manifest repair when the manifest is missing or malformed and draft files remain present. Repair MUST only reconstruct entries that can be derived without guessing user intent and MUST keep unclassified draft files preserved with diagnostics.

#### Scenario: Untitled draft file is repairable
- **WHEN** a draft file has an untitled draft identifier that can be mapped to a session entry or safe untitled recovery candidate
- **THEN** the system includes that draft in repaired or partial restore state
- **AND** the draft file is not deleted solely because the manifest was missing or malformed

#### Scenario: Ambiguous draft file is preserved
- **WHEN** a draft file exists but its original path or tab identity cannot be determined safely
- **THEN** the system preserves the draft file
- **AND** it reports that the draft could not be automatically restored

#### Scenario: Repair writes are durable
- **WHEN** the system writes a repaired draft manifest
- **THEN** the repaired manifest is written through the durable JSON path
- **AND** failed repair writes leave the original draft files eligible for later recovery

### Requirement: First-dirty draft autosave reduces the crash-loss window
The system SHALL schedule a short first-dirty draft autosave after an editor first becomes draft-dirty in an editing cycle, in addition to the existing periodic autosave timer. The first-dirty path MUST reuse the existing chunked snapshot, background write, generation guard, and retry behavior.

#### Scenario: First dirty edit schedules early draft write
- **WHEN** an editor transitions from not draft-dirty to draft-dirty
- **THEN** the system schedules an early autosave pass without waiting for the next periodic five-second tick
- **AND** the draft remains eligible for the normal periodic timer if the early pass fails

#### Scenario: Large first-dirty buffer snapshots in chunks
- **WHEN** the first-dirty autosave needs text from a buffer that exceeds the synchronous snapshot threshold
- **THEN** the text is captured through the chunked main-loop snapshot path
- **AND** the UI is not blocked by one unbounded buffer copy

#### Scenario: In-flight autosave coalesces first-dirty request
- **WHEN** a first-dirty autosave is requested while another autosave batch is already in flight
- **THEN** the system marks an autosave rerun pending
- **AND** it does not start a concurrent conflicting draft manifest write

### Requirement: Session save failures are visible and retryable
The system SHALL track failed debounced and close-time session saves as retryable session persistence failures. The newest session generation MUST remain eligible for retry, and the user MUST receive visible feedback when close-time session persistence cannot be confirmed.

#### Scenario: Debounced session save failure remains retryable
- **WHEN** a debounced session save fails
- **THEN** the window records that session persistence is dirty or failed
- **AND** a later session-changing event retries with the newest session snapshot

#### Scenario: Close-time session save failure is visible
- **WHEN** close-time session persistence fails after document and draft safety has completed
- **THEN** the user receives visible warning feedback that tab layout may not restore
- **AND** the failure is logged with the data directory and generation context

#### Scenario: Older session save cannot overwrite newer accepted state
- **WHEN** an older debounced session save completes after a newer accepted generation
- **THEN** the older save is ignored
- **AND** retry state remains tied to the newest unsaved generation

### Requirement: Startup recovery diagnostics are surfaced after restore
The system SHALL surface grouped startup diagnostics after session and draft restore completes. The diagnostics MUST distinguish restored work, skipped stale drafts, corrupted metadata, repaired metadata, and unavailable metadata without blocking unaffected restored tabs.

#### Scenario: Grouped recovery warning after startup
- **WHEN** startup restore completes with one or more recovery diagnostics
- **THEN** the window shows a grouped warning or status message
- **AND** restored tabs that were unaffected remain usable

#### Scenario: Stale draft warning remains document-scoped
- **WHEN** a file-backed draft is skipped because the backing file changed externally
- **THEN** the affected editor still receives the document-scoped stale draft warning
- **AND** the grouped startup warning does not replace that more specific editor notification

#### Scenario: Diagnostics clear after successful recovery
- **WHEN** a later startup loads session and draft metadata without diagnostics
- **THEN** stale diagnostics from an earlier startup are not shown again

### Requirement: Draft and session reliability has layered automated coverage
The project SHALL add deterministic service, integration, widget, and smoke coverage for malformed metadata, first-dirty autosave, session-save retry, and real restart behavior.

#### Scenario: Service tests cover malformed restore metadata
- **WHEN** service tests load malformed session JSON and malformed draft manifests
- **THEN** the load result includes diagnostics and preserves the original metadata

#### Scenario: Widget tests cover visible session-save warning
- **WHEN** a test forces close-time session save failure after draft safety succeeds
- **THEN** the user-visible warning is shown
- **AND** the window does not claim session persistence succeeded

#### Scenario: Crash smoke covers recovered drafts and session
- **WHEN** the crash recovery smoke lane runs against draft and session state
- **THEN** it verifies recovery across a real process termination and relaunch
