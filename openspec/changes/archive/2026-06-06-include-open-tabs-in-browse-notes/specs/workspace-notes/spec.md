## MODIFIED Requirements

### Requirement: Users can browse notes within the current workspace scope
The system SHALL provide a `Browse Notes...` surface that lists workspace-scoped bookmarks, workspace notes, and document notes that fall inside the current shared workspace scope, plus a clearly separated `Open Tabs` section for saved open files that have bookmarks or document notes but fall outside that current scope. In a concrete workspace scope, normal workspace sections MUST be limited to that workspace. In `All workspaces`, normal workspace sections MUST aggregate bookmarks and notes across restored workspace roots. Supplemental open-tab rows MUST preserve their open-tab source explicitly and MUST NOT be represented as belonging to a fake workspace.

#### Scenario: Browse notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists that workspace's bookmarks and workspace note together with document notes that belong to files inside that workspace root
- **AND** bookmarks and notes from files outside that workspace are excluded from the workspace sections
- **AND** bookmarks or document notes attached to saved open tabs outside that workspace appear only in a dedicated `Open Tabs` section

#### Scenario: Browse notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser aggregates bookmarks, workspace notes, and document notes from every restored workspace root
- **AND** each workspace row preserves enough scope metadata for the user to tell which workspace it belongs to
- **AND** bookmarks or document notes attached to saved open tabs outside every restored workspace root appear only in the `Open Tabs` section

#### Scenario: Browse open-tab notes with no restored workspace
- **WHEN** no workspace roots are restored
- **AND** at least one saved open tab has a bookmark or an existing document note
- **AND** the user opens `Browse Notes...`
- **THEN** the browser opens successfully
- **AND** it lists the eligible rows in the `Open Tabs` section without requiring the user to add a workspace first

#### Scenario: No notes remain explicit when there are no workspaces or open-tab rows
- **WHEN** no workspace roots are restored
- **AND** no saved open tab has a bookmark or an existing document note
- **AND** the user opens `Browse Notes...`
- **THEN** the system reports that there are no browsable notes or bookmarks
- **AND** it does not create workspace, bookmark, or document-note data implicitly

#### Scenario: Open a workspace note from the notes browser
- **WHEN** the user activates a workspace-note row in `Browse Notes...`
- **THEN** the system opens that workspace note's surface
- **AND** the system does not require an active document tab for that workspace

#### Scenario: Open a bookmark from the notes browser
- **WHEN** the user activates a bookmark row in `Browse Notes...`
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line
