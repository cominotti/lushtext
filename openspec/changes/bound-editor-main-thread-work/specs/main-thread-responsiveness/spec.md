## ADDED Requirements

### Requirement: Editor-sized GTK work uses live bounds and freshness
The system SHALL base periodic editor workflows on current constant-time buffer size information rather than only load-time file metadata. Any chunked snapshot that can overlap edits MUST carry enough editor, path, lifetime, and buffer-generation state to reject mixed or stale content before background work begins.

#### Scenario: Small file grows before periodic work
- **WHEN** an editor loaded from a small file grows beyond a workflow's live threshold before a periodic callback runs
- **THEN** the callback uses the current buffer classification
- **AND** it does not perform a direct whole-buffer copy based only on the old file size

#### Scenario: Buffer changes during chunked capture
- **WHEN** the user edits while a chunked periodic snapshot is being assembled
- **THEN** the completion is rejected as stale
- **AND** the mixed snapshot is not persisted or analyzed as a coherent document state

### Requirement: Search selection prefill is bounded before copy
The system SHALL prefill in-editor Find and Replace queries only from non-empty selections of at most 1,024 characters. The selection length MUST be checked before materializing its text, and an oversized selection MUST leave the current query unchanged while the search surface remains usable.

#### Scenario: Short selection prefills Find
- **WHEN** the user opens Find with a non-empty selection of at most 1,024 characters
- **THEN** the search entry is populated with that exact selection
- **AND** the query is focused and selected for editing

#### Scenario: Large selection does not allocate query text
- **WHEN** the user opens Find or Replace with a selection longer than 1,024 characters
- **THEN** the editor does not copy that selection into an owned query string
- **AND** the existing query remains unchanged and focused

#### Scenario: Unicode selection uses character count
- **WHEN** the selection contains multibyte Unicode characters
- **THEN** prefill eligibility is determined by character count rather than raw UTF-8 byte count
- **AND** an accepted selection is copied without splitting a character

### Requirement: Byte-compatible scans use established search primitives
The system SHALL use the existing `memchr` dependency for complete CR/LF candidate scanning in line-ending detection while preserving exact LF, CRLF, CR, mixed, and empty-input semantics. Optimized byte searches MUST remain in GTK-free service code and MUST be covered against the prior scalar behavior.

#### Scenario: Mixed line endings preserve counts
- **WHEN** decoded text contains LF, CRLF, and lone CR endings
- **THEN** the optimized scan counts each logical ending exactly once
- **AND** detection and suggested save style match the established policy

#### Scenario: CRLF does not count as lone LF
- **WHEN** a carriage return is immediately followed by a line feed
- **THEN** the pair contributes one CRLF ending
- **AND** its line feed is not counted again as LF

### Requirement: Unchanged Markdown allocation avoids embed traversal
The system SHALL cache the last processed effective Markdown text-column width together with the rendered-embed generation. A code-block width refresh MUST avoid traversing embedded widgets when both values are unchanged, while new embeds or a changed valid width MUST still receive the full width update.

#### Scenario: Repeated unchanged allocation settles cheaply
- **WHEN** GTK requests several deferred code-block refreshes with the same effective text-column width and unchanged embeds
- **THEN** only the first refresh traverses the rendered embeds
- **AND** later passes still complete readiness callbacks

#### Scenario: New code block at the same width
- **WHEN** Markdown rerenders new embedded code blocks without changing the text-column width
- **THEN** the changed embed generation forces width assignment for the new blocks
- **AND** the cache does not leave them at a narrow natural allocation

#### Scenario: Hidden preview reports zero width
- **WHEN** a preview is temporarily hidden or unallocated between valid presentations
- **THEN** the invalid width does not replace the last valid processed tuple
- **AND** the next valid allocation can repair every current code block

### Requirement: Focused responsiveness changes have regression evidence
The project SHALL add unit, property, GTK widget, visual-geometry, and performance coverage for live size classification, stale chunked capture, selection prefill boundaries, line-ending scan equivalence, and Markdown width-cache invalidation.

#### Scenario: Performance tests detect restored full scans
- **WHEN** responsiveness benchmarks run on large line-ending input and many embedded code blocks
- **THEN** they exercise the optimized candidate scan and unchanged-width fast path
- **AND** a regression to repeated scalar/full-embed scanning is observable
