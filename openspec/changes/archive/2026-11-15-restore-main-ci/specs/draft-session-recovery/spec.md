## ADDED Requirements

### Requirement: Text a whole-buffer replacement installs is owed a draft
When a bounded replacement (local-history Restore, Undo Restore, a set-aside draft opened in a new tab) completes and leaves a modified tab holding text its draft does not hold, the system SHALL mark the tab draft-dirty, advance its dirty generation and project its modified indicator, as an edit does, whether the tab was modified or clean before. An autosave pass captured before the replacement MUST NOT clear the flag.

#### Scenario: Restore after the draft settled
- **WHEN** a modified file-backed tab's draft is written and no longer dirty
- **AND** the user restores a local-history snapshot, either below or above the direct-replacement threshold
- **THEN** after the replacement completes the tab is draft-dirty
- **AND** the next autosave pass persists the restored text as the tab's draft

#### Scenario: Restore into a clean tab
- **WHEN** an unmodified file-backed tab restores a local-history snapshot
- **THEN** after the replacement completes the tab is modified and draft-dirty
- **AND** its tab title shows the modified indicator
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

### Requirement: A replacement the draft already holds owes nothing
A replacement whose installed text the draft already holds (startup draft recovery), or that leaves the tab unmodified (save-formatting mirror-back, memory eviction), MUST NOT mark the tab draft-dirty.

#### Scenario: Replacements that leave nothing owed
- **WHEN** startup draft recovery installs a draft body, a save-formatting mirror-back replaces the buffer, or a memory eviction clears it
- **THEN** the replacement does not mark the tab draft-dirty
- **AND** after the mirror-back the saved tab is unmodified and not draft-dirty
- **AND** no autosave pass rewrites the recovered draft before the user edits it
- **AND** a recovered draft's tab, which is modified, shows the modified indicator
