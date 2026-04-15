# markdown-preview-local-images Specification

## Purpose
TBD - created by archiving change markdown-preview-followups. Update Purpose after archive.
## Requirements
### Requirement: Markdown preview renders local image blocks
The system SHALL render Markdown image syntax that resolves to local files, including file-relative and workspace-relative paths, as native read-only image blocks within the preview flow.

#### Scenario: Render a file-relative local image
- **WHEN** the user previews a Markdown file whose image destination resolves relative to the current Markdown file
- **THEN** the preview shows the image as a native block in the surrounding document flow
- **AND** the raw Markdown image syntax does not remain visible in the rendered preview

#### Scenario: Render a workspace-relative local image
- **WHEN** the user previews a Markdown file whose image destination resolves from the current workspace roots
- **THEN** the preview shows the resolved image as a native block
- **AND** the image appears in the correct place relative to surrounding paragraphs and other supported blocks

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

