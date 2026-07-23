## ADDED Requirements

### Requirement: Residual responsiveness and test-isolation boundaries have direct evidence
The project SHALL maintain deterministic tests and performance fixtures for incremental refresh-state capture, one-active/one-latest bookmark previews, cancellable large-file processing, and target-scoped Replace/Undo fault injection. Evidence MUST assert actual work, retained ownership, cancellation, terminal publication, and registry state rather than relying only on elapsed time or self-reported outcomes.

#### Scenario: Targeted refresh runs in a large materialized tree
- **WHEN** one affected directory is refreshed among many expanded and materialized rows
- **THEN** instrumentation proves refresh preparation and expansion bookkeeping touch only affected state
- **AND** a full derivation oracle proves the final expansion and selection state remains correct

#### Scenario: Bookmark selections supersede delayed workers
- **WHEN** many closed-file bookmarks are selected while excerpt workers are deliberately delayed
- **THEN** evidence records an active high-water of one, a pending high-water of one, cooperative cancellation, and latest-only publication
- **AND** dialog teardown drains or cancels all retained preview ownership

#### Scenario: Large-file work is cancelled at varied stages
- **WHEN** representative large ASCII, multibyte UTF-8, UTF-16, and fallback-encoded fixtures are cancelled during classification, decoding, or analysis
- **THEN** work counters prove bounded cancellation progress and exact-once transient-capacity release
- **AND** uncancelled runs remain byte-for-text and metadata equivalent to the reference path

#### Scenario: Different Undo targets race in parallel
- **WHEN** target-scoped after-metadata hooks are registered for different temporary files and Undo operations interleave or run concurrently
- **THEN** each operation consumes only its own one-shot hook
- **AND** no unconsumed registration leaks into another test

#### Scenario: Near-limit memory evidence is requested
- **WHEN** the opt-in performance lane exercises a supported file near the configured load ceiling
- **THEN** it records fixture size, encoding, cancellation progress, transient ownership, resident-memory context, profile, and environment metadata
- **AND** default pull-request validation does not gain an absolute host-sensitive RSS or timing gate
