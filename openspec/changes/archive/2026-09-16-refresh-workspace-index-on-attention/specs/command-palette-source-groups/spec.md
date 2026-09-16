## ADDED Requirements

### Requirement: The file index reflects externally changed workspace files
The command-palette file index SHALL reflect files created, removed, or renamed under the current workspace scope by processes other than LushText, at any depth the index covers, without requiring the user to change workspace scope, toggle a visibility setting, or restart the application. Freshness MUST be established by the attention-moment refresh, and MUST apply the index's existing coverage rule — excluded directory names and workspace entry visibility — exactly as a full build does.

#### Scenario: A branch switch introduces new files
- **WHEN** a version-control operation outside LushText replaces the working tree of a workspace folder with one containing files the index has never seen, including files in directories that were never expanded in the sidebar
- **AND** the user returns to the window and opens the command palette
- **THEN** the new files are searchable in the palette

#### Scenario: A file created from a terminal in a deep subdirectory
- **WHEN** a file is created by another process in a directory several levels below a workspace folder that has never been expanded in the sidebar
- **AND** the user returns to the window and opens the command palette
- **THEN** that file is searchable in the palette

#### Scenario: Externally removed files stop appearing
- **WHEN** an indexed file is removed from disk by another process
- **AND** an attention-triggered refresh settles
- **THEN** the palette no longer offers that file

#### Scenario: Excluded directories stay excluded after refresh
- **WHEN** external changes occur inside a directory the index excludes by name or by workspace entry visibility
- **THEN** the refreshed index does not admit files from that directory

#### Scenario: A refresh cannot overwrite a newer index
- **WHEN** a newer workspace-scope build is accepted while an attention-triggered refresh is in flight
- **THEN** the older refresh cannot install its result as current

### Requirement: In-app saves update the file index
Saving a document to a path inside the current workspace scope SHALL update the file index, including the first save of an untitled document and a Save As to a new path. The admitted entry MUST carry the canonical identity of the file the durable write actually resolved, so that a symlinked save is indexed as its target. The index MUST NOT depend on a later refresh to learn about a file the application itself wrote.

#### Scenario: Saving an untitled document indexes it
- **WHEN** an untitled document is saved to a path inside a workspace folder
- **THEN** that path is searchable in the command palette without waiting for a refresh

#### Scenario: Save As to a new workspace path indexes the new path
- **WHEN** a document is saved under a new name inside a workspace folder
- **THEN** the new path is searchable in the command palette

#### Scenario: A symlinked save indexes the resolved target
- **WHEN** a document is saved through a path that is a symbolic link, so the durable write resolves and writes the link's target
- **THEN** the index admits the resolved target's canonical identity
- **AND** activating the palette result opens the file whose bytes were written

#### Scenario: Saving outside the workspace scope does not index
- **WHEN** a document is saved to a path outside every folder in the current workspace scope
- **THEN** the index does not admit that path

#### Scenario: The workspace-membership test does not block the save terminal
- **WHEN** a save completes and the index must decide whether the written path belongs to a workspace folder
- **THEN** the decision is a path comparison against the configured workspace folders that requires no filesystem access
- **AND** the canonical identity is resolved on the index mutation worker, not on the GTK thread
