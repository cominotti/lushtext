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

### Requirement: Markdown render context shares immutable path ownership
Each Markdown render generation SHALL own one immutable shared path context containing the document path and workspace-folder snapshot. Cloning render context for plans, table cells, embeds, or worker handoff MUST clone constant-size shared ownership rather than every PathBuf, without weakening generation freshness.

#### Scenario: A maximum-size table clones render context
- **WHEN** Markdown planning creates many table-cell builders for one accepted generation
- **THEN** every builder shares the same immutable path storage
- **AND** retained path ownership does not multiply by cell count

#### Scenario: Workspace scope changes during rendering
- **WHEN** workspace scope changes after a render generation captured its context
- **THEN** the generation continues to resolve against its immutable original snapshot or is superseded
- **AND** it cannot mix old Markdown content with newly selected workspace paths

### Requirement: Relative image candidates are resolved lazily after admission
Markdown preview SHALL apply generation, image-count, and retained-byte admission before expanding workspace-relative image candidates. An admitted relative image MUST be resolved off GTK in deterministic file-relative then workspace-folder order, checking at most one candidate at a time instead of retaining a full candidate vector.

#### Scenario: Render contains more than four local images
- **WHEN** one accepted render contains more embeds than the four-image admission limit
- **THEN** excess embeds become deterministic accessible placeholders before candidate expansion
- **AND** they allocate no workspace-folder candidate graph

#### Scenario: Admitted relative image has many possible bases
- **WHEN** an admitted image is resolved with many workspace folders
- **THEN** the worker joins and checks candidates one at a time in documented precedence order
- **AND** retained candidate-path ownership remains bounded independently of folder count

#### Scenario: An early candidate resolves
- **WHEN** the document-relative or an earlier workspace-relative candidate is valid
- **THEN** resolution stops without constructing later candidates
- **AND** existing canonical identity, size, decode, and accessibility checks still apply

#### Scenario: Image work becomes stale
- **WHEN** render generation or page lifetime changes during lazy resolution
- **THEN** later candidate checks and projection stop
- **AND** any admitted plain payload retires off GTK while the current preview remains unchanged

#### Scenario: No candidate resolves
- **WHEN** every admitted candidate is missing, unsafe, oversized, or unreadable
- **THEN** the embed renders the existing accessible fallback state
- **AND** diagnostic ownership remains bounded
