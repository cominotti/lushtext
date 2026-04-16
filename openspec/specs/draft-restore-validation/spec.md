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

