# Large-File Contracts

Use this reference for file open, save, and editor-residency changes. Current code and tests own all numeric policy.

## Contents

1. [Size policy](#size-policy)
2. [Load pipeline](#load-pipeline)
3. [Save pipeline](#save-pipeline)
4. [Residency budget](#residency-budget)
5. [Review questions](#review-questions)

## Size policy

`model/document_scale.rs` owns the thresholds and the `DocumentScale` policy, which replaced the byte-only `FileSizeCheck` (`document-scale-policy`). It reads a paragraph profile (total bytes, longest paragraph, wrap-off batch density) that the decode-analysis worker accumulates in its existing pass (`model/paragraph_boundary.rs`), and decides two kinds of output: byte tiers (open admission at 500 MB, undo and local history at 50 MB, save-only history at 10 MB, the memory multiplier) and presentation features (syntax, bracket matching, minimap, invisibles, search highlighting, `protected_view`), which an editor session holds monotone and which edits re-evaluate synchronously (`model/paragraph_profile_watch.rs`, `ui/editor_page/document_scale.rs`). Comparisons are strict (`>`); read the constants and the `t`/`t+1` tests directly.

Never duplicate threshold literals in a skill-driven patch. Callers that know only a byte count use `DocumentScale::for_total_bytes`; an editor reads `editor.document_scale()`, so open, peek, syntax, undo, and memory policy stay consistent.

## Load pipeline

`services/editor_io.rs` owns metadata checks, filesystem reads, encoding-aware decode, line-ending detection, and file-health classification. Expensive work belongs in a bounded background task, while GTK buffer installation belongs on the main thread.

Review these separate contracts:

- size is checked before an unacceptable allocation;
- cancellation can avoid unnecessary work;
- freshness prevents a completed load from applying to the wrong editor state;
- valid UTF-8 uses the accelerated branch without discarding BOM/legacy-encoding support;
- GTK installation is itself bounded or deliberately degraded for large content.

## Save pipeline

Editor saves route through the filesystem durable-write boundary. Preserve safe temporary permissions, metadata, temp-file sync, stable target coordination, rename/fallback classification, and parent-directory sync. A background thread alone is not sufficient.

The editor snapshot is main-thread-owned. Current code selects synchronous or sliced snapshotting based on live buffer state and keeps the view protected while saving. Duplicate saves and close flows must respect the in-flight save. Modified state clears only after accepted durable success; durability uncertainty remains distinct from a before-rename failure.

## Residency budget

`model/editor_memory.rs` owns the editor-text estimate, upper budget, low-water policy, deterministic LRU selection, and protected/no-progress outcomes. The estimate is intentionally conservative and O(1), not process RSS. Active, modified, untitled, saving, loading, failed, or non-reloadable pages protect user work and may make the budget soft.

Any eviction change must revalidate editor identity and policy/freshness state immediately before applying it.

## Review questions

- Can bytes, decoded text, GTK buffer storage, and a save/load result coexist at peak?
- Does cancellation stop work, and does freshness stop stale application?
- Can a protected document be evicted or closed?
- Does a new preview, history, or health analysis bypass the same size policy?
- Are failure states surfaced without making unsaved work appear clean?
