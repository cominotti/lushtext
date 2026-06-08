## ADDED Requirements

### Requirement: Workspace domain terminology uses folders, not singular roots
The system SHALL treat `workspace folder` as the domain term for folders that belong to a workspace. User-facing strings, persisted model fields, public model/service helpers, tests, fixtures, comments, README text, and OpenSpec contracts MUST NOT describe the workspace domain as a singular `workspace root`. The term `root` MAY remain only in internal tree/traversal APIs where it means a displayed tree root row or a GTK model root rather than a workspace-owned folder.

#### Scenario: Domain naming audit passes after implementation
- **WHEN** the implementation is complete
- **THEN** code, tests, fixtures, resources, specs, and documentation contain no user-facing or domain-level `workspace root` references
- **AND** any remaining `root` references are limited to internal tree/traversal vocabulary or legacy migration diagnostics
- **AND** each remaining internal use is either obviously tree-local from its module name or documented by nearby code/tests

#### Scenario: UI strings use folder terminology
- **WHEN** the app presents workspace membership, folder notes, folder actions, or workspace persistence recovery feedback
- **THEN** visible labels, tooltips, dialogs, status messages, and action names use `folder` terminology
- **AND** they do not mention singular workspace roots

### Requirement: Workspaces persist ordered folder sets
The system SHALL persist each workspace as a stable workspace ID, a display name, and an ordered list of workspace folders. Each workspace folder MUST persist a stable folder ID and its configured folder path. A workspace MAY persist zero folders. The current workspace scope selection MUST continue to persist as either one concrete workspace ID or the explicit aggregate scope `All workspaces`.

#### Scenario: Relaunch restores ordered workspace folders
- **WHEN** a workspace contains multiple folders in a user-defined order and the app restarts after persistence completes
- **THEN** the workspace is restored with the same workspace ID and name
- **AND** the same folder IDs and folder paths are restored in the same order
- **AND** the previously selected workspace scope is restored when the workspace still exists

#### Scenario: Empty workspace persists as an empty folder set
- **WHEN** a workspace has no folders and workspace state is saved
- **THEN** the workspace remains present in `workspaces.json`
- **AND** its persisted folder list is empty
- **AND** restarting the app restores that zero-folder workspace without creating a fake folder

### Requirement: Workspace folder uniqueness is scoped to one workspace
The system SHALL prevent adding the same canonical folder more than once inside one workspace. The same canonical folder MAY belong to more than one workspace. Folder uniqueness checks MUST use canonical folder identity when available and MUST avoid silently creating duplicates when canonicalization fails.

#### Scenario: Duplicate folder is rejected inside one workspace
- **WHEN** a workspace already contains a folder whose canonical path is `/repo`
- **AND** the user tries to add `/repo` again through the folder chooser, path alias, or symlink that resolves to the same canonical folder
- **THEN** the workspace folder list is unchanged
- **AND** the user receives recoverable feedback that the folder already belongs to that workspace

#### Scenario: Same folder may belong to another workspace
- **WHEN** Workspace A contains canonical folder `/repo`
- **AND** Workspace B does not contain `/repo`
- **AND** the user adds `/repo` to Workspace B
- **THEN** Workspace B accepts the folder
- **AND** Workspace A remains unchanged

#### Scenario: Overlapping folders are allowed
- **WHEN** a workspace contains `/repo`
- **AND** the user adds `/repo/src`
- **THEN** the workspace accepts `/repo/src` as a distinct folder
- **AND** both folders remain visible and ordered in the workspace folder list

### Requirement: Workspace state migrates supported single-folder payloads safely
The system SHALL load the current supported v1 workspace-state payload that stores `workspaces[].root` and migrate each workspace to the new folder-set payload in memory. Saving after migration MUST write only the new folder-set shape. Unsupported pre-public bare workspace JSON, wrong-kind envelopes, unsupported versions, malformed files, and unsupported file kinds MUST continue to be preserved through recovery metadata before replacement.

#### Scenario: Current single-folder workspace migrates to one folder
- **WHEN** startup loads a supported v1 workspace-state envelope whose workspace payload contains one `root` path
- **THEN** the restored workspace contains exactly one workspace folder for that path
- **AND** the workspace ID, workspace name, and current scope are preserved
- **AND** the migrated folder has a stable folder ID for future reorder/remove operations

#### Scenario: Migrated state saves only the new folder-set shape
- **WHEN** a migrated single-folder workspace is saved after startup
- **THEN** `workspaces.json` is written as a supported workspace-state envelope
- **AND** each workspace payload stores `folders`
- **AND** the old singular `root` payload field is not written

#### Scenario: Pre-public entries shape remains unsupported
- **WHEN** startup finds a pre-public bare workspace JSON document with `entries`
- **THEN** the document is treated as unsupported recovery metadata
- **AND** the original metadata is preserved before any replacement is allowed
- **AND** the runtime does not silently interpret that shape as the new folder-set format

### Requirement: Workspace folder mutations persist latest state
The system SHALL persist workspace folder additions, removals, reorders, workspace renames, workspace removals, and scope changes through the existing latest-state-wins persistence behavior. Rapid folder mutations MUST NOT allow an older debounced snapshot to overwrite a newer in-memory workspace folder order or membership.

#### Scenario: Rapid folder edits restore newest order
- **WHEN** the user adds folders, removes one folder, and reorders the remaining folders in quick succession
- **AND** persistence completes before restart
- **THEN** restarting the app restores the latest completed in-memory folder list and order
- **AND** no older debounced snapshot restores a removed folder or stale order

#### Scenario: Removing selected workspace still falls back to All workspaces
- **WHEN** the currently selected workspace is removed
- **THEN** the persisted current scope falls back to `All workspaces`
- **AND** the app does not silently select another concrete workspace

## REMOVED Requirements

### Requirement: Workspaces persist named roots and active workspace reference in the app data directory
**Reason**: A workspace is no longer exactly one folder/root. The persisted contract is replaced by ordered workspace folder sets.
**Migration**: Supported current single-root payloads migrate to one-folder workspace sets; unsupported pre-public formats remain preserved recovery metadata.

### Requirement: Persisted workspace roots are deduplicated and latest-state wins
**Reason**: The uniqueness and latest-state contract now applies to per-workspace folder sets rather than one root directory per workspace.
**Migration**: Existing tests and callers must move to folder-set helpers and folder mutation coverage.
