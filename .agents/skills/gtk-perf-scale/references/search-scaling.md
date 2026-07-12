# Search and Index Scaling

Use this reference for palette, recent-document, note, and workspace search changes. Search workflows have different semantics; verify the specific implementation before reusing a pattern.

## Contents

1. [Input settling and freshness](#input-settling-and-freshness)
2. [Index construction](#index-construction)
3. [Scoring and result bounds](#scoring-and-result-bounds)
4. [Review questions](#review-questions)

## Input settling and freshness

UI search commonly uses `gtk_lush_settle::Debounce`, but the delay is workflow-owned policy. Clearing an empty query may require immediate projection. A debounce token prevents superseded scheduled work; background search or rebuild completion also needs a freshness check before installing results.

Do not confuse cancellation with freshness: work may finish successfully and still be obsolete.

## Index construction

`services/palette/index.rs` owns file-count and depth limits. Rebuild and incremental-update paths must preserve those limits, canonical/path semantics, ignore rules, and stable ownership. Workspace-triggered rebuilds should be coalesced and performed off the GTK thread.

Verify how filesystem watcher events update the index before recommending a full rebuild. A simpler full rebuild can be materially worse on large workspaces.

## Scoring and result bounds

Palette fuzzy scoring uses `nucleo-matcher`; inspect current matcher reuse and ordering semantics. Result construction must have an explicit maximum, but the best top-N algorithm depends on candidate count, result count, ranking stability, and measured cost. Preserve deterministic tie-breaking where user-visible order depends on it.

Do not claim a fixed candidate count fits a frame budget without a current benchmark on representative data. Background scoring still requires bounded results and freshness on completion.

## Review questions

- What caps candidates, index size, traversal depth, result count, and history?
- Does every rebuild/update preserve the same caps?
- Can repeated input enqueue unbounded work or retain large query snapshots?
- Are empty queries and mode switches handled without stale flashes?
- Does ranking remain deterministic after an optimization?
- Does benchmark data include realistic names, paths, misses, Unicode, and maximum-size indexes?
