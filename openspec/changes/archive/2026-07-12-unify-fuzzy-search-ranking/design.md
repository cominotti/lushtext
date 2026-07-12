## Context

`services::palette::fuzzy` wraps `nucleo_matcher` in a reusable-per-query matcher and character buffer, but its `FuzzyQuery` is private to the palette module. Recent-document search lowercases title, subtitle, and path, then uses a separate subsequence predicate. Prefix and substring tiers are useful and should remain, but every fuzzy match currently receives the same tier score and is ordered only by recency. The recent list is capped at 200 rows, so the primary objective is consistent ranking and one source of fuzzy semantics rather than a new indexing architecture.

## Goals / Non-Goals

**Goals:**

- Give palette and Recent Open one concrete GTK-free nucleo query helper.
- Preserve Recent Open's prefix-before-substring-before-fuzzy tier policy.
- Rank matches within the fuzzy tier by nucleo score, then deterministically by recency and path.
- Preserve newest-first empty-query behavior, current caps, open-tab exclusion, and row/UI contracts.
- Cover Unicode normalization, awkward paths, multi-field matching, and deterministic ties.

**Non-Goals:**

- Making every search surface share one ranking policy.
- Adding a search trait, global matcher, cache service, or new crate.
- Changing command-palette group priorities or score ordering.
- Performing filesystem I/O during query filtering.

## Decisions

### Move the concrete scorer to a shared service module

The existing `FuzzyQuery` implementation will move from `services/palette/fuzzy.rs` to `services/fuzzy.rs` with `pub(crate)` construction and scoring. It remains a small concrete value that owns `Matcher`, `Atom`, and its reusable UTF-32 buffer. Palette helpers call it exactly as before; recent-document search creates one instance per non-empty query and reuses it across all row fields.

`fuzzy_score` remains available through the palette facade if current callers/tests require it, delegating to the shared module. No UI type or domain model depends on nucleo directly.

Alternatives considered:

- Making Recent Open import a private palette helper was rejected because recents are not a palette sub-workflow.
- Defining a `FuzzyMatcher` trait was rejected because there is one implementation and no runtime substitution need.
- Keeping duplicate subsequence logic was rejected because it provides no meaningful ranking inside the fuzzy tier.

### Keep Recent Open's tiered rank explicit

Recent search will use a plain `RecentMatchRank { tier, fuzzy_score }`. For each of title, subtitle, and display path:

1. a case-insensitive prefix match yields `Prefix`;
2. otherwise a case-insensitive substring yields `Substring`;
3. otherwise a nucleo match yields `Fuzzy(score)`;
4. otherwise the field does not match.

The best field rank wins. Rows sort by tier, descending nucleo score only within `Fuzzy`, descending `last_opened_secs`, then ascending path for deterministic equal timestamps. Empty trimmed queries bypass fuzzy scoring and retain explicit newest-first/path ordering.

Alternatives considered:

- Letting nucleo replace all tiers was rejected because the current GNOME-like prefix and substring expectations are user-visible.
- Mixing nucleo scores across prefix and substring tiers was rejected because score scales do not encode the existing priority policy.

### Keep normalization ownership narrow

The shared scorer retains nucleo's `CaseMatching::Ignore` and `Normalization::Smart`. Recent Open uses case-folded strings only for prefix/substring checks and passes original display strings to nucleo. Query trimming remains recent-specific. Candidate display text and persisted paths are not rewritten.

## Risks / Trade-offs

- [Fuzzy-only result order changes for users accustomed to recency ties] → Preserve recency as the next tie-break and add explicit representative ranking fixtures.
- [Moving the helper can accidentally change palette visibility] → Run the existing palette tests and cross-surface score fixtures against the same candidates.
- [Unicode case folding and nucleo normalization differ] → Keep tier logic explicit and test accented, composed/decomposed, mixed-case, symbol, and path candidates.
- [Path display allocation remains per query] → The set is capped at 200; avoid premature search-index state unless profiling shows a need.

## Migration Plan

1. Move `FuzzyQuery` to the shared service module and keep palette behavior unchanged through delegation.
2. Add `RecentMatchRank` and migrate recent filtering/sorting from the subsequence predicate.
3. Remove `fuzzy_match` after parity/no-result tests migrate.
4. Add cross-surface, Unicode, deterministic-tie, empty-query, and 200-row fixtures.
5. Rollback can restore the recent predicate and palette-local helper; no persisted data changes are involved.

## Open Questions

None. Other search surfaces should adopt the concrete helper only when their own user-facing ranking policy has been specified.
