## Why

The command palette already uses the project's nucleo-backed fuzzy matching path, while Recent Open maintains a separate hand-written subsequence matcher whose fuzzy matches often collapse into the same score bucket. The duplicate policy is small but produces weaker ordering and creates two places to evolve matching semantics. Recent Open should reuse a GTK-free ranking seam while keeping its GNOME-style prefix, substring, recency, and activation behavior.

## What Changes

- Extract a small GTK-free fuzzy-query/ranking helper from the existing palette service boundary for reuse by other app search surfaces.
- Keep Recent Open's explicit prefix and substring tiers, then rank true fuzzy matches by the shared nucleo score.
- Preserve newest-first empty-query behavior, the 200-entry history cap, open-tab exclusion, and deterministic recency/path tie-breaks.
- Avoid a generic search trait or UI dependency; the shared abstraction owns query normalization and score calculation only.
- Add cross-surface ranking fixtures, Unicode and awkward-path cases, deterministic tie tests, and regression coverage for empty/no-result/many-result states.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `recent-open-popover`: Refines fuzzy-tier ordering to use the shared GTK-free ranking engine while preserving prefix, substring, recency, geometry, accessibility, and activation contracts.

## Impact

- Affects `services/palette/`, `services/recent_documents.rs`, and their pure service tests; GTK row structure remains unchanged.
- Introduces one concrete reusable value/helper, not a trait, manager, or new crate.
- Can land independently before the final adapter-decomposition change.
