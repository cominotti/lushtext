# workspace-state-persistence Specification

## Purpose
Persist and restore the user's named single-root workspaces and current workspace scope under the LushText app data directory so startup behavior stays predictable and recoverable.
## Requirements
### Requirement: Workspaces persist named roots and active workspace reference in the app data directory
The system SHALL persist workspace state to `$XDG_DATA_HOME/lushtext/workspaces.json`. Each persisted workspace MUST keep a stable ID, a display name, and exactly one root directory. Persisted workspace state MUST also keep the user's current workspace scope selection as either a concrete workspace or the explicit aggregate scope `All workspaces`.

#### Scenario: Relaunch restores persisted workspaces
- **WHEN** the user creates or edits one or more workspaces and later restarts the app
- **THEN** the same workspace names and root directories are restored from persisted workspace state
- **AND** the previously selected workspace scope is restored when it still exists

#### Scenario: Each workspace persists one directory root
- **WHEN** a workspace is persisted after this change
- **THEN** that workspace stores exactly one root directory
- **AND** persisted workspace state does not preserve mixed directory or standalone-file root collections as a supported shape

### Requirement: Workspace state uses the public v1 JSON envelope
The system SHALL persist `workspaces.json` as a supported v1 app-owned JSON envelope. Runtime loading MUST require the workspace document kind and supported version before reading workspace data.

#### Scenario: Persist workspace state as v1
- **WHEN** workspace state is saved after the format hardening change
- **THEN** `workspaces.json` is written as a pretty JSON envelope with the workspace document kind
- **AND** its payload stores the current workspace scope and single-root workspace list

#### Scenario: Load supported workspace state
- **WHEN** startup loads `workspaces.json` with the workspace document kind and supported version
- **THEN** the sidebar restores the workspace names, roots, and current scope from the payload
- **AND** missing selected-workspace targets still normalize to `All workspaces`

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
The system SHALL restore workspace state to a usable current workspace scope even when persisted state is empty or the previously selected workspace no longer exists. A fresh or empty state MUST preserve zero persisted workspaces and the intentional empty sidebar shell. If the previously selected workspace no longer exists, the restored scope MUST fall back to the explicit aggregate scope `All workspaces`.

#### Scenario: Empty state restores to the empty sidebar shell
- **WHEN** the app starts with no persisted workspaces
- **THEN** the restored state contains no visible workspace sections
- **AND** the sidebar remains in its intentional empty-shell form instead of creating a placeholder workspace section

#### Scenario: Missing selected workspace falls back to All workspaces
- **WHEN** the app restores persisted workspaces but the previously selected workspace no longer exists
- **THEN** persisted workspace state no longer points at the missing workspace as the current scope
- **AND** the restored scope becomes `All workspaces`

### Requirement: Persisted workspace roots are deduplicated and latest-state wins
The system SHALL persist at most one root directory per workspace, and rapid workspace mutations MUST persist the latest in-memory state rather than an older snapshot. Persisted workspace roots MUST change only through supported workspace lifecycle operations that add, remove, or rename workspaces, or update the current workspace scope. The system MUST NOT support replacing one existing workspace's root directory in place.

#### Scenario: Removing and adding a different root creates a different workspace entry
- **WHEN** the user removes a workspace and then creates a workspace for a different root directory
- **THEN** persisted workspace state stores the new root as its own workspace entry
- **AND** the removed workspace's root is not retained as an additional root inside that new workspace

#### Scenario: Rapid workspace edits restore the newest state after restart
- **WHEN** the user makes several workspace mutations in quick succession and then restarts the app after persistence completes
- **THEN** the restored workspace state reflects the latest completed in-memory state
- **AND** an older debounced snapshot does not overwrite the newer one
