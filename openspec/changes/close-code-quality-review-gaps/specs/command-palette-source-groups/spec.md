## ADDED Requirements

### Requirement: File-index construction enforces its working-set byte budget
Command-palette file indexing SHALL enforce a conservative O(1) byte ledger before retaining each output-vector allocation, output path, display path, canonical identity, hash-table bucket, visited-directory identity, pending directory, scan-page entry, or owned workspace-folder path. Peak construction ownership MUST stay within MAX_FILE_INDEX_BUILD_RETAINED_BYTES, defined as twice the 64 MiB installed-result budget, while completed output MUST remain within MAX_FILE_INDEX_RETAINED_BYTES. The ledger MUST release temporary charges when ownership ends and MUST stop with a typed RetainedByteLimit outcome before either applicable cap is exceeded.

#### Scenario: Long paths dominate a large traversal
- **WHEN** indexing encounters many long or deeply nested paths before reaching the item-count limit
- **THEN** each prospective retained owner is charged before insertion
- **AND** measured build high water remains at or below 128 MiB and installed output remains at or below 64 MiB

#### Scenario: Directory-only traversal grows pending state
- **WHEN** a broad tree contains many directories but few indexable files
- **THEN** visited, pending, scan-page, and workspace-root path ownership still consumes the byte ledger
- **AND** the traversal cannot bypass the cap merely because final output is small

#### Scenario: Scan batch approaches remaining scratch capacity
- **WHEN** the filesystem scanner would return a batch larger than the ledger's remaining build capacity
- **THEN** scanning honors a byte limit or yields a bounded batch that can be charged before retention
- **AND** scan entries are included in peak build metrics

#### Scenario: Next item would exceed the byte budget
- **WHEN** retaining a path or scan batch would cross the remaining byte budget
- **THEN** indexing stops before taking that ownership and reports RetainedByteLimit
- **AND** it returns the deterministic usable partial index already admitted

#### Scenario: Indexing is cancelled
- **WHEN** cancellation wins during a byte-bounded traversal
- **THEN** temporary ledger charges and owned traversal state are released through the existing worker lifecycle
- **AND** the cancelled generation cannot publish its partial index as current
