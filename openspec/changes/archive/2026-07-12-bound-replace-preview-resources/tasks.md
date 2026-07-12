## 1. Preview Domain Model and Budgets

- [x] 1.1 Add `SearchMatchId`, `ReplacePreviewBudget`, limiting reason, and typed `ReplacePreviewOutcome` with generated, omitted-eligible, and skipped-source counts.
- [x] 1.2 Implement 10,000-row and 64 MiB saturating byte accounting that admits only complete rows in deterministic search order.
- [x] 1.3 Add generator tests for zero, exact row/byte limits, one-over limits, truncated source rows, huge literal text, regex expansions, Unicode, and saturation.

## 2. Shared Storage and Stable Identity

- [x] 2.1 Share bounded original lines between `SearchMatch` and accepted replacements with `Arc<str>` and share one literal replacement across literal-mode rows.
- [x] 2.2 Keep regex expansions row-owned and verify original line, match range, replacement text, and stale validation remain unchanged.
- [x] 2.3 Assign monotonic match IDs at streamed result ingestion and store them in the plain search cache and GTK result row.
- [x] 2.4 Build one dense generation-scoped match-ID-to-preview-index map and key checked state by match ID.
- [x] 2.5 Migrate row bind and checkbox activation to direct bounds-checked identity lookup with no preview scan or display-path reconstruction.
- [x] 2.6 Remove duplicate pre-display original line and original-range fields from `SearchResultItem` after all preview consumers use stable IDs.

## 3. Bounded Preview UI and Apply Boundary

- [x] 3.1 Accept the typed preview outcome only when query, replacement, options, search results, panel lifetime, and preview generation remain current.
- [x] 3.2 Show generated, checked, omitted, and skipped state in visible and accessible feedback without adding fake result rows.
- [x] 3.3 Ensure confirmation clones only checked generated rows from the current outcome and never includes omitted, skipped, unchecked, or stale matches.
- [x] 3.4 Preserve explicit pending, no-eligible-preview, exit/invalidation, empty replacement, and normal-result restoration states.
- [x] 3.5 Keep search controls, omission feedback, and confirmation outside the item scrolling region in constrained geometry.

## 4. Safety, Scale, and UI Verification

- [x] 4.1 Add property tests showing accepted literal/regex previews apply identically to existing safe range and stale-file semantics.
- [x] 4.2 Add 10,000-row performance tests that detect linear lookup during bind/toggle and assert preview byte accounting stays within budget.
- [x] 4.3 Add widget/accessibility tests for zero, representative, truncated, large replacement, awkward path, Unicode, stale generation, and constrained states.
- [x] 4.4 Add visual geometry proof for dense/truncated preview feedback, no horizontal scrollbar, reachable controls, and item-region-only scrolling.
- [x] 4.5 Run formatting, data-safety/performance/architecture reviews, focused tests, `make accessibility-smoke`, `make visual-geometry-smoke`, `make check`, and `make pre-commit`; fix every issue found.
- [x] 4.6 Run the learning workflow and update search-panel guidance only for durable new contracts.
