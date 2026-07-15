## MODIFIED Requirements

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

#### Scenario: Untrusted startup state skips cleanup
- **WHEN** startup recovery cannot trust the draft manifest or cleanup continuation metadata
- **THEN** orphan cleanup is not executed from that state
- **AND** ambiguous draft bodies and metadata remain preserved for repair or diagnosis

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
