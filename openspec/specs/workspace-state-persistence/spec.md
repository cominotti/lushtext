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

### Requirement: Legacy persisted workspaces migrate safely to single-root form
The system SHALL normalize legacy persisted workspace data into the single-root contract before that data is rendered or re-saved. Extra directory roots from one legacy workspace MUST become additional sibling workspaces. Standalone file roots MUST be promoted to parent-directory workspaces when their parent directory can be determined.

#### Scenario: Legacy multi-root workspace is split into single-root workspaces
- **WHEN** persisted workspace data from an older version contains multiple directory roots in one workspace
- **THEN** the loader normalizes that data into multiple single-root workspaces before rendering it
- **AND** each resulting workspace owns exactly one root directory

#### Scenario: Legacy file root is promoted to a parent-directory workspace
- **WHEN** persisted workspace data from an older version contains a standalone file root whose parent directory can be determined
- **THEN** the loader normalizes that legacy entry into a workspace rooted at that parent directory
- **AND** the normalized persisted state no longer treats the standalone file as a supported workspace root
