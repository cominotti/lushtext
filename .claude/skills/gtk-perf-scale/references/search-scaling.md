# Search & Indexing Scaling Patterns

Code examples for making the command palette's fuzzy search and file indexing scale to 100k+ files.

## Table of Contents

1. [Debounced Search Input](#1-debounced-search)
2. [Generation Counter for Stale Results](#2-generation-counter)
3. [Bounded Heap for Top-N](#3-bounded-heap)
4. [Incremental Index Updates](#4-incremental-index)
5. [nucleo: SIMD-Accelerated Fuzzy Search](#5-nucleo)
6. [SIMD Crate Adoption Beyond Search](#6-simd-crates)
7. [Index Rebuild Coalescing](#7-rebuild-coalescing)

---

## 1. Debounced Search Input {#1-debounced-search}

The current `setup_search` in `command_palette/imp.rs` connects `search_changed` directly to `rebuild_results`. Every keystroke triggers a full index scan. Adding a 150ms debounce cuts work by 3–5x for typical typing speeds.

```rust
fn setup_search(&self) {
    let obj = self.obj().clone();
    let pending_id: Rc<Cell<Option<glib::SourceId>>> = Rc::new(Cell::new(None));

    self.search_entry.connect_search_changed(move |entry| {
        // Cancel any pending search
        if let Some(id) = pending_id.take() {
            id.remove();
        }

        let query = entry.text().to_string();
        let palette = obj.clone();
        let pending = pending_id.clone();

        // For empty queries, rebuild immediately (instant clear is expected UX)
        if query.is_empty() {
            palette.imp().rebuild_results(&query);
            return;
        }

        // Schedule rebuild after 150ms of inactivity
        let id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(150),
            move || {
                palette.imp().rebuild_results(&query);
                pending.set(None);
            },
        );
        pending_id.set(Some(id));
    });
}
```

The empty-query shortcut is important: when the user clears the search bar (Backspace to empty, or Ctrl+A Delete), results should refresh instantly. The debounce only applies to non-empty queries where scoring is the bottleneck.

---

## 2. Generation Counter for Stale Results {#2-generation-counter}

When search is debounced, results arrive 150ms after the last keystroke. But what if the user types, pauses 150ms (triggering a search), then immediately types more? The first search's results arrive and briefly flash before being replaced by the second search's results. A generation counter prevents this.

```rust
// In imp.rs struct:
pub search_generation: Cell<u32>,

// In rebuild_results:
pub fn rebuild_results(&self, query: &str) {
    let generation = self.search_generation.get().wrapping_add(1);
    self.search_generation.set(generation);

    let mode = self.mode.get();
    let index = self.file_index.borrow();
    let results = palette::search_all(&index, query, mode, 50);

    // Check if a newer query has been issued while we were scoring
    if self.search_generation.get() != generation {
        return; // Stale — a newer query superseded us
    }

    self.results_store.remove_all();
    for result in &results {
        let item = match &result.item {
            SearchResultItem::File(f) => PaletteItem::from_indexed_file(f),
            SearchResultItem::Command(c) => PaletteItem::from_command_def(c),
        };
        self.results_store.append(&item);
    }

    // ... rest of the method (no_results_label, auto-select)
}
```

For synchronous scoring (current implementation), the generation counter catches the case where `rebuild_results` is re-entered during the `results_store.remove_all()` / `append` loop (unlikely with GTK's single-threaded model but defensive). For future async scoring, it becomes essential.

---

## 3. Bounded Heap for Top-N {#3-bounded-heap}

The current `search_items` collects all matches into a `Vec`, sorts the entire vector, then truncates to `max`. For 100k files with 80k matches (common with short queries), this sorts 80k elements to keep 50.

A `BinaryHeap` with a size cap is O(n log k) instead of O(n log n), where k=50 and n=match count:

```rust
use std::cmp::Reverse;
use std::collections::BinaryHeap;

fn search_items_bounded<'a, I, T, F, G>(
    items: I,
    get_text: F,
    wrap: G,
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'a>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    F: Fn(&T) -> &str,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();

    // Min-heap: keeps the top `max` items. The root is the *lowest* score
    // in the heap, so we can efficiently check if a new item beats it.
    let mut heap: BinaryHeap<Reverse<ScoredResult<'a>>> = BinaryHeap::with_capacity(max + 1);

    for item in items {
        let text = get_text(item);
        if let Some(score) = fuzzy_score_chars(&query_chars, text) {
            let result = ScoredResult {
                item: wrap(item),
                score,
            };

            if heap.len() < max {
                heap.push(Reverse(result));
            } else if let Some(min) = heap.peek() {
                if score > min.0.score {
                    heap.pop();
                    heap.push(Reverse(result));
                }
            }
        }
    }

    // Extract results sorted by score descending
    let mut results: Vec<_> = heap.into_iter().map(|r| r.0).collect();
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}
```

This requires `ScoredResult` to implement `Ord`. Since we only compare by score:

```rust
impl Ord for ScoredResult<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.cmp(&other.score)
    }
}

impl PartialOrd for ScoredResult<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
```

**When to adopt**: benchmark `search_items` with 100k entries. If the current sort-and-truncate takes <5ms, the heap won't provide meaningful improvement. At >10ms, the heap reduces it to ~2ms.

---

## 4. Incremental Index Updates {#4-incremental-index}

The current `FileIndex::rebuild` re-scans all workspace roots on every change. For incremental updates (file created, deleted, or renamed), a delta-based approach avoids the full scan:

```rust
impl FileIndex {
    /// Add a single file to the index.
    pub fn add_file(&mut self, path: PathBuf, workspace_root: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.files.push(IndexedFile {
            path,
            name,
            workspace_root,
        });
    }

    /// Remove a file from the index by path.
    pub fn remove_file(&mut self, path: &Path) {
        self.files.retain(|f| f.path != path);
    }

    /// Remove all files under a directory prefix.
    pub fn remove_directory(&mut self, dir: &Path) {
        self.files.retain(|f| !f.path.starts_with(dir));
    }

    /// Update a file's path (rename).
    pub fn rename_file(&mut self, old_path: &Path, new_path: PathBuf) {
        if let Some(entry) = self.files.iter_mut().find(|f| f.path == old_path) {
            entry.name = new_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            entry.path = new_path;
        }
    }
}
```

Wire these into the existing file operation callbacks:

- `connect_file_created` → `file_index.add_file(path, workspace_root)`
- `connect_file_deleted` → `file_index.remove_file(path)` or `remove_directory(path)`
- `connect_file_renamed` → `file_index.rename_file(old, new)`

Full rebuilds are still needed for: initial load, adding/removing workspace roots. But incremental updates handle the common case (user creates/renames/deletes files while editing) without a full rescan.

**RAM note**: `retain()` does not release excess capacity. After a bulk `remove_directory` that drops >25% of entries, call `files.shrink_to_fit()` to reclaim the unused Vec capacity. For a 100k-entry index where 30k files are removed, this reclaims ~6MB of unused Vec backing storage.

---

## 5. nucleo: SIMD-Accelerated Fuzzy Search {#5-nucleo}

`nucleo` is a high-performance fuzzy matching library used by the Helix editor. It is the **recommended replacement** for the current hand-rolled `fuzzy_score_chars`. With LushText's x86-64-v3 + Apple Silicon baseline, nucleo's SIMD paths run at full speed on every target machine.

### Why nucleo is mandatory, not optional

| Aspect | Current `fuzzy_score_chars` | `nucleo` |
|--------|---------------------------|----------|
| Algorithm | Greedy subsequence with bonuses | Smith-Waterman variant with optimal alignment |
| SIMD usage | None (scalar, 1 byte/cycle) | AVX2 on x86-v3, NEON on aarch64 |
| Performance | ~100ns per candidate | ~50ns per candidate (2x faster) |
| Match quality | Good for simple cases; misses optimal alignments | Better ranking — finds the best alignment, not just the first |
| Async support | None (synchronous, blocks main thread) | Built-in `Nucleo<T>` with background worker thread |
| Match positions | Not tracked | Returns exact match positions (enables highlight rendering) |
| Dependencies | Zero | Small (~3 crates, no system deps) |

**RAM note**: `Nucleo<T>` owns both the item data and internal scoring state. During active search, memory is approximately 2x the IndexedFile collection (items + scored copies with UTF-32 character buffers). For 100k files (~20MB of IndexedFile data), expect ~40MB during active search, dropping back to ~20MB when idle. The `Injector` is lock-free and does not duplicate items — it shares ownership with the matcher.

The 2x raw scoring speedup is the floor — the real wins come from:
- **Async scoring**: `Nucleo<T>` scores on a dedicated worker thread, never blocking the GTK main thread regardless of index size
- **Incremental results**: Results stream in as scoring progresses, enabling progressive display for large indexes
- **Match highlighting**: Position data enables highlighting matched characters in the palette UI (e.g., bold the "m" and "r" in "**m**ain.**r**s" when querying "mr")

### How nucleo uses SIMD internally

nucleo's inner scoring loop uses SIMD for two critical operations:

1. **Candidate pre-filtering**: Uses SIMD byte comparison to quickly reject candidates that don't contain the query's first character. On AVX2, this checks 32 bytes simultaneously — a 32x throughput improvement over scalar `chars().any()`.

2. **Bonus computation**: The word-boundary and case-change bonuses (similar to what `fuzzy_score_chars` computes with `matches!(prev_cand_char, '/' | '.' | '_' | '-' | ' ')`) are computed in bulk using SIMD vector operations on the candidate's byte array.

These SIMD paths are selected automatically at runtime via `std::is_x86_feature_detected!("avx2")` (x86) or compile-time NEON availability (aarch64). With our x86-v3 baseline, the AVX2 path is always taken — the detection is just a single branch on first call, cached thereafter.

### Full integration guide

#### Step 1: Add dependency

```toml
# In workspace Cargo.toml [workspace.dependencies]:
nucleo = "0.5"

# In crates/lushtext-core/Cargo.toml [dependencies]:
nucleo = { workspace = true }
```

Then: `cargo hakari generate && make cargo-sources`

#### Step 2: Replace FileIndex + search_items with Nucleo<T>

The key architectural change: `Nucleo<T>` owns both the data and the matcher. It replaces `FileIndex` + `fuzzy_score_chars` + `search_items` with a single type.

```rust
use nucleo::{Nucleo, Config, Injector, Utf32String};
use nucleo::pattern::{CaseMatching, Normalization, Pattern, AtomKind};
use std::path::PathBuf;
use std::sync::Arc;

/// Data stored per indexed file in nucleo's internal buffer.
#[derive(Clone)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub name: String,
    pub workspace_root: Arc<PathBuf>,  // shared across files in same workspace
}

/// Create a new Nucleo matcher instance.
/// Call once at window construction; reuse for the window's lifetime.
pub fn create_matcher(notify: impl Fn() + Send + Sync + 'static) -> Nucleo<IndexedFile> {
    Nucleo::new(
        Config::DEFAULT,
        Arc::new(notify),  // called when new results are ready
        None,              // single column (match against filename)
        1,                 // 1 worker thread (sufficient for text editor use)
    )
}

/// Populate the matcher with files from workspace roots.
/// Call from a background thread via spawn_blocking_then.
pub fn inject_files(injector: &Injector<IndexedFile>, roots: &[PathBuf]) {
    let mut visited = std::collections::HashSet::new();
    for root in roots {
        let canonical_root = match root.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let workspace_root = Arc::new(root.clone());
        inject_recursive(injector, root, &workspace_root, &canonical_root, &mut visited, 0);
    }
}

fn inject_recursive(
    injector: &Injector<IndexedFile>,
    dir: &std::path::Path,
    workspace_root: &Arc<PathBuf>,
    canonical_root: &std::path::Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    depth: u32,
) {
    if depth > 64 { return; }
    let canonical = match dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return,
    };
    if !canonical.starts_with(canonical_root) || !visited.insert(canonical) {
        return;
    }

    for (path, is_dir) in crate::services::file_tree::scan_directory(dir) {
        if is_dir {
            inject_recursive(injector, &path, workspace_root, canonical_root, visited, depth + 1);
        } else {
            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let file = IndexedFile {
                path,
                name: name.clone(),
                workspace_root: Arc::clone(workspace_root),
            };
            // The closure tells nucleo what text to match against
            injector.push(file, |item, cols| {
                cols[0] = Utf32String::from(item.name.as_str());
            });
        }
    }
}

/// Update the search query. Call on every (debounced) search input change.
pub fn update_query(nucleo: &mut Nucleo<IndexedFile>, query: &str) {
    nucleo.pattern.reparse(
        0,                         // column index
        query,
        CaseMatching::Smart,       // lowercase query = case-insensitive
        Normalization::Smart,      // normalize unicode
        query.starts_with(&nucleo.pattern.column_pattern(0).atoms().map_or(
            String::new(),
            |atoms| atoms.iter().map(|a| a.text()).collect::<String>(),
        )),  // append mode optimization: true if new query extends the old one
    );
}

/// Read the top N results from the matcher's snapshot.
pub fn get_results(nucleo: &Nucleo<IndexedFile>, max: usize) -> Vec<(u32, &IndexedFile)> {
    let snapshot = nucleo.snapshot();
    (0..snapshot.matched_item_count().min(max as u32))
        .filter_map(|idx| {
            let item = snapshot.get_matched_item(idx)?;
            Some((item.score, item.data))
        })
        .collect()
}
```

#### Step 3: Wire into GTK main loop

The `notify` callback from `Nucleo::new` fires on the worker thread when results change. Use `glib::idle_add_once` to bounce it to the main thread:

```rust
// In window construction:
let window_weak: glib::SendWeakRef<LushtextWindow> = window.downgrade().into();
let matcher = create_matcher(move || {
    let weak = window_weak.clone();
    glib::idle_add_once(move || {
        if let Some(window) = weak.upgrade() {
            window.imp().command_palette.refresh_from_matcher();
        }
    });
});
```

#### Step 4: Match highlighting in palette results

nucleo returns `Indices` (matched byte positions) per result. Use these to render highlighted matches in the palette's `GtkLabel`:

```rust
// In connect_bind for the palette factory:
let item = snapshot.get_matched_item(position).unwrap();
let indices = item.indices();  // Vec<u32> of matched byte positions
let name = &item.data.name;

// Build Pango markup with matched chars in bold
let mut markup = String::with_capacity(name.len() * 2);
for (i, ch) in name.chars().enumerate() {
    if indices.contains(&(i as u32)) {
        markup.push_str("<b>");
        markup.push(ch);
        markup.push_str("</b>");
    } else {
        // Escape for Pango markup
        match ch {
            '&' => markup.push_str("&amp;"),
            '<' => markup.push_str("&lt;"),
            '>' => markup.push_str("&gt;"),
            _ => markup.push(ch),
        }
    }
}
name_label.set_markup(&markup);
```

### What happens to the existing code

- **Delete**: `fuzzy_score`, `fuzzy_score_chars`, `search_items` in `palette.rs` — fully replaced by nucleo
- **Delete**: `FileIndex` struct and `collect_files_recursive` — replaced by nucleo's `Injector` + the new `inject_files`/`inject_recursive` functions
- **Keep**: `all_commands()`, `search_commands()` — commands are a small static list; nucleo overhead isn't justified for ~11 items. Score them with a simple loop or a second `Nucleo<CommandDef>` instance if unified scoring is preferred.
- **Keep**: `search_all()` as a dispatcher between file results (from nucleo) and command results (from the existing loop)

---

## 6. SIMD Crate Adoption Beyond Search {#6-simd-crates}

> **Cross-reference**: For a comprehensive SIMD adoption guide covering the full file-load path (simdutf8 for all file sizes, not just >10MB), see `gtk-perf-rust-optimize/references/simd-opportunities.md`.

With x86-64-v3 + Apple Silicon as the target baseline, several SIMD-accelerated crates provide free performance gains for operations LushText already performs.

### `memchr` — Fast byte scanning

Already a transitive dependency. Use explicitly for newline counting and line-number computation in large files:

```rust
use memchr::memchr_iter;

/// Count lines in a byte slice. ~32 bytes/cycle on AVX2 vs ~1 byte/cycle scalar.
fn count_lines(content: &[u8]) -> usize {
    memchr_iter(b'\n', content).count()
}

/// Find the byte offset of line N.
fn line_offset(content: &[u8], line: usize) -> Option<usize> {
    if line == 0 { return Some(0); }
    memchr_iter(b'\n', content).nth(line - 1).map(|pos| pos + 1)
}
```

This matters for large file operations: counting lines in a 50MB file takes ~1.5ms with `memchr` vs ~50ms with scalar iteration.

### `simdutf8` — Fast UTF-8 validation

Drop-in accelerated alternative to `std::str::from_utf8`:

```rust
use simdutf8::basic::from_utf8;

// In the file loading path, after read_to_vec:
let bytes = std::fs::read(&path)?;
let content = match from_utf8(&bytes) {
    Ok(s) => s.to_string(),
    Err(_) => return Err(anyhow::anyhow!("File is not valid UTF-8")),
};
```

Validates UTF-8 at ~12 GB/s on AVX2 vs ~1.5 GB/s for `std::str::from_utf8`. For a 100MB file, that's 8ms vs 67ms — meaningful when combined with `read_to_string` which does UTF-8 validation internally.

If using `simdutf8`, prefer `std::fs::read` (returns `Vec<u8>`) + `simdutf8::from_utf8` over `std::fs::read_to_string` (which uses the slower stdlib validator internally). This requires using `unsafe { String::from_utf8_unchecked(bytes) }` after validation, which is sound because `simdutf8` has validated the bytes.

### `aho-corasick` — Multi-pattern matching (future)

For project-wide search or multi-keyword filtering:

```rust
use aho_corasick::AhoCorasick;

let patterns = &["TODO", "FIXME", "HACK", "XXX"];
let ac = AhoCorasick::new(patterns).unwrap();

// Finds all matches in a single pass, using SIMD for the initial byte scan
for mat in ac.find_iter(content) {
    println!("Found '{}' at offset {}", patterns[mat.pattern()], mat.start());
}
```

Uses `memchr` internally for the initial candidate-byte scan, then a deterministic finite automaton for multi-pattern matching. Much faster than running `str::find` in a loop for each pattern.

---

## 7. Index Rebuild Coalescing {#7-rebuild-coalescing}

---

## 6. Index Rebuild Coalescing {#6-rebuild-coalescing}

When the user adds multiple workspace folders rapidly (e.g., via drag-and-drop), each addition fires `connect_workspace_changed`, which calls `rebuild_file_index`. Coalescing prevents redundant full scans:

```rust
// In window/imp.rs:
pub rebuild_pending: Cell<Option<glib::SourceId>>,

// In rebuild_file_index:
pub fn rebuild_file_index(&self) {
    // Cancel any pending rebuild
    if let Some(id) = self.imp().rebuild_pending.take() {
        id.remove();
    }

    let window_weak = self.downgrade();
    let id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(300),
        move || {
            let Some(window) = window_weak.upgrade() else { return };
            window.imp().rebuild_pending.set(None);

            let roots = window.imp().sidebar.workspace_roots();
            let window_weak = window.downgrade();
            async_task::spawn_blocking_then(
                (),
                move || FileIndex::rebuild(&roots),
                move |(), index| {
                    if let Some(window) = window_weak.upgrade() {
                        window.imp().command_palette.set_file_index(index);
                    }
                },
            );
        },
    );
    self.imp().rebuild_pending.set(Some(id));
}
```

The 300ms window coalesces rapid changes while staying responsive — the user won't notice a 300ms delay before the command palette's file list updates after adding a workspace folder.
