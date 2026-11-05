## MODIFIED Requirements

### Requirement: Accepted workspace child caches rebuild in linear time
After bounded child-store reconciliation accepts a terminal mirror, the system SHALL rebuild sibling paths, item locations, and visible-path occurrence evidence in O(n) work for that mirror. The bulk rebuild MUST preserve duplicate-path accounting and lookup behavior without invoking per-row index shifting across already cached rows.

#### Scenario: Broad child store reaches the scan cap
- **WHEN** reconciliation accepts a directory mirror near the configured 16,384-row cap (`MAX_DIR_ENTRIES`, a presentation cap rounded up from 10,000 by `power-of-two-thresholds`, so every directory `main` showed in full still shows in full)
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
