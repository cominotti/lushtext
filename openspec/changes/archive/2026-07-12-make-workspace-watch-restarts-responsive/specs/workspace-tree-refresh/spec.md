## ADDED Requirements

### Requirement: Materialized watch targets update incrementally
The system SHALL maintain the deduplicated workspace watch-target set as flattened tree rows and expansion states change. Updating the set MUST do work proportional to affected rows rather than rewalking the complete flattened tree, and overlapping visible rows MUST retain correct target reference counts.

#### Scenario: Expand one nested directory
- **WHEN** the user expands one directory in a large workspace tree
- **THEN** that directory's non-recursive target is added through the affected row update
- **AND** the section does not rescan every flattened row to derive the new set

#### Scenario: Collapse a branch with expanded descendants
- **WHEN** a branch containing several expanded descendant directories is collapsed
- **THEN** targets contributed only by the removed flattened descendants are released
- **AND** targets still contributed by another overlapping visible row remain active

#### Scenario: Effective target set does not change
- **WHEN** tree signals update row presentation without changing the deduplicated materialized target set
- **THEN** the watcher generation does not restart
- **AND** the current backend watcher remains installed

#### Scenario: Zero-folder workspace has no targets
- **WHEN** a workspace contains no configured folders and no materialized rows
- **THEN** the incremental target set is empty
- **AND** no fake or fallback filesystem target is created

### Requirement: Watcher lifecycle work stays off the GTK main thread
The system SHALL perform watcher creation, target registration, replacement teardown, and stale-handle disposal outside GTK callbacks. GTK MUST receive only an owned watcher result or typed failure and MUST keep the current rendered tree and controls usable while replacement is in progress.

#### Scenario: Slow watcher startup
- **WHEN** the watcher backend takes noticeable time to create or register many materialized targets
- **THEN** startup work runs on a background worker
- **AND** sidebar input, repaint, scrolling, and manual Refresh remain schedulable

#### Scenario: Slow watcher teardown
- **WHEN** dropping the old backend watcher blocks while its resources shut down
- **THEN** teardown occurs off the GTK main thread
- **AND** replacing or hiding the workspace section does not synchronously stall the UI

#### Scenario: Empty target set retires a watcher
- **WHEN** the latest materialized target set becomes empty
- **THEN** the old watcher is retired outside the GTK callback
- **AND** no poll source remains installed for that section

### Requirement: Watcher replacement is generation-safe
The system SHALL associate each effective target snapshot with a monotonically advancing generation. A startup success or failure MUST affect the section only if its generation and section lifetime remain current; stale watcher handles MUST be disposed off-thread.

#### Scenario: Scope changes during watcher startup
- **WHEN** a watcher is starting and the workspace filter, focus folder, folders, or expansion state produces a newer target generation
- **THEN** the older watcher is never installed
- **AND** the newest generation remains authoritative

#### Scenario: Stale startup succeeds
- **WHEN** an obsolete watcher startup returns successfully after a newer generation exists
- **THEN** the obsolete handle is disposed outside the GTK callback
- **AND** it does not replace or clear the current watcher error state

#### Scenario: Stale startup fails
- **WHEN** an obsolete watcher startup reports an error after a newer generation exists
- **THEN** the old error is ignored
- **AND** the current section does not show feedback for obsolete targets

#### Scenario: Section is destroyed during startup
- **WHEN** a workspace section is disposed before its background watcher startup completes
- **THEN** no poll source or watcher is installed on the destroyed section
- **AND** any returned watcher is disposed off-thread

### Requirement: Responsive watcher replacement preserves refresh semantics
The system SHALL retain materialized-scope non-recursive watching, access-noise filtering, overlapping-folder updates, stable tree reconciliation, recoverable warnings, and manual Refresh while watcher lifecycle changes are pending or failed.

#### Scenario: Current-generation startup fails
- **WHEN** the watcher backend cannot register a current materialized path
- **THEN** the existing rendered tree remains mounted and usable
- **AND** the section exposes one recoverable automatic-refresh warning
- **AND** manual Refresh remains reachable

#### Scenario: Overlapping folder target remains valid
- **WHEN** the same canonical directory is materialized through overlapping workspace folders
- **THEN** one deduplicated backend target represents all current row contributions
- **AND** removing one contribution does not stop watching while another remains

#### Scenario: Constrained sidebar during restart
- **WHEN** watcher replacement occurs while a workspace with long paths is shown in a narrow sidebar
- **THEN** header controls and Refresh remain visible
- **AND** only the file-tree item region scrolls
- **AND** no horizontal scrollbar or transient fake row is introduced

### Requirement: Workspace watcher responsiveness has layered coverage
The project SHALL add pure target-set tests, service integration tests, GTK widget tests, accessibility/geometry checks, and performance fixtures for empty, representative, many-target, overlapping, unreadable, slow-backend, stale-completion, reorder, expansion, collapse, and constrained-sidebar states.

#### Scenario: Incremental state matches full oracle
- **WHEN** generated sequences of folder, row, expansion, collapse, refresh, and reorder events are applied
- **THEN** the incremental deduplicated set matches a test-only full derivation oracle after every step
- **AND** reference counts never underflow or retain removed-only targets

#### Scenario: Many expanded rows avoid GTK full scans
- **WHEN** a performance fixture changes one target in a tree with many expanded rows
- **THEN** target bookkeeping touches only the affected splice or row state
- **AND** watcher construction and disposal time is excluded from the GTK main-thread interval
