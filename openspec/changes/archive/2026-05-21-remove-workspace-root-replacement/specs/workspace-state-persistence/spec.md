## MODIFIED Requirements

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
