## ADDED Requirements

### Requirement: Text a whole-buffer replacement installs is owed a draft
When a bounded replacement completes and leaves a modified tab holding text its draft does not (draft recovery, local-history Restore, Undo Restore), the system SHALL mark the tab draft-dirty and advance its dirty generation, as an edit does. An autosave pass captured before the replacement MUST NOT clear the flag, so the next pass writes the installed text.

#### Scenario: Restore after the draft settled
- **WHEN** a modified file-backed tab's draft is written and no longer dirty
- **AND** the user restores a local-history snapshot larger than the direct-replacement threshold
- **THEN** after the replacement completes the tab is draft-dirty
- **AND** the next autosave pass persists the restored text as the tab's draft

#### Scenario: A pass captured before the restore commits later
- **WHEN** an autosave pass captures the pre-restore buffer
- **AND** its manifest commit completes during or after the restore's replacement
- **THEN** the commit does not leave the restored tab clean
- **AND** the persisted draft is the pre-restore text until the next pass writes the restored text

#### Scenario: Undo Restore owes a draft too
- **WHEN** the user invokes Undo Restore and its replacement completes
- **THEN** the tab is draft-dirty
- **AND** the next autosave pass persists the pre-restore text

#### Scenario: Replacements that leave nothing owed
- **WHEN** a save-formatting mirror-back or a memory eviction replaces the buffer
- **THEN** the tab's draft-dirty state is not changed by the replacement
