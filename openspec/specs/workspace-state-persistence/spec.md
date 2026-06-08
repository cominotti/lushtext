# workspace-state-persistence Specification

## Purpose
Persist and restore named workspaces as ordered folder sets, together with the current workspace scope, under the LushText app data directory so startup behavior stays predictable and recoverable.

## Requirements

### Requirement: Workspace domain terminology uses folders, not singular roots
The system SHALL treat `workspace folder` as the domain term for folders that belong to a workspace. User-facing strings, persisted model fields, public model/service helpers, tests, fixtures, comments, README text, and OpenSpec contracts MUST NOT describe the workspace domain as a singular root. The term `root` MAY remain only in internal tree/traversal APIs where it means a displayed tree root row, a GTK model root, or a legacy migration payload field rather than a workspace-owned folder.

#### Scenario: Domain naming audit passes after implementation
- **WHEN** the implementation is complete
- **THEN** code, tests, fixtures, resources, specs, and documentation contain no user-facing or domain-level singular-root references
- **AND** any remaining `root` references are limited to internal tree/traversal vocabulary or legacy migration diagnostics
- **AND** each remaining internal use is either obviously tree-local from its module name or documented by nearby code/tests

#### Scenario: UI strings use folder terminology
- **WHEN** the app presents workspace membership, folder notes, folder actions, or workspace persistence recovery feedback
- **THEN** visible labels, tooltips, dialogs, status messages, and action names use `folder` terminology
- **AND** they do not describe workspaces as singular roots

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

### Requirement: Workspace state uses the public v1 JSON envelope
The system SHALL persist `workspaces.json` as a supported v1 app-owned JSON envelope. Runtime loading MUST require the workspace document kind and supported version before reading workspace data.

#### Scenario: Persist workspace state as v1
- **WHEN** workspace state is saved
- **THEN** `workspaces.json` is written as a pretty JSON envelope with the workspace document kind
- **AND** its payload stores the current workspace scope and ordered workspace folder sets

#### Scenario: Load supported workspace state
- **WHEN** startup loads `workspaces.json` with the workspace document kind and supported version
- **THEN** the sidebar restores the workspace names, folder sets, and current scope from the payload
- **AND** missing selected-workspace targets still normalize to `All workspaces`

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

### Requirement: Unsupported workspace JSON is preserved before reset
The system SHALL treat pre-public bare workspace JSON, wrong-kind envelopes, unsupported versions, malformed files, and unsupported file kinds as recovery metadata problems. The system MUST preserve that metadata before writing a default v1 workspace file.

#### Scenario: Unsupported workspace file resets safely
- **WHEN** startup finds unsupported workspace metadata and preservation succeeds
- **THEN** the original metadata is available in quarantine or diagnostic evidence
- **AND** the app may continue with an empty v1 workspace state

#### Scenario: Workspace preservation failure blocks overwrite
- **WHEN** startup finds unsupported workspace metadata and preservation fails
- **THEN** the app does not overwrite the original workspace file
- **AND** it reports that workspace recovery could not safely replace the file

### Requirement: Workspace state always restores to a usable active workspace model
The system SHALL restore workspace state to a usable current workspace scope even when persisted state is empty, a workspace contains zero folders, or the previously selected workspace no longer exists. A fresh or empty state MUST preserve zero persisted workspaces and the intentional empty sidebar shell. If the previously selected workspace no longer exists, the restored scope MUST fall back to the explicit aggregate scope `All workspaces`.

#### Scenario: Empty state restores to the empty sidebar shell
- **WHEN** the app starts with no persisted workspaces
- **THEN** the restored state contains no visible workspace sections
- **AND** the sidebar remains in its intentional empty-shell form instead of creating a placeholder workspace section

#### Scenario: Missing selected workspace falls back to All workspaces
- **WHEN** the app restores persisted workspaces but the previously selected workspace no longer exists
- **THEN** persisted workspace state no longer points at the missing workspace as the current scope
- **AND** the restored scope becomes `All workspaces`

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
