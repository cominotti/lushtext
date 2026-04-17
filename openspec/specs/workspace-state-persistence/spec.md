# workspace-state-persistence Specification

## Purpose
Persist and restore the user's named workspace collections under the LushText app data directory so workspace roots and active-workspace model state survive restart safely and predictably.

## Requirements
### Requirement: Workspaces persist named roots and active workspace reference in the app data directory
The system SHALL persist workspace state to `$XDG_DATA_HOME/lushtext/workspaces.json`. The persisted state MUST keep each workspace's stable ID, display name, directory or file roots, and the current active workspace reference stored by the workspace model.

#### Scenario: Relaunch restores persisted workspaces
- **WHEN** the user creates or edits one or more workspaces and later restarts the app
- **THEN** the same workspace names and roots are restored from persisted workspace state
- **AND** the previously active workspace reference is restored when it still exists

#### Scenario: Directory and file roots persist together
- **WHEN** a workspace contains both directory roots and standalone file roots
- **THEN** both kinds of entries are preserved in persisted workspace state
- **AND** reopening the app restores the same mixed root set

### Requirement: Workspace state always restores to a usable active workspace model
The system SHALL restore workspace state to a usable active workspace model even when persisted state is empty or the previously active workspace no longer exists. A fresh or empty state MUST yield a default `New Workspace`, and removing the active workspace MUST rebase activity onto the first remaining workspace when one exists.

#### Scenario: Empty state restores to a default workspace
- **WHEN** the app starts with no persisted workspaces
- **THEN** the workspace model creates a default `New Workspace`
- **AND** that workspace becomes active

#### Scenario: Removing the active workspace rebases activity
- **WHEN** the user removes the currently active workspace while other workspaces still exist
- **THEN** persisted workspace state no longer references the removed workspace
- **AND** the first remaining workspace becomes active

### Requirement: Persisted workspace roots are deduplicated and latest-state wins
The system SHALL avoid persisting duplicate roots inside the same workspace, and rapid workspace mutations MUST persist the latest in-memory state rather than an older snapshot.

#### Scenario: Adding the same root twice does not persist a duplicate
- **WHEN** the user adds the same directory or file root to a workspace more than once
- **THEN** persisted workspace state keeps only one copy of that root in that workspace

#### Scenario: Rapid workspace edits restore the newest state after restart
- **WHEN** the user makes several workspace mutations in quick succession and then restarts the app after persistence completes
- **THEN** the restored workspace state reflects the latest completed in-memory state
- **AND** an older debounced snapshot does not overwrite the newer one
