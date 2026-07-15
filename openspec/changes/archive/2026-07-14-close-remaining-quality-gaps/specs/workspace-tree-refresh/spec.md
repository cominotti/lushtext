## MODIFIED Requirements

### Requirement: Watcher event delivery uses a bounded coalescing mailbox
The watcher backend SHALL normalize each raw tree-changing event outside GTK and merge it directly into one bounded pending notice without first retaining an app-unbounded debouncer queue or event vector. The notice SHALL contain either a capped unique path set or a conservative full-refresh marker; exceeding any retained-path bound, observing an ambiguous rename shape, or encountering producer-side lock contention MUST promote through constant-space state to full refresh rather than silently losing visible changes or creating backlog.

#### Scenario: Event burst stays below the path cap
- **WHEN** raw create, remove, and rename events produce a unique changed-path set within the configured cap
- **THEN** the mailbox retains one deduplicated bounded path notice
- **AND** GTK receives no access-only or duplicate paths

#### Scenario: Event burst exceeds the path cap
- **WHEN** unique tree-changing paths exceed the configured cap before GTK consumes them
- **THEN** the pending notice becomes a full-refresh marker
- **AND** additional raw events do not grow retained memory

#### Scenario: Producer outruns GTK polling
- **WHEN** raw backend callbacks arrive faster than the next GTK poll can consume them
- **THEN** they merge into the same bounded notice or constant-space full-refresh latch
- **AND** no backend debouncer vector or application channel backlog grows with event count

#### Scenario: Producer cannot acquire mailbox state
- **WHEN** a raw callback overlaps mailbox consumption and cannot immediately merge its event
- **THEN** it records a conservative full-refresh need in constant space without blocking GTK
- **AND** a later poll observes that refresh need

#### Scenario: Error and disconnect arrive with pending changes
- **WHEN** bounded changes, backend errors, or disconnection overlap
- **THEN** the mailbox preserves a bounded current error/disconnect state and conservative refresh need
- **AND** repeated identical errors do not grow retained state

## ADDED Requirements

### Requirement: Accepted workspace child caches rebuild in linear time
After bounded child-store reconciliation accepts a terminal mirror, the system SHALL rebuild sibling paths, item locations, and visible-path occurrence evidence in O(n) work for that mirror. The bulk rebuild MUST preserve duplicate-path accounting and lookup behavior without invoking per-row index shifting across already cached rows.

#### Scenario: Broad child store reaches the scan cap
- **WHEN** reconciliation accepts a directory mirror near the configured 10,000-row cap
- **THEN** cache rebuilding visits accepted and replaced cache entries only a bounded number of times
- **AND** it does not perform one cached-location scan for each inserted row

#### Scenario: Mirror contains duplicate and reordered identities
- **WHEN** accepted rows include duplicate paths, removals, insertions, and reordering
- **THEN** the bulk cache result matches the test-only full derivation oracle
- **AND** visible-path reference counts neither underflow nor retain removed-only occurrences

#### Scenario: Reconciliation is superseded before terminal acceptance
- **WHEN** a newer refresh invalidates the current reconciliation before its mirror is accepted
- **THEN** the stale mirror does not replace current cache state
- **AND** no partial bulk-cache commit remains visible
