## 1. Define Ranking and Privacy Baselines

- [x] 1.1 Record current per-source scoring, grouped deduplication, equal-score behavior, result limits, direct/debounced query dispatch, readiness, and Replace Preview invalid-row diagnostics.
- [x] 1.2 Add a test-only full-sort reference with one total rank key: score descending and original source ordinal ascending.
- [x] 1.3 Add captured-tracing fixtures with unique source/replacement sentinels that prove invalid preview diagnostics and typed outcomes reveal no document content.

## 2. Retain Bounded Top Results

- [x] 2.1 Implement a GTK-free worst-first bounded selector that reuses one `FuzzyQuery`, retains at most the requested limit, and final-sorts with the shared total rank key.
- [x] 2.2 Preserve the empty-query direct take path and avoid collecting all matching candidates for non-empty queries.
- [x] 2.3 Switch file, open-tab, note, and command source scoring to bounded selection without changing source group priority, canonical deduplication, labels, or action identity.
- [x] 2.4 Add property/equivalence tests for generated corpora, Unicode normalization, equal scores, zero/one/oversized limits, duplicates, empty queries, and mixed All-mode groups.

## 3. Coalesce Command-Palette Search Work

- [x] 3.1 Add one-active/one-latest runtime state with generation-scoped cancellation and compact pending query/index `Arc` snapshots.
- [x] 3.2 Make direct and debounced query entry points replace the same latest request rather than dispatching independent generic worker tasks.
- [x] 3.3 Check cancellation at a benchmark-calibrated bounded candidate interval and return a typed cancelled outcome before grouping obsolete results.
- [x] 3.4 Apply completions only for the current open palette generation, then start at most one latest request and keep searching/accessibility/readiness state accurate.
- [x] 3.5 Add widget tests for rapid typing, mode/index/scope changes, close during search, stale completion, cancellation progress, and final-query focus/accessibility behavior.

## 4. Remove Content-Bearing Search Diagnostics

- [x] 4.1 Remove document substrings and replacement expansions from model/service traces and represent invalid preview rows with bounded typed reason counts.
- [x] 4.2 Keep confirmation limited to valid current rows and render only non-private counts/reason classes in status or warning UI.
- [x] 4.3 Add unit/integration tests proving invalid-row behavior, confirmation safety, default-level captured logs, and automation snapshots contain none of the sentinel contents.

## 5. Performance and Repository Verification

- [x] 5.1 Extend Criterion coverage to the 100,000-file corpus, varied hit rates, Unicode/ties, small and large result limits, cancellation, and rapid latest-query replacement.
- [x] 5.2 Compare bounded selection against the full-sort reference and record retained candidate counts plus one-active/one-latest high-water state.
- [x] 5.3 Update relevant search/performance/privacy guidance and comments for deterministic rank, cancellation ownership, and content-free diagnostics.
- [x] 5.4 Run focused unit/property/widget tests, `make test-unit`, command-palette/search accessibility and automation-readiness smokes, and Replace Preview safety tests.
- [x] 5.5 Run `make check`, `make lint-advisory`, `make pre-commit`, relevant visual-geometry proof, `git diff --check`, and strict OpenSpec validation.
- [x] 5.6 Perform final architecture/performance/comment/data-safety review confirming GTK-free scoring, bounded result/pending ownership, current-generation UI projection, and private-content exclusion.
