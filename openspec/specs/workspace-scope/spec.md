# workspace-scope Specification

## Purpose
Define the shared current workspace scope that the sidebar, search, palette indexing, Markdown preview, and workspace-aware note workflows all honor consistently.

## Requirements

### Requirement: Workspace scope is a shared app-wide concept
The system SHALL maintain one current workspace scope that all workspace-aware features share. The legal scope values MUST be either a specific workspace or the explicit aggregate scope `All workspaces`. A concrete workspace scope SHALL resolve to that workspace's ordered folder set, which MAY contain zero folders. The sidebar selector MUST update that shared scope instead of acting as a sidebar-only visibility filter.

#### Scenario: Selecting one workspace updates the shared scope
- **WHEN** the user selects a specific workspace from the sidebar scope selector
- **THEN** that workspace becomes the current shared workspace scope
- **AND** workspace-aware features observe that same selection
- **AND** those features resolve the scope through the selected workspace's ordered folder set

#### Scenario: Selecting All workspaces activates the aggregate scope
- **WHEN** the user selects `All workspaces` from the sidebar scope selector
- **THEN** the current shared workspace scope becomes the aggregate `All workspaces` scope
- **AND** workspace-aware features treat that aggregate scope as the active scope until the user chooses a concrete workspace
- **AND** aggregate consumers resolve workspace folders in workspace order and then folder order

#### Scenario: Selecting an empty workspace remains a valid scope
- **WHEN** the user selects a workspace that contains zero folders
- **THEN** that workspace becomes the current shared workspace scope
- **AND** workspace-aware features report empty workspace-folder coverage instead of rebasing to another workspace

### Requirement: Search and palette results honor the current workspace scope
The system SHALL scope workspace-aware search and command palette workspace-file indexing to the current shared workspace scope. When a concrete workspace is selected, workspace-aware search and the command palette workspace file group MUST use that workspace's ordered folder set. When `All workspaces` is selected, they MUST aggregate the folder sets of every restored workspace in workspace order and then folder order. Search results and command-palette workspace-indexed file rows MUST be de-duplicated by canonical file identity when overlapping folders cover the same document. The command palette MAY also present matching open file-backed tabs in a separate `Open Tabs` group before workspace-indexed file results; that active-document group MUST NOT change the current workspace scope and MUST NOT cause open files to be relabeled as workspace-indexed results.

#### Scenario: Search stays inside the selected workspace folders
- **WHEN** a concrete workspace is the current shared scope and the user runs a workspace search
- **THEN** search results come only from files covered by that workspace's folder set
- **AND** files outside that workspace's folders do not appear in the results

#### Scenario: Overlapping folders produce one search result per file
- **WHEN** the selected workspace contains folders `/repo` and `/repo/src`
- **AND** `/repo/src/main.rs` matches the user's workspace search
- **THEN** the search result list shows `/repo/src/main.rs` only once
- **AND** the displayed workspace context uses the earliest covering folder by folder order

#### Scenario: Palette workspace group stays inside the selected workspace folders
- **WHEN** a concrete workspace is the current shared scope and the user runs a file-palette lookup
- **THEN** workspace-indexed palette results come only from files covered by that workspace's folder set
- **AND** matching open file-backed tabs outside that workspace may appear only in the separate `Open Tabs` group

#### Scenario: Aggregate scope searches across all workspace folders
- **WHEN** `All workspaces` is the current shared scope and the user runs a workspace search or file-palette lookup
- **THEN** the workspace-aware feature searches across the folders of every restored workspace
- **AND** results from multiple workspaces may appear together
- **AND** duplicate canonical files covered by overlapping folders appear at most once per result surface

#### Scenario: Empty workspace folder set yields no workspace-indexed rows
- **WHEN** a concrete workspace with zero folders is the current shared scope
- **AND** the user runs workspace search or a file-palette lookup
- **THEN** no workspace-indexed file rows are produced for that workspace
- **AND** the feature presents an explicit empty-folder-scope state when user-visible feedback is needed

### Requirement: Workspace-aware note flows honor the current workspace scope
The system SHALL scope workspace-aware note, bookmark, and folder-note workflows to the current shared workspace scope. A concrete workspace scope MUST limit those flows to that workspace's ordered folder set. The aggregate scope MUST include the folder sets of all restored workspaces. Browser-style note flows MUST de-duplicate document-level rows by canonical saved-file identity when overlapping folders cover the same document, while folder-note rows remain one row per folder-note identity.

#### Scenario: Note browsers stay inside the selected workspace folders
- **WHEN** a concrete workspace is the current shared scope and the user opens a workspace-scoped bookmark or notes browser
- **THEN** the browser lists folder notes for folders in that workspace together with document notes and bookmarks for files covered by that workspace's folders
- **AND** records from other workspaces are excluded from normal workspace sections
- **AND** saved open-tab records outside the selected workspace appear only in the dedicated `Open Tabs` section when eligible

#### Scenario: Overlapping folders produce one document-level notes row
- **WHEN** the selected workspace contains overlapping folders that both cover one saved file
- **AND** that file has a document note or bookmark
- **THEN** the notes browser shows that document-level entry only once in the workspace-scoped section
- **AND** the row's primary folder context is chosen by the workspace folder order

#### Scenario: Aggregate scope browses across all workspace folders
- **WHEN** `All workspaces` is the current shared scope and the user opens a workspace-scoped bookmark or notes browser
- **THEN** the browser includes data covered by every restored workspace folder set
- **AND** the browser is not silently narrowed to one workspace or one folder

#### Scenario: Folder-note actions require a deterministic folder target
- **WHEN** a note workflow opens or edits a folder note
- **THEN** the target is one explicit workspace folder
- **AND** the system does not infer a folder from a workspace merely because that workspace is selected when multiple folders are available

### Requirement: Workspace creation and removal update shared scope predictably
The system SHALL update the current shared workspace scope predictably when workspaces are created or removed. Creating a workspace MUST select that new workspace as the current shared scope, whether it starts with zero folders or one initial folder. Removing the currently selected workspace MUST fall back to the explicit aggregate `All workspaces` scope instead of silently rebasing to another concrete workspace. Adding, removing, or reordering folders inside the selected workspace MUST refresh workspace-aware consumers without changing the selected workspace scope.

#### Scenario: Creating a workspace selects it immediately
- **WHEN** the user creates a new workspace from the sidebar shell
- **THEN** the new workspace becomes the current shared workspace scope
- **AND** workspace-aware features update to use that workspace's folder set

#### Scenario: Removing the selected workspace falls back to All workspaces
- **WHEN** the user removes the currently selected workspace while other workspaces still exist
- **THEN** the current shared workspace scope becomes `All workspaces`
- **AND** the app does not silently choose a different concrete workspace instead

#### Scenario: Folder mutation keeps the same workspace selected
- **WHEN** the user adds, removes, or reorders folders inside the currently selected workspace
- **THEN** that workspace remains the current shared workspace scope
- **AND** search, palette, notes, bookmarks, and Markdown preview consumers observe the updated folder set
