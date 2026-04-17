# draft-restore-validation Specification

## Purpose
TBD - created by archiving change draft-mtime-validation. Update Purpose after archive.
## Requirements
### Requirement: File-backed draft restore validates backing file freshness
The system SHALL compare a file-backed draft's recorded backing-file mtime with the current backing-file mtime before restoring recovered draft content into the editor. The system MUST restore the draft only when that comparison still shows the file unchanged.

#### Scenario: Restore a file-backed draft when the backing file is unchanged
- **WHEN** LushText opens a file-backed tab that has stored draft content and the current backing-file mtime matches the mtime recorded with that draft
- **THEN** the system restores the draft content into the editor
- **AND** the editor shows the normal draft-restored warning for that file

#### Scenario: Skip a file-backed draft when the backing file changed externally
- **WHEN** LushText opens a file-backed tab that has stored draft content and the current backing-file mtime differs from the mtime recorded with that draft
- **THEN** the system keeps the current on-disk file contents in the editor
- **AND** it does not apply the stored draft content
- **AND** the editor shows a warning that the draft was not restored because the file changed externally

### Requirement: Stale file-backed drafts are discarded after a confirmed mismatch
The system SHALL delete file-backed draft recovery data after skipping restore because the backing file changed externally, so the same stale draft is not offered again on later opens.

#### Scenario: Reopen a file after a stale draft was skipped
- **WHEN** a file-backed draft was previously skipped because the backing file mtime no longer matched
- **THEN** opening that same file again does not restore the stale draft
- **AND** the earlier stale-draft warning does not appear again unless a newer draft was written afterward

### Requirement: Untitled draft recovery remains unchanged
The system SHALL continue to restore untitled drafts without file-backed freshness validation.

#### Scenario: Restore an untitled draft
- **WHEN** LushText restores an untitled tab that has stored draft content but no backing file path
- **THEN** the system restores that untitled draft content
- **AND** file-backed mtime validation does not block the untitled restore

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

