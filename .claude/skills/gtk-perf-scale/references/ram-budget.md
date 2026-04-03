# RAM Budget Reference

Memory budget guidelines for LushText on typical desktop hardware (4-core, 8-16GB RAM). Every subagent should check changed code against these thresholds and patterns.

## Table of Contents

1. [Buffer Memory Model](#1-buffer-memory)
2. [File Index Memory Model](#2-index-memory)
3. [ListStore / TreeListModel Memory](#3-liststore-memory)
4. [Clone Avoidance Patterns](#4-clone-avoidance)
5. [Closure Capture Memory](#5-closure-captures)
6. [Background Thread Allocations](#6-background-allocs)
7. [Anti-Patterns](#7-anti-patterns)

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
- **Buffer eviction** (unloading inactive tabs) is worth considering when total buffer memory exceeds ~256MB. See `references/large-file-patterns.md` section 5 for the pattern.

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

### Arc<PathBuf> sharing is critical

Without `Arc` sharing, `workspace_root` would clone the full `PathBuf` per file. For a workspace with 50k files:
- **Without Arc**: 50,000 * ~80 bytes = ~4 MB wasted on identical PathBuf clones
- **With Arc**: 50,000 * 8 bytes + 1 * ~80 bytes = ~0.4 MB

This 10x reduction in workspace_root memory is why `Arc<PathBuf>` sharing is mandatory, not optional.

### Vec capacity hints

When the approximate entry count is known (e.g., from a previous index build or directory metadata), use `Vec::with_capacity()` to avoid reallocation churn:

```rust
// If we know roughly how many files to expect:
let mut files = Vec::with_capacity(estimated_count);
```

Without capacity hints, `Vec` doubles its allocation on growth. For 100k entries, this means up to 50% wasted capacity in the worst case (~10 MB waste). With a hint, waste is near zero.

---

## 3. ListStore / TreeListModel Memory {#3-liststore-memory}

Each `FileTreeItem` is a GObject subclass. GObject overhead:

| Component | Approx bytes |
|-----------|-------------|
| GObject base (type info, ref count, qdata) | ~64-96 bytes |
| Properties (path: PathBuf, name: String, is_dir: bool) | ~120-160 bytes |
| GObject signal infrastructure | ~64-128 bytes |
| **Total per FileTreeItem** | **~300-400 bytes** |

| Directory size | ListStore memory | Notes |
|---------------|-----------------|-------|
| 1,000 entries | ~400 KB | Typical directory |
| 10,000 (cap) | ~4 MB | At cap — truncate with sentinel |
| 50,000 (no cap) | ~20 MB | Why we cap at 10k |

### Why the 10,000 entry cap matters for RAM

Beyond the UI performance implications (slow `GtkListView` model diffs), each `FileTreeItem` GObject costs ~300-400 bytes. A single `node_modules` directory with 50k entries would allocate ~20MB of GObjects in one `ListStore` — not catastrophic, but wasteful for a directory the user likely didn't intend to browse file-by-file.

### `splice()` vs per-item `append()` — memory impact

`splice()` does a single reallocation for the batch. Per-item `append()` may trigger multiple reallocations as the internal array grows, temporarily consuming up to 2x the final memory during growth phases.

---

## 4. Clone Avoidance Patterns {#4-clone-avoidance}

Unnecessary cloning is the most common source of memory waste in Rust GTK code. These patterns reduce allocations:

### Prefer `&str` over `String::clone()` in function parameters

```rust
// Wasteful: clones the String to pass it
fn process_name(name: String) { ... }
let result = process_name(file.name.clone());

// Better: borrow instead
fn process_name(name: &str) { ... }
let result = process_name(&file.name);
```

### Use `Arc<T>` for shared ownership across closures

When multiple closures or data structures need the same value, `Arc` shares one allocation:

```rust
// Wasteful: clones PathBuf into every closure
for file in &files {
    let root = workspace_root.clone(); // 80+ bytes per clone
    spawn(move || process(file, &root));
}

// Better: share one allocation
let root = Arc::new(workspace_root);
for file in &files {
    let root = Arc::clone(&root); // 8 bytes (pointer copy)
    spawn(move || process(file, &root));
}
```

### Use `Cow<'_, str>` when a function sometimes borrows, sometimes owns

```rust
use std::borrow::Cow;

fn display_name(path: &Path) -> Cow<'_, str> {
    path.file_name()
        .map(|n| n.to_string_lossy()) // Returns Cow — no alloc if valid UTF-8
        .unwrap_or(Cow::Borrowed("unknown"))
}
```

### `PathBuf` → `Arc<PathBuf>` for shared paths

Any path that appears in multiple data structures (workspace root in every IndexedFile, file path in tab state + tree model) should use `Arc<PathBuf>`:

```rust
// In IndexedFile:
pub workspace_root: Arc<PathBuf>,  // shared across all files in workspace

// When constructing:
let root = Arc::new(workspace_root.clone());
for file in discovered_files {
    files.push(IndexedFile {
        workspace_root: Arc::clone(&root), // cheap pointer copy
        ..
    });
}
```

---

## 5. Closure Capture Memory {#5-closure-captures}

Signal handler closures in GTK are **long-lived** — they persist for the widget's entire lifetime. What they capture stays in memory.

### Capture only what you need

```rust
// Wasteful: captures the entire struct (which may contain Vec, HashMap, etc.)
let state = self.imp().clone(); // captures ALL fields of the imp struct
button.connect_clicked(move |_| {
    state.do_thing();
});

// Better: capture only the field you need
let label = self.imp().label.clone();
button.connect_clicked(move |_| {
    label.set_text("clicked");
});
```

### Use `@weak` references for GTK objects in closures

`@weak` prevents both reference cycles (memory leaks) and oversized captures:

```rust
// GOOD: weak reference — if widget is destroyed, closure becomes no-op
button.connect_clicked(clone!(@weak self as window => move |_| {
    window.do_something();
}));
```

### Never capture large collections in signal closures

```rust
// BAD: Vec<IndexedFile> with 100k entries (~20MB) captured in closure
let files = self.imp().file_index.borrow().clone();
entry.connect_changed(move |_| {
    search(&files, &query);
});

// GOOD: access via the widget's imp struct on each invocation
entry.connect_changed(clone!(@weak self as palette => move |entry| {
    let files = palette.imp().file_index.borrow();
    search(&files, &entry.text());
}));
```

The second pattern accesses the index through the widget reference each time, so the closure captures only a weak pointer (~8 bytes) instead of the entire index (~20MB).

---

## 6. Background Thread Allocations {#6-background-allocs}

### Peak memory during file reads

`std::fs::read_to_string` internally:
1. Allocates a `Vec<u8>` buffer sized to `fs::metadata.len()`
2. Reads into the buffer
3. Validates UTF-8 (walks the entire buffer)
4. Converts to `String` (reuses the `Vec<u8>` allocation — no copy)

**Peak memory = ~1x file size** for `read_to_string` itself. But in the `spawn_blocking_then` flow:

```
Background thread:  String content = read_to_string(path)  →  ~1x file_size
Main thread handoff: content moved into idle_add_once closure  →  same allocation, no copy
Main thread:        buffer.set_text(&content)  →  GtkTextBuffer allocates ~1.5x file_size
                    content dropped after set_text  →  1x file_size freed
Final:              ~1.5-2x file_size (GtkTextBuffer only)
```

**Brief peak during `set_text`**: ~2.5-3x file_size (String + GtkTextBuffer simultaneously).

### Drop intermediates before sending results

```rust
// Wasteful: raw_bytes lives until the closure completes
spawn_blocking_then(state, move || {
    let raw_bytes = std::fs::read(&path)?;      // 1x file_size
    let validated = simdutf8::from_utf8(&raw_bytes)?;
    let content = validated.to_string();          // 2x file_size (raw + content)
    Ok(content)
    // raw_bytes dropped here — but it lived alongside content briefly
}, |editor, result| { ... });

// Better: scope the intermediate
spawn_blocking_then(state, move || {
    let content = {
        let raw_bytes = std::fs::read(&path)?;  // 1x file_size
        let validated = simdutf8::from_utf8(&raw_bytes)?;
        validated.to_string()                     // 2x briefly, then raw_bytes dropped
    };
    // Here: only 1x file_size (content only)
    Ok(content)
}, |editor, result| { ... });
```

For a 50MB file, the "wasteful" version peaks at 100MB on the background thread; the "better" version peaks at 100MB briefly but returns to 50MB before the result is sent to the main thread.

### Concurrent load memory budget

When restoring a session with N tabs, each `spawn_blocking_then` runs on its own thread:

| Concurrent loads | Avg file size | Peak memory (threads only) |
|-----------------|---------------|---------------------------|
| 8 | 500 KB | ~4 MB |
| 8 | 5 MB | ~40 MB |
| 50 | 500 KB | ~25 MB |
| 50 | 5 MB | ~250 MB |

This is why the thread spawn guard (max 8 concurrent) matters for RAM, not just CPU scheduling. The guard in `spawn_blocking_then` automatically defers excess spawns via `timeout_add_local_once(50ms)` when 8 threads are active — no manual batching needed. This caps peak thread memory at 8 * max_file_size.

---

## 7. Anti-Patterns {#7-anti-patterns}

### [FLAG] PathBuf clone per-file instead of Arc<PathBuf> per-workspace

```rust
// BAD: 50k files × ~80 bytes = ~4MB wasted
for file in discovered_files {
    files.push(IndexedFile {
        workspace_root: workspace_root.clone(), // full PathBuf clone
        ..
    });
}
```

### [FLAG] Capturing entire FileIndex or Vec<IndexedFile> in a signal closure

Any closure that captures 20MB+ of data and lives for the widget lifetime is a memory concern, especially if it captures a snapshot that becomes stale.

### [RECOMMEND] Missing `Vec::with_capacity()` for known-size collections

When building a collection whose approximate final size is known (from directory metadata, previous build, or a cap constant), always pre-allocate:

```rust
// Without: up to 50% wasted capacity from doubling growth
let mut items = Vec::new();

// With: near-zero waste
let mut items = Vec::with_capacity(entry_count);
```

### [RECOMMEND] `String::clone()` where `&str` would suffice

Cloning a String allocates a new heap buffer. If the caller only needs to read the string, pass `&str` instead.

### [RECOMMEND] `retain()` without `shrink_to_fit()` after bulk removes

`Vec::retain()` does not release excess capacity. After removing >25% of entries, call `shrink_to_fit()` to reclaim memory:

```rust
index.files.retain(|f| !f.path.starts_with(&removed_dir));
if index.files.len() < index.files.capacity() * 3 / 4 {
    index.files.shrink_to_fit();
}
```

### [IMPLEMENTED] Buffer eviction for tabs exceeding memory budget

Buffer eviction triggers when total estimated buffer memory exceeds `BUFFER_MEMORY_BUDGET` (256MB). Unmodified background tabs are evicted on tab switch. See `references/large-file-patterns.md` section 5.
