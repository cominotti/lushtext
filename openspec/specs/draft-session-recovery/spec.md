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

### Requirement: Session and draft manifests use the public v1 JSON envelope
The system SHALL persist `session.json` and `drafts/manifest.json` as supported v1 app-owned JSON envelopes. Runtime loading MUST require the correct document kind and supported version before reading session or draft-manifest payloads.

#### Scenario: Save session as v1
- **WHEN** the app persists session state
- **THEN** `session.json` is written as a pretty JSON envelope with the session document kind
- **AND** the payload stores file paths, untitled draft IDs, cursor position, scroll position, pinned state, and selected tab index

#### Scenario: Save draft manifest as v1
- **WHEN** the app persists draft manifest state
- **THEN** `drafts/manifest.json` is written as a pretty JSON envelope with the draft-manifest document kind
- **AND** the payload maps draft IDs to original paths, backing-file mtimes, and saved timestamps

### Requirement: Path-backed draft IDs use explicit stable hashing in v1
The system SHALL derive path-backed draft IDs in the v1 draft format with an explicit stable hashing algorithm rather than an implementation-dependent hasher. The algorithm MUST be documented in code and covered by deterministic tests.

#### Scenario: Same path yields same v1 draft ID
- **WHEN** the v1 draft ID helper receives the same absolute file path across process launches
- **THEN** it returns the same draft ID
- **AND** the result does not depend on process-randomized hash seeds

#### Scenario: Unsupported old draft manifest is preserved
- **WHEN** startup finds an unsupported pre-public draft manifest
- **THEN** the manifest is preserved through recovery diagnostics before replacement is allowed
- **AND** the runtime does not parse it through a permanent legacy manifest reader

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

### Requirement: Draft orphan cleanup reports typed conservative outcomes
The system SHALL inspect and execute draft orphan cleanup through typed results that distinguish confirmed manifest removals, confirmed file deletions, already-absent files, retained items, scan or status failures, deletion failures, manifest-write failures, and unfinished bounded work. Cleanup counts and success diagnostics MUST include only confirmed committed actions.

#### Scenario: Orphan draft file deletion succeeds
- **WHEN** a bounded trusted cleanup pass finds a draft body with no entry in the latest manifest and deletion succeeds
- **THEN** the outcome reports that file as confirmed deleted
- **AND** the cleaned count includes it exactly once

#### Scenario: Orphan draft file deletion fails
- **WHEN** deletion of a confirmed orphan draft body fails
- **THEN** the outcome retains the path with a typed failure
- **AND** the cleaned count does not include that file
- **AND** a later deferred pass may retry it

#### Scenario: Draft directory scan fails
- **WHEN** the drafts directory exists but cannot be scanned consistently
- **THEN** cleanup returns a scan failure without executing a partial destructive plan
- **AND** it does not report the directory as clean

### Requirement: Cleanup distinguishes missing artifacts from metadata errors
The system SHALL use recovery-aware path status for draft body presence decisions. Only a confirmed missing body MAY make its matching manifest entry eligible for cleanup; permission, metadata, symlink, or other I/O errors MUST retain the entry and produce diagnostics.

#### Scenario: Manifest body is confirmed missing
- **WHEN** path status confirms that a manifest entry's draft body does not exist
- **THEN** the entry becomes eligible for merge-safe manifest removal
- **AND** it is not treated as a body deletion

#### Scenario: Manifest body status is unreadable
- **WHEN** path status for a manifest entry fails because metadata cannot be inspected
- **THEN** the manifest entry remains present
- **AND** the cleanup outcome reports the status failure
- **AND** no destructive decision is inferred from that error

### Requirement: Cleanup revalidates latest recovery state before mutation
The system MUST revalidate an orphan candidate against the latest persisted manifest and current path status before deletion or manifest removal. A cleanup plan MUST carry entry fingerprints or equivalent generation evidence so a newer draft with the same ID cannot be removed by stale work.

#### Scenario: New manifest entry appears after inspection
- **WHEN** inspection identifies an orphan body but a new manifest entry for that draft ID is committed before execution
- **THEN** execution skips deleting the body
- **AND** the newer recovery entry remains intact

#### Scenario: Draft body reappears after missing-body inspection
- **WHEN** inspection identifies a manifest entry's body as missing but a body is written before manifest cleanup commits
- **THEN** execution rechecks the path and retains the manifest entry
- **AND** stale cleanup does not detach the new body from its recovery metadata

#### Scenario: Same draft ID has a newer generation
- **WHEN** the latest manifest contains the same draft ID with newer saved-generation metadata than the inspected fingerprint
- **THEN** cleanup does not remove the newer entry
- **AND** the stale plan is reported as skipped

### Requirement: Manifest cleanup is durable before visible acceptance
The system SHALL remove confirmed missing-body entries through the serialized durable manifest update path. The window MUST merge only removals that were committed to the latest manifest; a manifest-write failure MUST leave visible state retryable and MUST NOT be presented as successful cleanup.

#### Scenario: Manifest cleanup commit succeeds
- **WHEN** a confirmed missing-body fingerprint still matches and the durable manifest update succeeds
- **THEN** the outcome includes the exact committed removal
- **AND** the window may remove that matching entry from its current manifest state

#### Scenario: Manifest cleanup commit fails
- **WHEN** the durable manifest update for confirmed missing entries fails
- **THEN** the outcome reports no committed manifest removals for that update
- **AND** the window retains its entries and surfaces retryable recovery feedback

### Requirement: Orphan cleanup remains bounded and non-blocking
The system SHALL keep orphan inspection and mutation off the GTK main thread and SHALL inspect no more than the configured bounded entry count in one pass. If eligible work remains, the outcome MUST carry deterministic directory and manifest continuation state for a later deferred retry rather than looping synchronously. Directory continuation MUST advance beyond a stable retained prefix, MUST survive application restart through app-owned durable state, and MUST wrap only after a complete directory cycle. Ambiguous, failed, or concurrently changed entries MUST remain preserved and retryable.

#### Scenario: Damaged directory exceeds the scan bound
- **WHEN** a drafts directory contains more candidates than one cleanup pass permits
- **THEN** the pass inspects only the configured maximum
- **AND** its outcome durably records where a later bounded pass continues
- **AND** startup and the GTK main loop remain usable

#### Scenario: Stable live prefix precedes later orphans
- **WHEN** at least one full cleanup page of live or conservatively retained bodies sorts before orphan bodies
- **THEN** later deferred passes continue beyond that prefix
- **AND** the later orphan bodies eventually receive normal revalidation and cleanup consideration

#### Scenario: Application closes before a directory cycle completes
- **WHEN** cleanup records remaining directory work and the application exits before the next pass
- **THEN** the next trusted startup resumes from durable continuation state
- **AND** it does not restart indefinitely at the first directory page

#### Scenario: Directory changes invalidate a continuation boundary
- **WHEN** files are inserted, removed, or renamed around the saved continuation between passes
- **THEN** cleanup advances conservatively without deleting an unvalidated artifact
- **AND** a later wraparound cycle can reconsider entries skipped by concurrent ordering changes

#### Scenario: Retryable final-page failure has no cursor
- **WHEN** deletion, status inspection, or manifest persistence fails after a pass reaches its final directory and manifest page
- **THEN** the outcome retains the failed artifact and reports `has_more_work`
- **AND** the window schedules one backoff-controlled retry from the safe beginning even though no continuation cursor exists

#### Scenario: Repeated failures do not multiply cleanup work
- **WHEN** several deferred cleanup passes encounter the same persistent retryable failure
- **THEN** at most one follow-up timer and one cleanup worker remain owned by the window
- **AND** retry delay grows to a bounded cap while diagnostics and retained artifacts remain bounded

#### Scenario: Cleanup reaches a terminal complete outcome
- **WHEN** a trusted pass reports no directory continuation, no manifest continuation, and `has_more_work` is false
- **THEN** no further cleanup timer is scheduled
- **AND** prior retry backoff state is reset

#### Scenario: Untrusted startup state skips cleanup
- **WHEN** startup recovery cannot trust the draft manifest or cleanup continuation metadata
- **THEN** orphan cleanup is not executed from that state
- **AND** ambiguous draft bodies and metadata remain preserved for repair or diagnosis

### Requirement: Draft orphan cleanup has deterministic fault coverage
The project SHALL add service and integration tests for missing files, metadata errors, unreadable directories, bounded scans, delete failures, manifest-write failures, concurrent same-ID updates, and partial successful outcomes.

#### Scenario: Generated failure combinations never over-report cleanup
- **WHEN** tests inject combinations of scan, status, deletion, and manifest-write failures
- **THEN** every reported removal corresponds to a confirmed action
- **AND** ambiguous or failed artifacts remain represented as retained or retryable

### Requirement: Draft persistence uses a bounded snapshot-write pipeline
The system SHALL process dirty drafts through a bounded sequence of snapshot, durable body write, manifest commit, and snapshot release. A pass MUST retain no more than one full draft body plus bounded snapshot-chunk overhead at a time, while candidate and completion metadata MAY remain queued until the shared manifest commit.

#### Scenario: Several large dirty tabs do not accumulate bodies
- **WHEN** an autosave pass finds several dirty editors whose buffers require chunked snapshots
- **THEN** the system snapshots and writes one draft body before snapshotting the next full body
- **AND** completed draft strings are released instead of accumulating until the end of the pass

#### Scenario: Close flush uses the same bounded pipeline
- **WHEN** the user closes a window containing several modified editors
- **THEN** close-time draft safety processes those editors through the bounded pipeline
- **AND** the close remains pending until eligible bodies and the shared manifest commit finish

#### Scenario: Empty draft remains recoverable
- **WHEN** an eligible modified editor has an empty buffer at snapshot time
- **THEN** the pipeline persists an empty draft body and matching manifest entry
- **AND** it does not confuse empty content with a missing snapshot

### Requirement: Draft acceptance is generation-safe and retryable
The system MUST clear an editor's draft-dirty state only after the matching body write and manifest entry are durably accepted for the same draft ID and dirty generation. Snapshot, body-write, or manifest failures MUST leave affected editors eligible for retry, and an older completion MUST NOT clear newer dirty state.

#### Scenario: Edit arrives after snapshot
- **WHEN** an editor is modified again after its draft snapshot is captured but before the manifest commit completes
- **THEN** completion for the older generation does not clear the newer draft-dirty state
- **AND** a pending or later autosave pass remains eligible to capture the new content

#### Scenario: Draft body write fails
- **WHEN** the durable write of one draft body fails
- **THEN** that draft receives no accepted manifest update from the failed write
- **AND** its editor remains draft-dirty and retryable
- **AND** other candidates in the pass may continue safely

#### Scenario: Shared manifest commit fails
- **WHEN** one or more draft bodies were written but the final manifest update fails
- **THEN** none of those editor generations is marked successfully protected
- **AND** all affected editors remain retryable
- **AND** written bodies are preserved as recovery evidence rather than deleted as success cleanup

### Requirement: Automatic draft write and restore limits stay aligned
The system SHALL define a shared automatic per-draft recovery limit of `64 * 1024 * 1024` UTF-8 bytes for draft capture and draft read. A dirty buffer that exceeds the limit MUST NOT be presented as automatically protected, MUST remain draft-dirty, and MUST receive visible document-scoped feedback while normal explicit save workflows remain available.

#### Scenario: Draft exceeds the automatic limit during chunked capture
- **WHEN** a chunked draft snapshot grows beyond the automatic per-draft limit
- **THEN** capture stops after bounded chunk overhead
- **AND** no oversized draft body is committed as automatically recoverable
- **AND** the editor remains draft-dirty with visible recovery-limit feedback

#### Scenario: Draft exactly at the limit is accepted
- **WHEN** a draft's UTF-8 body is exactly the automatic per-draft limit and its writes succeed
- **THEN** the draft body and manifest entry are accepted
- **AND** the matching dirty generation may be cleared

### Requirement: Aggregate-preload skips restore lazily and safely
The system SHALL preserve the `64 * 1024 * 1024` aggregate eager startup preload cap and SHALL lazily read a size-eligible valid draft that was skipped only because admitting it would exceed that aggregate cap. Lazy restore MUST be serialized to bound peak body memory and MUST validate draft identity, backing-file freshness, editor lifetime, and restore generation before applying text.

#### Scenario: Second valid draft exceeds aggregate eager preload
- **WHEN** multiple individually eligible drafts cannot all fit within the aggregate eager preload cap
- **THEN** startup recreates every corresponding session tab
- **AND** drafts skipped only by the aggregate cap are read and applied through the bounded lazy restore path

#### Scenario: Lazy restore becomes stale
- **WHEN** a user closes, repurposes, edits, or saves an editor before its lazy draft read completes
- **THEN** the stale completion does not replace the current editor content
- **AND** the preserved draft remains governed by the normal recovery-resolution rules

#### Scenario: Lazy draft read fails
- **WHEN** a size-eligible aggregate-cap-skipped draft cannot be read lazily
- **THEN** the editor remains usable without applying partial text
- **AND** the user receives a recovery diagnostic
- **AND** the draft file and manifest entry are not deleted solely because the read failed

### Requirement: Draft pipeline reliability has layered coverage
The project SHALL add deterministic service, window, crash/restart, and scale coverage for pipeline memory bounds, generation races, body and manifest faults, close-time blocking, aggregate-cap lazy restore, and automatic-limit feedback.

#### Scenario: Scale fixture bounds retained draft bodies
- **WHEN** a test autosaves many large dirty tabs
- **THEN** instrumentation shows at most one complete body retained by the pipeline at a time
- **AND** every successfully accepted generation remains restorable

#### Scenario: Abrupt termination preserves accepted generations
- **WHEN** crash smoke terminates the app after one draft generation is accepted and another is still retryable
- **THEN** relaunch restores the accepted generation
- **AND** the artifacts do not claim the uncommitted generation was protected

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

### Requirement: Every asynchronous draft restore is freshness-gated
The system MUST associate every asynchronous untitled and file-backed draft restore with a ticket containing the expected draft identity, editor lifetime, file path, dirty or edit generation, load generation, and resolved manifest entry. A completion MUST apply recovered text, show restore-specific feedback, or delete stale recovery state only while every applicable ticket field remains current.

#### Scenario: User edits while an ordinary restore read is pending
- **WHEN** an editor changes after an asynchronous draft restore starts but before its completion reaches GTK
- **THEN** the older completion does not replace the current buffer
- **AND** it does not clear, delete, or reclassify the preserved draft

#### Scenario: File identity or load generation changes during restore
- **WHEN** an editor is reused, reloaded, renamed, or assigned a different path while draft resolution is pending
- **THEN** the completion is rejected before applying content or feedback
- **AND** recovery state belonging to the original request remains available for an eligible later restore

#### Scenario: Restore entry is replaced while resolution is pending
- **WHEN** the manifest entry for a draft changes after restore begins
- **THEN** the old resolution cannot apply or delete the newer entry
- **AND** the latest entry remains authoritative

### Requirement: Restored draft installation uses incoming-size policy and bounded ownership
The system SHALL classify draft-recovery history availability from the incoming restored UTF-8 body rather than the stale pre-restore buffer. It MUST avoid cloning a full incoming body solely to seed an ineligible local-history baseline, and large accepted bodies MUST use the bounded GTK replacement contract while their restore ticket remains current. Recovery content MUST remain non-editable and non-saveable until complete installation and restore-specific finalization.

#### Scenario: Small file has a large recovery draft
- **WHEN** a small backing-file buffer receives a valid recovery body in the large-file SaveOnly or history-unavailable tier
- **THEN** history policy is selected from the incoming body's size
- **AND** the system does not create an automatic full-body baseline that the incoming tier forbids

#### Scenario: Eligible restored body seeds local history
- **WHEN** the incoming recovery body remains eligible for a local-history baseline
- **THEN** ownership is transferred or shared without an avoidable second full UTF-8 clone
- **AND** the baseline represents the restored work rather than the stale backing-file buffer

#### Scenario: Draft restore ticket changes during installation
- **WHEN** an accepted large restore begins installing and its editor, path, dirty, load, or manifest generation changes
- **THEN** the replacement session stops before restore finalization
- **AND** the preserved recovery state is not cleared or presented as successfully applied

### Requirement: Draft mutations follow user-intent order
The system SHALL assign monotonically ordered intent to draft autosave upserts and draft deletion caused by Save, discard, close resolution, or stale-recovery cleanup. Draft body mutation and manifest mutation MUST execute through one ordered persistence workflow so an older autosave cannot recreate a draft after a later deletion intent.

#### Scenario: Save completes while autosave is writing
- **WHEN** an autosave operation is active and the corresponding editor successfully saves afterward
- **THEN** the Save-triggered deletion is ordered after the older autosave body and manifest work
- **AND** the final durable state contains neither the deleted manifest entry nor a resurrected draft body

#### Scenario: Autosave completion arrives after a deletion intent
- **WHEN** an older body write or completion reaches the coordinator after Save or discard has advanced the draft mutation generation
- **THEN** the obsolete upsert is rejected or ordered before the authoritative deletion
- **AND** it cannot become startup-restorable metadata

#### Scenario: New edit follows a completed deletion intent
- **WHEN** the user edits again after a Save-triggered draft deletion
- **THEN** the newer dirty generation may enqueue a later autosave upsert
- **AND** the ordering guard does not suppress legitimate recovery for the new unsaved work

#### Scenario: Ordered mutation fails
- **WHEN** a draft body, manifest upsert, or deletion operation fails
- **THEN** the workflow reports the failure without marking an uncommitted generation protected
- **AND** retry preserves the same intent order and the existing one-complete-body bound
