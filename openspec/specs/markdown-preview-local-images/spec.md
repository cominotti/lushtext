# markdown-preview-local-images Specification

## Purpose
Define how Markdown preview resolves local image destinations without network fetches, including file-relative paths and workspace-relative paths from the current ordered workspace folder coverage.
## Requirements
### Requirement: Markdown preview renders local image blocks
The system SHALL render Markdown image syntax that resolves to local files, including file-relative and workspace-relative paths, as native read-only image blocks within the preview flow. Workspace-relative paths MUST resolve against the current shared workspace scope's ordered folder set: a concrete workspace uses its folders in user-defined order, while `All workspaces` uses restored workspace order and then folder order. When multiple workspace folders could resolve the same image destination, the first loadable match by that order is used.

#### Scenario: Render a file-relative local image
- **WHEN** the user previews a Markdown file whose image destination resolves relative to the current Markdown file
- **THEN** the preview shows the image as a native block in the surrounding document flow
- **AND** the raw Markdown image syntax does not remain visible in the rendered preview

#### Scenario: Render a workspace-relative local image from a selected workspace
- **WHEN** the user previews a Markdown file whose image destination resolves from one of the current workspace's folders
- **THEN** the preview shows the resolved image as a native block
- **AND** the image appears in the correct place relative to surrounding paragraphs and other supported blocks

#### Scenario: Folder order resolves ambiguous workspace-relative images
- **WHEN** the selected workspace contains folders A and B in that order
- **AND** the same workspace-relative image destination exists under both folders
- **THEN** the preview resolves the image from folder A
- **AND** reordering B before A changes the primary workspace-relative match after preview context refreshes

#### Scenario: Empty workspace folder set falls back explicitly
- **WHEN** the current shared scope is a concrete workspace with zero folders
- **AND** the preview encounters a workspace-relative image destination that cannot resolve file-relative
- **THEN** the preview shows the normal unresolved-image fallback
- **AND** it does not silently resolve the image through another workspace

### Requirement: Markdown preview shows explicit fallback for unsupported or unresolved image targets
The system MUST not fetch remote images or silently drop unsupported or unresolved local image targets. When an image destination cannot be resolved or loaded, the preview SHALL show an explicit fallback state in document flow.

#### Scenario: Show fallback for a missing local image
- **WHEN** the preview encounters Markdown image syntax whose local destination does not resolve to a loadable file
- **THEN** the preview shows an explicit fallback state for that image in document flow
- **AND** the raw Markdown image syntax does not remain visible in the rendered preview

#### Scenario: Do not fetch a remote image target
- **WHEN** the preview encounters Markdown image syntax whose destination is a remote URL
- **THEN** the preview shows an explicit unsupported-image fallback state
- **AND** the system does not perform a remote fetch
