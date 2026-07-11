## ADDED Requirements

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
