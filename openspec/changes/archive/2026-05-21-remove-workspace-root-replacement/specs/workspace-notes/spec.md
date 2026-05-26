## MODIFIED Requirements

### Requirement: Workspace-note persistence follows workspace-root identity
The system SHALL persist workspace notes under app data using a stable identity derived from the workspace root's canonical path. Renaming a workspace label MUST keep the same workspace note. Renaming the workspace root through LushText's in-app rename workflow MUST migrate the workspace note to the renamed root identity. Removing a workspace MUST NOT delete the workspace note for that root, so re-adding the same root MUST restore the same workspace note. Adding a different root MUST use that different root's own workspace-note identity.

#### Scenario: Renaming a workspace label keeps the same workspace note
- **WHEN** the user renames a workspace label without changing its root directory
- **THEN** the existing workspace note remains attached to that workspace root
- **AND** the note content does not reset

#### Scenario: In-app root rename preserves a workspace note
- **WHEN** the user renames the workspace root directory through LushText's in-app rename workflow
- **THEN** the persisted workspace note is migrated to the renamed root identity
- **AND** reopening that renamed workspace restores the same workspace note

#### Scenario: Remove and re-add the same root restores the same workspace note
- **WHEN** the user removes a workspace that has a workspace note and later adds the same root directory again
- **THEN** the system restores the same workspace note for that root
- **AND** the note does not depend on the old workspace slot identifier

#### Scenario: Adding a different root uses that root's workspace-note identity
- **WHEN** the user removes one workspace and later creates a workspace for a different root directory
- **THEN** the different root uses its own workspace-note identity
- **AND** the previous root keeps its existing workspace note data for a future remove-and-readd flow
