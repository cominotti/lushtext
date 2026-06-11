# Search & Indexing Scaling Patterns

Code examples for making the command palette's fuzzy search and file indexing scale to 100k+ files.

## Table of Contents

1. [Debounced Search Input](#1-debounced-search)
2. [Debounce Token for Stale Results](#2-generation-counter)
3. [Bounded Heap for Top-N](#3-bounded-heap)
4. [Incremental Index Updates](#4-incremental-index)
5. [SIMD Fuzzy Search](#5-nucleo)
6. [Index Rebuild Coalescing](#6-rebuild-coalescing)

---

## 1. Debounced Search Input {#1-debounced-search}

**Status: IMPLEMENTED** — `setup_search` in `command_palette/imp.rs` uses a 150ms `crate::ui::settle::Debounce`. Empty queries rebuild immediately (instant clear UX); non-empty queries are scheduled through the helper, and superseded timers no-op before rebuilding.

---

## 2. Debounce Token for Stale Results {#2-generation-counter}

**Status: IMPLEMENTED** — `search_debounce: Debounce` on the imp struct. `rebuild_results` advances and captures the helper token; on completion, it checks whether the token is still current before updating the `ListStore`. This prevents stale results from flashing when the user types faster than the debounce interval.

---

## 3. Bounded Heap for Top-N {#3-bounded-heap}

**Status: IMPLEMENTED** — `search_items` uses `collect` + `sort_unstable_by` + `truncate(max)`. With k=50 fixed and small, Vec+sort is simpler and equivalently fast to a bounded heap. Benchmarked at 100k files in `bench_file_index_search`.

---

## 4. Incremental Index Updates {#4-incremental-index}

**Status: IMPLEMENTED** — `FileIndex` has `add_file`, `remove_path` (handles both files and directories via `Path::starts_with`), and `rename_path`. Wired to file operation callbacks in the command palette. Full rebuilds only on initial load or workspace folder add/remove. `remove_path` calls `shrink_to_fit()` when >25% of entries removed to reclaim Vec capacity. Benchmarked in `bench_file_index_incremental`.

---

## 5. SIMD Fuzzy Search {#5-nucleo}

**Status: IMPLEMENTED** — The codebase uses `nucleo-matcher = "0.3"` (the low-level SIMD-accelerated scoring library) via `fuzzy_score` and `search_items` in `services/palette.rs`. Matcher and char buffer are reused across candidates. Top-N uses bounded `BinaryHeap`.

**Future option**: The full `nucleo = "0.5"` framework adds a dedicated background worker thread, incremental results streaming, and match position highlighting. Migration guide at `docs/next/nucleo-migration.md`. This is not a current scaling concern — the synchronous approach with 150ms debounce handles 100k files within the frame budget.

---

## 6. Index Rebuild Coalescing {#6-rebuild-coalescing}

**Status: IMPLEMENTED** — `rebuild_file_index` in `window/focus_indexing.rs` uses a 300ms `Debounce` (`index_rebuild_debounce`). If the scheduled token is still current, it spawns a background `FileIndex::rebuild` via `spawn_blocking_then`; async completion checks the same token before installing results. This coalesces rapid workspace mutations (add/remove folders) into a single rebuild.
