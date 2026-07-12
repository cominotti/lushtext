## 1. Shared Concrete Fuzzy Query

- [x] 1.1 Move the reusable nucleo `FuzzyQuery` and buffer-reuse implementation into a GTK-free shared service module with narrow `pub(crate)` visibility.
- [x] 1.2 Delegate existing palette fuzzy helpers to the shared query without changing palette score, grouping, max-result, or empty-query behavior.
- [x] 1.3 Add cross-surface fixtures proving palette and recent fuzzy scoring share case, normalization, acceptance, and numeric score semantics.

## 2. Recent Open Tiered Ranking

- [x] 2.1 Add plain `RecentMatchTier`/`RecentMatchRank` values and select the best prefix, substring, or nucleo fuzzy rank across title, subtitle, and display path.
- [x] 2.2 Sort non-empty results by tier, descending fuzzy score within the fuzzy tier, descending recency, and ascending path.
- [x] 2.3 Preserve explicit newest-first/path ordering for empty trimmed queries, the 200-row cap, open-tab exclusion, and no-result behavior.
- [x] 2.4 Remove the hand-written subsequence matcher after all recent-document callers and tests migrate.

## 3. Ranking and UI Regression Coverage

- [x] 3.1 Add service tests for prefix versus substring versus fuzzy, best matching field, fuzzy score ordering, equal-score recency/path ties, and whitespace-only queries.
- [x] 3.2 Add Unicode, composed/decomposed, mixed-case, symbols, deep paths, one-row, 200-row, no-result, and all-open fixtures.
- [x] 3.3 Run existing command-palette and Recent Open widget/accessibility tests to prove row structure, focus, activation, empty states, and geometry are unchanged.
- [x] 3.4 Run formatting, performance/architecture review, focused tests, `make check`, `make lint-advisory`, and `make pre-commit`; fix every issue found.
- [x] 3.5 Run the learning workflow and document the shared scorer only if another durable adoption rule is needed.
