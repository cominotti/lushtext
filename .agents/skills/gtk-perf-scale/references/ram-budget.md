# RAM Budget Reference

Memory budget guidelines for LushText on typical desktop hardware (4-core, 8-16GB RAM). This reference documents the memory model so subagents can understand the architectural constraints — not as a checklist for flagging micro allocation patterns.

## Table of Contents

1. [Buffer Memory Model](#1-buffer-memory)
2. [File Index Memory Model](#2-index-memory)
3. [ListStore / TreeListModel Memory](#3-liststore-memory)
4. [Concurrent Load Memory Budget](#4-concurrent-loads)
5. [Established Patterns](#5-established-patterns)

---

## 1. Buffer Memory Model {#1-buffer-memory}

GtkSourceView's `GtkTextBuffer` uses a B-tree of text segments with gap buffers internally. Approximate memory per open tab:

| Scenario | Memory per tab | Formula |
|----------|---------------|---------|
| File loaded, no edits | ~1.5-2x file size | Text B-tree + line index |
| File with active undo history | ~3-4x file size | Text + undo entries (each edit creates entries) |
| File with undo disabled (>50MB) | ~1.5-2x file size | `begin_irreversible_action()` prevents undo accumulation |
| File with syntax highlighting | +10-30% of text memory | GtkSourceView regex context cache |

### Per-tab budget math

| Usage pattern | Tabs | Avg file size | Estimated RAM | Verdict |
|--------------|------|---------------|---------------|---------|
| Typical developer | 20 | 50 KB | ~2 MB | Fine |
| Heavy usage | 20 | 500 KB | ~20 MB | Acceptable |
| Large file editing | 5 | 50 MB | ~500 MB | Concerning on 8GB machines |
| Extreme | 3 | 100 MB | ~600 MB | Near limit — consider eviction |

### Key rules

- **Undo is the biggest memory multiplier.** Disabling it for files >50MB (via permanent `begin_irreversible_action()`) is critical — without this, editing a 50MB file can consume 200MB+ in undo entries after a few find-and-replace operations.
- **Syntax highlighting adds memory proportional to context cache.** Disabling it for files >10MB saves 10-30% of text memory and eliminates the initial highlight scan cost.
- **Buffer eviction** triggers when total estimated buffer memory exceeds `BUFFER_MEMORY_BUDGET` (256MB). Unmodified background tabs are evicted on tab switch.

---

## 2. File Index Memory Model {#2-index-memory}

`FileIndex` holds `Vec<IndexedFile>` where each entry contains:

| Field | Type | Heap allocation | Approx bytes |
|-------|------|-----------------|-------------|
| `path` | `PathBuf` | Yes — one heap alloc per file | ~80-120 bytes (typical path length) |
| `name` | `String` | Yes — one heap alloc per file | ~20-40 bytes (typical filename) |
| `workspace_root` | `Arc<PathBuf>` | Shared — one alloc per workspace | ~8 bytes (Arc pointer) |

**Total per entry**: ~120-170 bytes, conservatively ~200 bytes including Vec overhead and alignment.

| File count | Estimated memory | Notes |
|-----------|-----------------|-------|
| 10,000 | ~2 MB | Typical single-workspace project |
| 50,000 | ~10 MB | Large monorepo |
| 100,000 (cap) | ~20 MB | At index cap — log warning |

### Why the 100k cap matters

At 100k entries, linear scan per search query takes >10ms. The cap prevents unbounded growth. New code that adds files to the index should respect this cap.

---

## 3. ListStore / TreeListModel Memory {#3-liststore-memory}

Each `FileTreeItem` is a GObject subclass. GObject overhead is ~300-400 bytes per item.

| Directory size | ListStore memory | Notes |
|---------------|-----------------|-------|
| 1,000 entries | ~400 KB | Typical directory |
| 10,000 (cap) | ~4 MB | At cap — truncate with sentinel |
| 50,000 (no cap) | ~20 MB | Why we cap at 10k |

### Why the 10,000 entry cap matters

Beyond UI performance (slow GtkListView model diffs), a single `node_modules` with 50k entries would allocate ~20MB of GObjects — wasteful for a directory the user likely didn't intend to browse file-by-file. The cap with a sentinel row is the correct architecture.

---

## 4. Concurrent Load Memory Budget {#4-concurrent-loads}

When restoring a session with N tabs, each `spawn_blocking_then` runs on its own thread:

| Concurrent loads | Avg file size | Peak memory (threads only) |
|-----------------|---------------|---------------------------|
| 8 | 500 KB | ~4 MB |
| 8 | 5 MB | ~40 MB |
| 50 (no guard) | 5 MB | ~250 MB |

This is why the thread spawn guard (max 8 concurrent) matters for RAM, not just CPU scheduling. The guard in `spawn_blocking_then` automatically defers excess spawns via `timeout_add_local_once(50ms)` when 8 threads are active. New code that spawns concurrent loads should respect this guard.

---

## 5. Established Patterns {#5-established-patterns}

These patterns are already implemented and should be preserved:

- **`Arc<PathBuf>` sharing** for workspace roots in `IndexedFile` — files in the same workspace share one allocation instead of cloning per file. 10x memory reduction at 50k files.
- **Buffer eviction** on tab switch when total memory exceeds 256MB budget.
- **`ListStore::splice()`** for batch updates — single `items-changed` signal instead of per-item `append()`.
- **Thread spawn guard** (max 8 concurrent) — caps peak thread memory.
- **File size limits** — 1MB toast, 10MB no syntax, 50MB no undo, 500MB refuse.
- **Directory entry cap** at 10,000 — prevents UI freeze and excessive GObject allocation.
- **Index cap** at 100,000 files — prevents slow queries.

These are architectural decisions. Flag them only if new code bypasses or undermines them.
