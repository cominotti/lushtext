## Context

The shared `FuzzyQuery` removes repeated matcher and UTF-32 buffer setup, but palette `search_items` still collects all matching candidates, sorts them, and truncates to 50. The command palette dispatches a worker for each settled/direct query and rejects stale results only at GTK completion, allowing obsolete scans to consume the generic FIFO. Separately, Replace Preview's model logs the original matched substring when regex revalidation fails even though the outcome already counts invalid rows.

The change must keep fuzzy ranking and grouping GTK-free, preserve current source priority/deduplication, keep only one current UI projection, and avoid putting private text into logging or automation surfaces.

## Goals / Non-Goals

**Goals:**

- Retain O(k) candidates per source and select the same deterministic top results as a reference full sort.
- Keep at most one active palette search and one compact latest request.
- Cooperatively stop obsolete scans and expose accurate current readiness/accessibility state.
- Remove content-bearing search diagnostics by construction.
- Add equivalence/property coverage and maximum-index benchmarks.

**Non-Goals:**

- Changing fuzzy library/configuration, result limits, group order, or visible command actions.
- Combining command-palette fuzzy search with workspace content search.
- Adding speculative SIMD or a new dependency.
- Logging hashes or encoded forms of private document text.

## Decisions

### 1. Use a bounded worst-first heap with a deterministic total key

For non-empty queries, score candidates with one `FuzzyQuery` and retain at most `k` entries in a `BinaryHeap` whose root is the worst retained candidate. The total rank is score descending, then source ordinal ascending. A candidate replaces the root only when better. Final retained entries sort by the same key before projection. Empty query continues to take the first `k` source items directly.

The source ordinal is captured before filtering so equal-score results are stable and full-sort reference behavior is well-defined.

**Alternative considered:** `select_nth_unstable` after collecting all matches. Rejected because it still retains O(n) results and does not reduce peak ownership.

### 2. Keep one-active/one-latest in command-palette runtime

Mirror the proven Replace Preview structure: runtime state owns `active`, an atomic cancellation token, and one latest owned query request. New requests advance generation, cancel active work, and replace pending state. Completion drops active ownership, applies results only if current and open, then starts the latest request if any.

Queued state includes the query and shared index snapshots, not precomputed matches. Closing the palette cancels and clears both states. Automation readiness is based on current active/pending ownership rather than every stale task completion.

### 3. Check cancellation at bounded candidate intervals

Scoring accepts a lightweight cancellation view and checks before each source plus at a documented candidate interval. Cancellation returns a typed outcome without completing grouping. The interval avoids per-candidate atomic overhead while bounding obsolete work.

**Alternative considered:** only ignore stale GTK completion. Rejected because it preserves correctness but not worker/backlog bounds.

### 4. Preserve grouping and deduplication after source-local selection

Top-k operates inside existing sources. Group order, open-tab suppression, workspace canonical identity, note categories, and commands remain owned by `grouped_hits`. Equivalence tests compare the optimized selector with a full-sort reference before and after grouping across generated duplicates and ties.

### 5. Remove private diagnostic payloads from the model

Replace Preview continues to increment typed invalid reason counts and skip unsafe rows. The model emits no trace containing line contents or replacement expansion. If the UI needs a warning, it constructs bounded counts/reason classes only. Captured-tracing tests use sentinel source/replacement text to prove absence.

**Alternative considered:** redact/truncate the substring. Rejected because even a short prefix can be sensitive and the content is unnecessary to diagnose the invariant.

## Risks / Trade-offs

- **[Risk] Heap ordering reversals produce subtly different results.** → Define one total-rank helper used by heap, final sort, and full-sort reference property tests.
- **[Risk] Cancellation token outlives or cancels a newer search.** → Scope one token to one active generation and replace it only from main-thread runtime state.
- **[Risk] Latest request retains a large index clone.** → Retain shared `Arc` snapshots only; assert one active plus one pending request and no result vectors in pending state.
- **[Risk] Removing text from logs reduces debugging detail.** → Keep typed reason counts, line number only where policy permits, and reproducible tests without content.
- **[Trade-off] Heap selection is more complex than sort/truncate.** → Confine it to one GTK-free helper and prove exact equivalence.

## Migration Plan

1. Add total-rank/full-sort reference tests and bounded top-k implementation.
2. Switch source scoring while preserving grouped output fixtures.
3. Add one-active/one-latest runtime and cooperative cancellation tests.
4. Replace content-bearing diagnostics with typed reasons and captured-tracing privacy coverage.
5. Run maximum-index benchmarks, widget/accessibility/readiness tests, full repository gates, and strict OpenSpec validation.

No persisted formats or public actions change; rollback can restore prior scoring/runtime independently.

## Open Questions

- Calibrate the cancellation-check interval from the 100,000-file benchmark while keeping a deterministic test override. The one-active/one-latest ownership bound is not open.
