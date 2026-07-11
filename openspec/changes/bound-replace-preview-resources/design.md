## Context

Workspace search retains at most 10,000 bounded match lines, and Replace Preview generation already runs through `spawn_blocking_then` with query/panel generation checks. Preview construction nevertheless clones each original line, builds a replaced line, duplicates literal replacement text per row, and returns an unbounded `Vec<Replacement>`. The GTK row model separately keeps pre-display original content and resolves preview rows with repeated linear searches plus `Path::display().to_string()` allocation during bind and checkbox activation.

## Goals / Non-Goals

**Goals:**

- Bound preview-specific row count and retained bytes independently of Replace All apply limits.
- Make truncation explicit and ensure omitted matches cannot be applied implicitly.
- Share immutable preview input where ownership stays clear.
- Resolve preview rows and checkboxes in O(1) by stable search-match identity.
- Preserve async generation guards, stale-file validation, atomic apply, and undo safety.

**Non-Goals:**

- Increasing the 10,000 search-result limit or the per-search-line cap.
- Applying replacements that the user could not inspect in the current preview.
- Changing regex expansion semantics or weakening truncated-line exclusion.
- Introducing a generic UI identity framework or replacing GTK list virtualization.

## Decisions

### Return a budgeted preview outcome

`generate_replacement_preview` will accept a `ReplacePreviewBudget` and return `ReplacePreviewOutcome`, not a bare vector. The shipped budget is at most 10,000 generated rows and 64 MiB of conservatively charged preview bytes. Charging includes each original line, replaced line, row-specific regex expansion, path bytes, and the literal replacement once even when storage is shared. Saturating arithmetic checks the next complete row before admitting it.

The outcome contains generated rows, eligible matches omitted by row/byte budget, source rows skipped because their line was truncated or otherwise invalid, and the limiting reason. Generation remains deterministic in incoming search order.

Alternatives considered:

- A row cap alone was rejected because one large replacement template can dominate memory.
- Truncating individual line text further was rejected because apply safety requires complete original lines for accepted rows.
- Failing the whole preview at the first limit was rejected because a bounded inspectable subset remains useful.

### Make preview identity explicit

The search-panel runtime assigns every accepted streamed match a monotonic `SearchMatchId` before creating the GTK row. The ID is stored in the plain search cache and the row item. `ReplacePreviewOutcome` exposes a dense ID-to-preview-index table (with `None` for skipped or omitted matches) and rows carry their originating ID. Checked state is keyed by `SearchMatchId`.

List binding and checkbox activation read the row ID and perform one bounds-checked lookup. They do not compare display paths, line numbers, or offsets, and do not scan preview rows. IDs are scoped to the search generation, so a new search replaces the cache, mapping, and checked set together.

Alternatives considered:

- A hash map keyed by path/line/range was rejected because it still duplicates composite keys and path normalization concerns.
- GTK list positions were rejected because tree expansion and virtualization positions are presentation state, not stable match identity.

### Share immutable content selectively

`SearchMatch::line_content` and `Replacement::original_line` will share an `Arc<str>` so preview generation does not copy the same bounded original line again. Literal mode shares one `Arc<str>` replacement across rows. Regex mode keeps each expanded replacement owned because captures can differ. Every row still owns its replaced display line because it is distinct.

The GTK `SearchResultItem` retains only its display-clamped line, display range, and `SearchMatchId`; the duplicate pre-display original line and original-range fields are removed once all preview consumers use the plain search cache and ID map.

### Present truncation as part of confirmation state

The panel shows generated count, checked count, and omitted eligible count. The confirmation callback clones only checked generated rows from the current outcome. The primary action states that it applies the checked preview subset; accessibility descriptions include that more matches were omitted when truncated. Exiting preview or changing search invalidates the outcome and mapping together.

The result list remains the only scrolling region. Empty generated previews, representative previews, dense previews, awkward paths, and constrained geometry keep search controls, omission feedback, and confirmation reachable without fake rows.

## Risks / Trade-offs

- [A bounded preview may require multiple searches to replace every match] → State the omitted count clearly and never imply full coverage; future pagination is outside this change.
- [`Arc<str>` changes clone and equality shapes] → Keep it inside GTK-free model/service types and add property tests for literal and regex apply equivalence.
- [ID mapping can become stale across searches] → Scope IDs to the search generation and replace all preview/check state atomically on accepted completion.
- [Conservative byte charging may stop before 64 MiB of allocator use] → Prefer predictable upper bounds; benchmark representative and worst-case fixtures.

## Migration Plan

1. Add `SearchMatchId`, budget, outcome, and pure generation tests while adapting current callers.
2. Share original/literal strings and verify apply/stale validation parity.
3. Add the dense ID mapping and migrate row binding, checkbox activation, and checked state.
4. Remove duplicated original content/ranges from `SearchResultItem`.
5. Add truncation UI, accessibility, automation snapshot fields if required, and state-extreme visual/widget tests.
6. Rollback can restore the old vector API; no persisted data format is involved.

## Open Questions

None. Pagination and an explicit “continue with next preview page” workflow are intentionally deferred until real use demonstrates that bounded subsets are insufficient.
