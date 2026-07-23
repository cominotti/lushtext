## ADDED Requirements

### Requirement: Workspace search bounds traversal identity ownership
Each workspace-search generation SHALL normalize its immutable ordered folder scope into a bounded traversal plan before scanning. A single effective traversal root and multiple roots proven disjoint MUST NOT retain one visited-file identity per scanned file. Exact duplicate and covered roots MUST be scanned only once while result attribution preserves the original configured folder precedence. Any unresolved alias fallback that still requires per-file identity tracking MUST enforce explicit entry and conservative path-byte limits and MUST terminate with typed incomplete-search feedback before either limit is exceeded.

#### Scenario: Single-root no-match search visits a huge tree
- **WHEN** one workspace folder contains more files than the result cap but none match the query
- **THEN** search retained identity state remains independent of the number of visited files
- **AND** cancellation, progress, and normal completion preserve their existing semantics

#### Scenario: Overlapping roots cover the same file
- **WHEN** ordered workspace folders include duplicates, descendants, or canonical aliases that cover the same file
- **THEN** the normalized traversal plan avoids duplicate scanning where coverage is resolved
- **AND** an admitted result is attributed according to the first configured folder that owned it before normalization

#### Scenario: Alias identity cannot be resolved completely
- **WHEN** unavailable or uncanonicalizable roots require fallback file-identity tracking to prevent duplicate results
- **THEN** the fallback ledger retains no more than its documented entry and path-byte budgets
- **AND** reaching either budget stops with explicit incomplete-search feedback rather than silently publishing a complete result

### Requirement: Decoded document and recovery bodies retain off-GTK disposal ownership
Every document-sized decoded file body and recovered draft body SHALL reserve bounded plain-data disposal capacity before the body crosses from worker or aggregate preload ownership onto GTK. The reservation MUST remain attached through weak-owner checks, generation validation, direct or sliced buffer installation, cancellation, teardown, and eligible accepted-baseline transfer. A stale, rejected, superseded, ineligible, or otherwise terminal body MUST perform its final plain-Rust destruction on the admitted disposal worker, and document-sized bodies MUST NOT use the statically-small unreserved sentinel path.

#### Scenario: File-load completion loses its editor
- **WHEN** a supported large decoded file body reaches main-loop completion after the editor weak reference can no longer be upgraded
- **THEN** the guarded result is rejected without destroying the body in GTK dispatch
- **AND** its final destructor runs through the pre-admitted disposal worker

#### Scenario: Sliced file installation is cancelled
- **WHEN** a newer load generation or editor teardown cancels a large installation between GTK slices
- **THEN** the installer releases transient load admission exactly once
- **AND** the remaining guarded decoded body is finally destroyed off GTK

#### Scenario: Draft body becomes stale before replacement
- **WHEN** an eager or lazy recovered draft body loses ticket freshness before or during bounded replacement
- **THEN** no partial or terminal restored state is published
- **AND** the body's guard survives until worker-side final destruction

#### Scenario: Accepted body seeds a clean baseline
- **WHEN** file-load or draft policy retains the accepted installed body as an eligible local-history baseline
- **THEN** ownership is transferred without a full-body clone or unguarded unwrap
- **AND** later baseline replacement or editor teardown still performs final plain-data destruction off GTK
