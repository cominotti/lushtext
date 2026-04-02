---
name: gtk-perf-scale
description: "Review and guide Rust code for performance at scale in GTK4/Libadwaita applications — large files, huge directory trees, massive file indexes, SIMD-accelerated fuzzy search, and byte-processing throughput. Auto-invoked when working on file loading/saving (editor_page), command palette search (palette.rs, command_palette/), file indexing (FileIndex, collect_files_recursive, nucleo), file tree scanning (file_tree.rs, workspace_section), async_task.rs, or any code that processes user files or directory listings. Also trigger when the user mentions performance, scalability, large files, slow search, memory usage, benchmarks, file count limits, SIMD, AVX2, NEON, vectorization, nucleo, memchr, or 'takes too long to open'. Use this skill proactively whenever new features touch file I/O, search, indexing, byte processing, or tree traversal — even if the user doesn't mention performance. LushText targets x86-64-v3 (AVX2 guaranteed) and Apple Silicon (NEON always available) — always prefer SIMD-accelerated crates over scalar implementations."
---

Guide and review Rust code for handling large data volumes in LushText without degrading the user experience. While `gtk-responsiveness` ensures the GTK main thread stays free (the 16ms frame budget), this skill ensures the *data path* scales — what happens when a user opens a 100MB log file, has 200k files across workspace roots, or types a fuzzy query into a command palette backed by a massive index.

## Relationship to gtk-responsiveness

These two skills are complementary, not overlapping:

| Concern | gtk-responsiveness | gtk-perf-scale (this skill) |
|---------|-------------------|----------------------------|
| Core question | "Does this block the main thread?" | "Does this scale to large inputs?" |
| Primary metric | Frame time (<16ms) | Throughput, memory, latency at 10x–100x typical load |
| Example fix | Move `fs::write` to background thread | Add file size check before loading; debounce search |
| When to trigger | Any UI code, signal handler, I/O call | File loading, search, indexing, tree traversal, benchmarking |

If you find a main-thread blocking issue while doing a scale audit, flag it but reference `gtk-responsiveness` for the fix pattern. This skill focuses on *what to do with the data* once it's off the main thread.

## Modes of Operation

### Audit Mode

When reviewing existing code or pull requests, produce a categorized report with findings at four severity levels:

- **[FLAG]** — Performance hazard. Will cause user-visible degradation (UI freeze, OOM, multi-second delay) at a defined threshold that real users can reach. Must fix before the threshold is hit.
- **[RECOMMEND]** — Scalability improvement. Works at current scale but degrades at a defined threshold. Fix before growth reaches that point.
- **[CONSIDER]** — Future optimization. Not a problem today. Note for when scale increases.
- **[GOOD]** — Existing scalable pattern. Reinforce and protect from regression.

See [Audit Report Format](#audit-report-format) below for the output template.

### Guidance Mode

When implementing new features, answer these questions before the implementation is complete:

1. What is the maximum input size this code path can encounter?
2. Does any hot-path operation scale worse than O(n) where n is the input size?
3. Is there a cancellation mechanism if the operation becomes stale?
4. What happens at 10x and 100x the typical input size?
5. Are there hard limits or graceful degradation for extreme inputs?

Provide the relevant patterns from this skill and reference files, tailored to the specific feature.

## Scale Thresholds

These thresholds are calibrated for GTK4/GtkSourceView5 on typical desktop hardware (4-core, 16GB RAM, SSD). Adjust for the target platform if known.

| Threshold | Value | Behavior | Rationale |
|-----------|-------|----------|-----------|
| Large file toast | 1 MB | Show informational toast: "Large file — some features may be slower" | Sets user expectations. GtkSourceView handles 1MB fine but undo history grows fast. |
| Disable syntax highlighting | 10 MB | `buffer.set_language(None)` + `buffer.set_highlight_syntax(false)` | GtkSourceView's regex-based syntax engine scans the full buffer for context. Above 10MB, initial highlight pass exceeds 500ms. GNOME Text Editor uses a similar threshold. |
| Disable undo history | 50 MB | Keep `begin_irreversible_action()` permanent (don't call `end_irreversible_action()`) | Each edit creates undo entries that roughly double memory for the buffer content. |
| Refuse to open | 500 MB | Show dialog: "File is too large to edit (N MB). Consider a pager like `less`." | `buffer.set_text()` for 500MB allocates ~1GB (GTK's internal UTF-8 + GapBuffer) and blocks the main thread for 5–10 seconds even with syntax highlighting off. |
| Search debounce | 150 ms | Delay `rebuild_results` after last keystroke | Avoids scoring the full index on every character. 150ms feels responsive while cutting work by 3–5x for typical typing speed. |
| Index rebuild debounce | 300 ms | Coalesce rapid `rebuild_file_index` calls | Adding multiple workspace folders fires `connect_workspace_changed` for each. 300ms coalesces these into one scan. |
| Max indexed files | 100,000 | Log warning, truncate index, consider `nucleo` for async search | Linear `search_items` scan over 100k entries takes >10ms per query on a single core. |
| Max directory entries | 10,000 | Truncate `ListStore` with "N more items..." sentinel row | A single `gio::ListStore` with >10k items causes slow model diff updates in `GtkListView`. |
| Thread spawn guard | 8 concurrent | Queue additional `spawn_blocking_then` calls | Unbounded `std::thread::spawn` under rapid operations (restoring 50 tabs) can exhaust OS thread limits or thrash the scheduler. |

## Topic 1: Large File Handling

The current `load_file_async` (`editor_page/mod.rs`) reads the entire file with `std::fs::read_to_string` on a background thread, then calls `buffer.set_text(content)` on the main thread. This works for typical source files (<100KB) but breaks down at scale:

- **No size check**: A 4GB log file will be `read_to_string`'d, allocating 4GB on the background thread, then passed to `set_text` which allocates another ~4GB on the main thread (GtkTextBuffer internal representation). Total: ~8GB for one tab.
- **No cancellation**: If the user closes the tab while the file is loading, the background thread continues reading, completes, and delivers the result to a callback that updates a now-irrelevant buffer.
- **Synchronous save**: `save_file()` calls `std::fs::write` directly on the main thread. For a 5MB file on a slow disk (NFS, USB), this freezes the UI for hundreds of milliseconds.

### What to implement

1. **Size-gated loading**: Check `fs::metadata(&path)?.len()` in the background `work` closure *before* reading content. Apply thresholds from the table above.
2. **Background save**: Move `std::fs::write` into `spawn_blocking_then`. Update `buffer.set_modified(false)` and status bar in the `then` callback.
3. **Cancel token**: Store an `Arc<AtomicBool>` per editor page. Set it on tab close. Check it before and after the `read_to_string` call.
4. **Syntax highlighting gate**: After loading, if `content.len() > 10MB`, skip `reapply_language()`.

See `references/large-file-patterns.md` for complete code examples.

## Topic 2: Scalable Search & Indexing

The command palette (`palette.rs` + `command_palette/imp.rs`) has a clean architecture: `FileIndex` holds `Vec<IndexedFile>`, `search_all` scores every entry with `fuzzy_score_chars`, sorts, and truncates to 50. However, `fuzzy_score_chars` is scalar code that processes one character at a time — with our guaranteed AVX2/NEON baseline, this is leaving a 2x+ speedup on the table. Additionally:

- **No SIMD**: The hand-rolled `fuzzy_score_chars` is scalar. `nucleo` achieves ~50ns/candidate vs ~100ns/candidate via SIMD-accelerated byte comparison, and provides better match quality (Smith-Waterman optimal alignment vs greedy subsequence).
- **No debounce**: `setup_search` connects `search_entry.connect_search_changed` directly to `rebuild_results`. Every keystroke re-scores the entire index.
- **No cancellation**: If the user types faster than scoring completes, stale results are computed and discarded.
- **Full rebuild on every workspace change**: `rebuild_file_index` does a complete recursive scan. Adding one file to a workspace with 100k entries re-scans everything.
- **Linear scan**: `search_items` iterates all entries and collects into a `Vec` before sorting. A `BinaryHeap` with capacity `max` would avoid sorting entirely.

### What to implement

1. **Replace `fuzzy_score_chars` with `nucleo`**: This is the single highest-impact change. `nucleo` provides SIMD-accelerated matching (AVX2 on x86-v3, NEON on aarch64), async background scoring via `Nucleo<T>`, and match position tracking for highlight rendering. With our target baseline, SIMD runs at full speed on every machine — no fallback paths needed.
2. **Debounce search input**: 150ms via `glib::timeout_add_local_once` with source ID cancellation. The pattern is already documented in `gtk-responsiveness` — apply it to `setup_search` in `command_palette/imp.rs`.
3. **Generation counter for stale results**: Increment a `Cell<u32>` on each query change. The rebuild closure captures the value; if it differs from current when results are ready, discard them.
4. **Index size guard**: After `FileIndex::rebuild`, if `files.len() > 100_000`, log a warning.
5. **Bounded heap**: Replace the `collect → sort → truncate` pattern in `search_items` with a min-heap of size `max`, yielding O(n log k) instead of O(n log n) where k=50 and n=file count.

See `references/search-scaling.md` for code examples, SIMD guidance, and `nucleo` integration.

## Topic 3: File Tree at Scale

The sidebar's `TreeListModel` with lazy per-expand scanning (`build_children_model` in `workspace_section/mod.rs`) is fundamentally sound — directories are only scanned when the user expands them, and each scan is async via `spawn_blocking_then`.

Remaining scale concerns:

- **Huge directories**: A single directory with 50k entries (e.g., `node_modules` before `.gitignore` filtering) will create 50k `FileTreeItem` GObjects in one `ListStore`. `GtkListView` virtualizes rendering, so scroll performance is fine, but the initial `append` loop takes ~100ms and `items-changed` fires for each append.
- **No scan coalescing**: Rapid expand/collapse cycles can spawn multiple scans for the same directory. Each completes independently and appends to the store.

### What to implement

1. **Batch append**: Use `gio::ListStore::splice()` instead of per-item `append()` to emit a single `items-changed` signal for the entire batch.
2. **Directory entry cap**: If `scan_directory` returns >10k entries, truncate and add a sentinel `FileTreeItem` with label "10,000+ items — showing first 10,000".
3. **Scan deduplication**: Store a generation counter or `Arc<AtomicBool>` per `TreeListRow`. If the node is collapsed before the scan completes, the `then` callback should no-op rather than appending stale results.

## Topic 4: Cancellation Patterns

Two cancellation patterns are relevant to LushText. Both are already documented in `gtk-responsiveness/references/async-patterns.md` as patterns #3 (cancellable load) and the generation counter concept from `widget-wiring.md`. This skill specifies *where* to apply them:

### Arc<AtomicBool> Cancel Token

Use for operations that are expensive and become irrelevant when the initiating context is destroyed:

| Operation | Cancel when | Store token on |
|-----------|-------------|----------------|
| File load (`load_file_async`) | Tab closed before load completes | `EditorPage` imp struct |
| File index rebuild | New rebuild starts before previous completes | `LushtextWindow` imp struct |
| Directory scan | Tree node collapsed before scan completes | `WorkspaceSection` per-row state |

### Generation Counter

Use for operations that are cheap to discard at the result-delivery stage:

| Operation | Increment when | Check in |
|-----------|---------------|----------|
| Search result rebuild | New query typed | `rebuild_results` — if counter changed since query started, discard results |
| Status bar message | New message pushed | Timer callback (already implemented) |

## Topic 5: Thread Management

Currently every `spawn_blocking_then` call creates a new OS thread via `std::thread::spawn`. This is fine for typical usage (1–5 concurrent operations), but degenerate cases exist:

- **Session restore with 50+ tabs**: Each tab spawns a file-read thread simultaneously.
- **Large workspace initial index + multiple directory expands**: Index scan + N directory scans all spawn threads.

### When to add a thread pool

A thread pool (`rayon::ThreadPool` or a simple `crossbeam` channel + worker threads) becomes worthwhile when:
- More than ~20 concurrent `spawn_blocking_then` calls are realistic
- Thread creation overhead (~50μs per thread on Linux) becomes measurable
- You need to prioritize operations (e.g., foreground tab load before background tabs)

For now, a simpler approach: add a semaphore (e.g., `Arc<Semaphore>`) to `spawn_blocking_then` that limits concurrent threads to `num_cpus::get()`. This prevents thread explosion without adding a full thread pool dependency.

## Topic 6: Memory at Scale

### Buffer memory

Each open tab holds the full file content in a `sourceview5::Buffer` (GtkTextBuffer). Internal representation is a B-tree of text segments with gap buffers. Approximate memory: `1.5x–2x file_size` per tab (text + line index + undo history).

For 20 tabs averaging 500KB each: ~20MB. Acceptable.
For 5 tabs with 50MB files each: ~500MB. Concerning on 8GB machines.

### Index memory

`FileIndex` holds `Vec<IndexedFile>` where each entry is: `PathBuf` (heap alloc) + `String` (name, heap alloc) + `PathBuf` (workspace_root, heap alloc). Approximate: ~200 bytes per entry. For 100k files: ~20MB. Acceptable but worth tracking.

### What to implement

1. **Pre-allocate with capacity hints**: `Vec::with_capacity` in `collect_files_recursive` when the directory entry count is known from `read_dir().count()` (though this double-reads the dir — only worthwhile if the directory is expected to be large).
2. **Share workspace_root**: Use `Arc<PathBuf>` or `Arc<str>` for `workspace_root` in `IndexedFile` instead of cloning the full `PathBuf` per file. For a workspace with 50k files, this saves ~2.4MB (50k * 48 bytes per PathBuf).
3. **Monitor memory**: Consider adding a debug-only memory reporter that logs total index size on rebuild.

## Topic 7: Benchmarking

LushText has no benchmarks yet. Adding `criterion` benchmarks for the hottest paths enables data-driven optimization and CI regression detection.

### Priority benchmark targets

| Target | Function | Why |
|--------|----------|-----|
| Fuzzy scoring | `fuzzy_score_chars` with 1/10/100 char candidates | This runs once per indexed file per query keystroke |
| Index search | `search_items` with 1k/10k/100k entries | End-to-end search latency determines if debounce is needed |
| Index rebuild | `FileIndex::rebuild` on a synthetic directory tree | Determines how fast workspace changes are reflected |
| Buffer set_text | `sourceview5::Buffer::set_text` with 1MB/10MB/50MB strings | Determines the large-file threshold (requires GTK init) |

See `references/benchmark-setup.md` for criterion configuration and example benches.

## Topic 8: SIMD-First Performance

LushText targets x86-64-v3 (AVX2 guaranteed) and Apple Silicon (NEON always available). This is a significant advantage: **every byte-processing operation should prefer SIMD-accelerated crates over scalar code**. There is no need for runtime feature detection or scalar fallbacks — SIMD runs at full speed on every target machine.

### The SIMD mandate

When choosing between a hand-rolled scalar implementation and a crate that uses SIMD internally, always choose the SIMD crate. The performance gap is not marginal — it's typically 2x–8x for string/byte operations. On our guaranteed baseline:

- **x86-64-v3**: 256-bit AVX2 vectors process 32 bytes per instruction
- **aarch64 (Apple Silicon)**: 128-bit NEON vectors process 16 bytes per instruction, but with wider issue and better throughput per clock

### Operations that benefit from SIMD

| Operation | Scalar approach | SIMD crate | Speedup | LushText use case |
|-----------|----------------|------------|---------|-------------------|
| Fuzzy matching | `fuzzy_score_chars` (hand-rolled) | `nucleo` | ~2x | Command palette search — the #1 priority |
| Byte search (newline, char) | `str::find`, `iter().position()` | `memchr` | 3–8x | Line counting, newline scanning in large files |
| UTF-8 validation | `std::str::from_utf8` | `simdutf8` | 4–8x | Validating large file content before buffer insertion |
| Multi-pattern search | Naive loop | `aho-corasick` | 5–20x | Future: multi-keyword search, syntax token scanning |
| Case folding | `char::to_lowercase()` per char | Bulk via `nucleo`'s internal normalizer | 2–4x | Case-insensitive search across index |

### Priority adoption order

1. **`nucleo`** (immediate): Replace `fuzzy_score_chars` and the entire `search_items` pipeline. This is the highest-impact change — it affects every command palette interaction. `nucleo` handles SIMD matching, async scoring, and match position tracking (for highlight rendering in results). See `references/search-scaling.md` for integration guide.

2. **`memchr`** (on large file work): Already a transitive dependency of many Rust crates. Use it explicitly for newline counting (`memchr::memchr_iter(b'\n', bytes).count()`) in large file stat operations and line-number computation. Processes 32 bytes/cycle on AVX2 vs 1 byte/cycle scalar.

3. **`simdutf8`** (on large file work): Drop-in replacement for `std::str::from_utf8` that validates UTF-8 at 12 GB/s on AVX2 vs ~1.5 GB/s scalar. Relevant when loading large files — validate before `String::from_utf8_unchecked` instead of paying for `read_to_string`'s built-in validation.

4. **`aho-corasick`** (future: multi-keyword search): If LushText adds project-wide search (grep-like), use `aho-corasick` for multi-pattern matching. It uses SIMD for the initial byte scan (via `memchr` internally) and builds a finite automaton for pattern matching.

### Build configuration for SIMD

For maximum SIMD performance, set target CPU in release builds:

```makefile
# In Makefile, for release builds targeting the host machine:
RUSTFLAGS += -C target-cpu=native

# For distributed builds (Flatpak) with known baseline:
# x86-64: use x86-64-v3 (AVX2 guaranteed)
# aarch64: NEON is always available, no flag needed
```

Most SIMD crates (`nucleo`, `memchr`, `aho-corasick`) use runtime feature detection via `std::is_x86_feature_detected!` and work without special RUSTFLAGS. But setting `target-cpu` allows the compiler to also auto-vectorize loops and use wider instructions for non-SIMD code paths.

### What NOT to do

- **Don't write raw SIMD intrinsics** (`std::arch::x86_64::_mm256_*`). Use crates that encapsulate SIMD internally. Hand-written intrinsics are error-prone, require unsafe, and need per-architecture implementations.
- **Don't add SIMD for operations under 1KB**. The setup cost of SIMD (loading data into vector registers) only pays off when processing enough data. For small strings (<64 bytes), scalar code is often faster due to reduced overhead.
- **Don't disable runtime detection in SIMD crates**. Even though we know AVX2/NEON is available, let the crates handle detection — it's free (amortized to once per process) and future-proofs against running on unexpected hardware.

See `references/search-scaling.md` for detailed `nucleo` integration and SIMD crate adoption patterns.

## Audit Report Format

When running in audit mode, produce a report using this template:

```
## Performance Scale Audit

### Summary
- **Files reviewed**: N
- **Findings**: X flag, Y recommend, Z consider, W good
- **Estimated scale ceiling**: "current code handles ~Nk files / ~NMB documents comfortably"

### [FLAG] Title — file:line
Description of the issue.
**Threshold**: At what input size does this become a problem?
**Impact**: What does the user experience? (freeze, OOM, slow response)
**Fix**: Concrete recommendation with code sketch or reference to pattern.

### [RECOMMEND] Title — file:line
...

### [CONSIDER] Title — file:line
...

### [GOOD] Title — file:line
Why this pattern is correct and what it handles well.
```

Always quantify thresholds ("10k files", "50MB", "100ms") rather than using vague terms ("large", "many", "slow").

## Anti-Patterns to Flag

### [FLAG] — No file size check before loading

```rust
// BAD: will happily read a 4GB file into memory
std::fs::read_to_string(&path)

// GOOD: check size first, apply thresholds
let meta = std::fs::metadata(&path)?;
if meta.len() > MAX_FILE_SIZE { return Err(...); }
```

### [FLAG] — Synchronous file write on main thread

```rust
// BAD: blocks UI for the duration of the write
std::fs::write(&path, text.as_str())?;

// GOOD: use spawn_blocking_then (see references/large-file-patterns.md)
```

### [FLAG] — Unbounded index scan without debounce

```rust
// BAD: every keystroke re-scores the entire index
entry.connect_search_changed(move |entry| {
    rebuild_results(&entry.text());
});

// GOOD: debounce with 150ms delay (see references/search-scaling.md)
```

### [RECOMMEND] — collect + sort + truncate for top-N results

```rust
// ACCEPTABLE but suboptimal for large N:
let mut results: Vec<_> = items.filter_map(|i| score(i)).collect();
results.sort_by(|a, b| b.score.cmp(&a.score));
results.truncate(max);

// BETTER for large collections: BinaryHeap with capacity max
```

### [RECOMMEND] — Full index rebuild on incremental change

```rust
// Current: re-scans entire workspace on every change
FileIndex::rebuild(&roots)

// Better: delta updates (add/remove individual files)
```

### [RECOMMEND] — Per-item ListStore append in a loop

```rust
// BAD: fires items-changed N times
for item in items { store.append(&item); }

// GOOD: fires items-changed once
store.splice(0, 0, &items);
```

### [FLAG] — Hand-rolled fuzzy scoring without SIMD

The current `fuzzy_score_chars` is scalar code — it processes one character at a time. With our x86-v3 + Apple Silicon baseline guaranteeing AVX2/NEON, this leaves significant performance on the table. Replace with `nucleo` which uses SIMD-accelerated matching, provides async batch scoring, and returns match positions for highlighting. See Topic 8 and `references/search-scaling.md` for details.

## Tone

Scale advice must be grounded in numbers. Instead of "this might be slow with many files," say "at 100k indexed files, `search_items` takes ~12ms per query, which exceeds the 150ms debounce budget if the user types 10 chars/second." Acknowledge what works well before suggesting improvements — the existing `FileIndex` architecture with background rebuild and capped result display is solid; the gaps are in the margins (debounce, cancellation, size guards).
