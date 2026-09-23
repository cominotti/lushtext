## MODIFIED Requirements

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
- **AND** the editor shows a warning that the unsaved edits were kept, because the file changed externally, naming local history when it accepted them and the set-aside location otherwise
- **AND** when the edits went to local history, the warning offers an action that opens the local-history browser for that file

### Requirement: Stale file-backed drafts are discarded after a confirmed mismatch
The system SHALL retire a file-backed draft from the draft journal after skipping its restore because the backing file changed externally, so that the same stale draft is not offered again on later opens. Before retiring it, the system SHALL preserve the stale body recoverably in two layers. First, it SHALL durably copy the body byte-identically into the drafts set-aside location. That location is never pruned, and the body counts as preserved only once this copy is durable. Then, whenever local-history policy accepts the file, the system SHALL also record the body as a local-history snapshot of the backing file, using the existing while-editing snapshot origin and the draft's save time. Local history accepts the file when the file and body are within size policy, snapshot normalization leaves the text byte-identical, and retention keeps the back-dated snapshot. The preserved body MUST outlive the draft journal entry and MUST be reachable through a user-visible recovery surface. Preservation MUST NOT change any persisted local-history format.

#### Scenario: Reopen a file after a stale draft was skipped
- **WHEN** a file-backed draft was previously skipped because the backing file mtime no longer matched
- **THEN** opening that same file again does not restore the stale draft
- **AND** the earlier stale-draft warning does not appear again unless a newer draft was written afterward

#### Scenario: Stale edits are kept in local history
- **WHEN** a stale file-backed draft is skipped for a file within local-history policy
- **THEN** the drafts set-aside location holds a byte-identical copy of the stale draft body
- **AND** the file's local history contains a while-editing snapshot whose content is byte-identical to the stale draft body and whose time is the draft's save time
- **AND** the user can preview and restore it through the local-history browser, reversibly

#### Scenario: Local-history retention cannot lose stale edits
- **WHEN** local-history retention later prunes the back-dated snapshot of a stale draft
- **THEN** the byte-identical copy in the drafts set-aside location remains

#### Scenario: Stale edits for a file outside local-history policy are set aside
- **WHEN** a stale file-backed draft is skipped for a file that local history does not accept
- **THEN** the body is preserved byte-identically in the drafts set-aside location
- **AND** the warning names that location

#### Scenario: Preservation failure keeps the draft
- **WHEN** preserving a stale draft body fails
- **THEN** the draft body and its manifest entry remain in place
- **AND** the draft is not offered for restore over the changed file, and a diagnostic reports the preservation failure

### Requirement: Confirmed stale draft cleanup removes both content and manifest state
The system SHALL remove the stale draft's body from the drafts directory and its manifest entry from persisted draft state, once its content has been durably preserved, so that no leftover draft-journal record can be offered again by either restore path.

#### Scenario: Stale draft cleanup removes file and manifest entry after preservation
- **WHEN** a file-backed draft is skipped because its backing file changed externally and its content was durably preserved
- **THEN** the stale draft file is deleted from the drafts directory
- **AND** the corresponding manifest entry is removed from persisted draft state
