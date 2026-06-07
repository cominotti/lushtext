## ADDED Requirements

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

