## ADDED Requirements

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
