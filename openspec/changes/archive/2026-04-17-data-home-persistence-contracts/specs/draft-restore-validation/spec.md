## ADDED Requirements

### Requirement: File-backed draft validation is applied consistently during preload and direct open
The system SHALL use the same file-backed draft restore decision during startup preload and later direct file-open checks. Matching drafts MUST restore, stale drafts MUST warn once and be cleaned up, and unavailable or missing drafts MUST be skipped in both paths without inventing a second restore policy.

#### Scenario: Startup preload uses the same matching-draft validation outcome
- **WHEN** startup preload evaluates a file-backed draft whose recorded mtime still matches the backing file
- **THEN** the draft is treated as safe to restore
- **AND** the later editor restore uses that prevalidated restore outcome

#### Scenario: Direct open uses the same stale-draft validation outcome
- **WHEN** a later file-open check evaluates a file-backed draft whose recorded mtime no longer matches the backing file
- **THEN** the draft is treated as stale
- **AND** the same skip-and-warn behavior used during startup restore applies

### Requirement: Confirmed stale draft cleanup removes both content and manifest state
The system SHALL remove both the stale file-backed draft file and its manifest entry after a confirmed stale-draft mismatch so no leftover recovery record can be offered again by either restore path.

#### Scenario: Stale draft cleanup removes file and manifest entry
- **WHEN** a file-backed draft is skipped because its backing file changed externally
- **THEN** the stale draft file is deleted from the drafts directory
- **AND** the corresponding manifest entry is removed from persisted draft state
