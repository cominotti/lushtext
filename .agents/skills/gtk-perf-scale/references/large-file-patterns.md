# Large-File Contracts

Use this reference for file open, save, and editor-residency changes. Current code and tests own all numeric policy.

## Contents

1. [Size policy](#size-policy)
2. [Load pipeline](#load-pipeline)
3. [Save pipeline](#save-pipeline)
4. [Residency budget](#residency-budget)
5. [Review questions](#review-questions)

## Size policy

`services/file_limits.rs` owns the thresholds and `FileSizeCheck` behavior. At the current checkout, policy distinguishes an informational large-file state, syntax-disabled state, undo-and-syntax-disabled state, and refusal state. Read the constants and boundary tests directly; preserve their strict/inclusive comparisons.

Never duplicate threshold literals in a skill-driven patch. Call `FileSizeCheck` queries so open, peek, syntax, undo, and memory policy stay consistent.

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
