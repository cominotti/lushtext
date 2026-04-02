---
name: gtk-perf-scale
description: "Review and guide Rust code for performance at scale in GTK4/Libadwaita applications — large files, huge directory trees, massive file indexes, SIMD-accelerated fuzzy search, byte-processing throughput, and RAM efficiency. Uses parallel subagents for deterministic, focused reviews. Auto-invoked when working on file loading/saving (editor_page), command palette search (palette.rs, command_palette/), file indexing (FileIndex, collect_files_recursive, nucleo), file tree scanning (file_tree.rs, workspace_section), async_task.rs, or any code that processes user files or directory listings. Also trigger when the user mentions performance, scalability, large files, slow search, memory usage, RAM, benchmarks, file count limits, SIMD, AVX2, NEON, vectorization, nucleo, memchr, or 'takes too long to open'. Use this skill proactively whenever new features touch file I/O, search, indexing, byte processing, or tree traversal — even if the user doesn't mention performance."
---

Guide and review Rust code for handling large data volumes in LushText without degrading the user experience or exhausting RAM. While `gtk-responsiveness` ensures the GTK main thread stays free (the 16ms frame budget), this skill ensures the *data path* scales — what happens when a user opens a 100MB log file, has 200k files across workspace roots, or types a fuzzy query into a command palette backed by a massive index.

## Relationship to gtk-responsiveness

These two skills are complementary, not overlapping:

| Concern | gtk-responsiveness | gtk-perf-scale (this skill) |
|---------|-------------------|----------------------------|
| Core question | "Does this block the main thread?" | "Does this scale to large inputs?" |
| Primary metric | Frame time (<16ms) | Throughput, memory, latency at 10x–100x typical load |
| RAM focus | Closure captures, signal handler leaks | Buffer memory, index memory, clone avoidance |
| Example fix | Move `fs::write` to background thread | Add file size check before loading; debounce search |

If you find a main-thread blocking issue while doing a scale audit, flag it but reference `gtk-responsiveness` for the fix pattern.

## Execution Model: Parallel Subagents

This skill uses **parallel subagents** for independent review concerns. Do NOT attempt to review all concerns inline — dispatch focused subagents instead.

### Workflow

1. **Identify changed files** — run `git diff --name-only` (or use the diff context if already available)
2. **Match trigger patterns** — for each subagent, check if any changed files match its triggers
3. **Always include ram-budget-audit** — memory is a cross-cutting concern that applies to every change
4. **Dispatch threshold** — if fewer than 2 subagents are relevant, run the review inline using only the relevant subagent's criteria (subagent overhead not justified for a single focused concern)
5. **Dispatch relevant subagents in parallel** via the Agent tool
6. **Aggregate results** — merge findings, deduplicate, produce the final report

### Subagent Prompt Template

Every subagent prompt MUST include:

1. The list of changed files relevant to its concern
2. Instruction to read its assigned reference file (if any)
3. The Scale Thresholds table (below)
4. The RAM Preamble (below)
5. Its specific review criteria and anti-patterns
6. The output format (severity-tagged findings)

### RAM Preamble (inject into every subagent prompt)

> While reviewing, also check for memory efficiency. Flag: unnecessary `.clone()` calls (prefer `&str`, `Arc`, `Cow`), large captures in closures, missing `Vec::with_capacity()` hints, `PathBuf` cloning where `Arc<PathBuf>` sharing is appropriate, `retain()` without `shrink_to_fit()` after bulk removes. For thresholds and patterns, read `references/ram-budget.md` in the `gtk-perf-scale` skill directory.

## Scale Thresholds

These thresholds are calibrated for GTK4/GtkSourceView5 on typical desktop hardware (4-core, 8-16GB RAM, SSD). Include this table in every subagent prompt.

| Threshold | Value | Behavior | Rationale |
|-----------|-------|----------|-----------|
| Large file toast | 1 MB | Show informational toast | Sets user expectations. GtkSourceView handles 1MB fine but undo history grows fast. |
| Disable syntax highlighting | 10 MB | `buffer.set_language(None)` | GtkSourceView's regex engine scans full buffer. Above 10MB, initial pass exceeds 500ms. |
| Disable undo history | 50 MB | `begin_irreversible_action()` permanent | Each edit creates undo entries ~doubling memory for the buffer content. |
| Refuse to open | 500 MB | Show dialog | `buffer.set_text()` for 500MB allocates ~1GB and blocks main thread for 5-10s. |
| Search debounce | 150 ms | Delay `rebuild_results` | Avoids scoring full index on every character. |
| Index rebuild debounce | 300 ms | Coalesce `rebuild_file_index` calls | Adding multiple workspace folders fires events for each. |
| Max indexed files | 100,000 | Log warning, truncate | Linear scan over 100k entries takes >10ms per query. |
| Max directory entries | 10,000 | Truncate ListStore with sentinel | >10k items causes slow model diff updates in GtkListView. |
| Thread spawn guard | 8 concurrent | Queue additional calls | Unbounded spawns can exhaust OS thread limits and RAM (8 * file_size peak). |

## Severity Levels

- **[FLAG]** — Performance hazard. User-visible degradation (UI freeze, OOM, multi-second delay) at a threshold real users can reach. Must fix.
- **[RECOMMEND]** — Scalability improvement. Works at current scale but degrades at a defined threshold. Fix before growth reaches that point.
- **[CONSIDER]** — Future optimization. Not a problem today.
- **[GOOD]** — Existing scalable pattern. Reinforce and protect from regression.

## Subagent Definitions

### 1. large-file-audit

**Triggers**: Changes to `editor_page/`, `file_limits.rs`, or any file containing `read_to_string`, `fs::write`, `fs::read`, `fs::metadata` in a file-loading context.

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for large-file handling correctness.

Read the reference file at: .claude/skills/gtk-perf-scale/references/large-file-patterns.md

Changed files to review:
{changed_files}

{scale_thresholds_table}

{ram_preamble}

Review criteria:
- Size-gated loading: does the code check fs::metadata size BEFORE read_to_string? Are thresholds applied (1MB toast, 10MB no syntax, 50MB no undo, 500MB refuse)?
- Background save: is fs::write moved to spawn_blocking_then? Is buffer.set_modified(false) called optimistically with rollback on failure?
- Cancel token: does EditorPage store Arc<AtomicBool>? Is it checked before AND after read_to_string? Is cancel_load() called on tab close?
- Syntax gate: is reapply_language() skipped for files >10MB?
- Buffer memory: peak memory during set_text() is ~2.5-3x file_size. Is this acceptable for the file sizes being handled?

Anti-patterns to flag:
- [FLAG] No file size check before read_to_string (will happily read a 4GB file)
- [FLAG] Synchronous fs::write on main thread (blocks UI)
- [FLAG] Missing cancel token on EditorPage (wasted work on tab close)
- [RECOMMEND] Missing syntax highlighting gate for files >10MB

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Threshold. Impact. Fix.
```

### 2. search-index-audit

**Triggers**: Changes to `palette.rs`, `command_palette/`, or any file containing `FileIndex`, `nucleo`, `fuzzy_score`, `search_items`.

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for search and indexing scalability.

Read the reference file at: .claude/skills/gtk-perf-scale/references/search-scaling.md

Changed files to review:
{changed_files}

{scale_thresholds_table}

{ram_preamble}

Review criteria:
- SIMD usage: is nucleo-matcher used for fuzzy scoring? Hand-rolled scalar scoring leaves 2x+ speedup on the table (AVX2/NEON guaranteed).
- Search debounce: is there a 150ms debounce on search input? Empty queries should bypass debounce for instant clear.
- Generation counter: does the rebuild check for stale results via Cell<u32> counter?
- Index size guard: is FileIndex capped at 100,000 entries with a warning log?
- Bounded top-N: does search use BinaryHeap with capacity max (O(n log k)) instead of collect+sort+truncate (O(n log n))?
- Matcher reuse: does search_items reuse a single Matcher and char buffer across candidates, or allocate per-candidate?
- Index memory: Arc<PathBuf> sharing for workspace_root? Vec::with_capacity hints?
- SIMD crate usage: for byte-processing operations (newline counting, UTF-8 validation, multi-pattern search), is a SIMD-accelerated crate used instead of scalar code? memchr for byte scanning (~32 bytes/cycle on AVX2), simdutf8 for UTF-8 validation (~12 GB/s on AVX2), aho-corasick for multi-pattern matching. See references/search-scaling.md section 6 for details.

Anti-patterns to flag:
- [FLAG] Hand-rolled fuzzy scoring without SIMD (scalar fuzzy_score_chars)
- [FLAG] No debounce on search input (every keystroke re-scores entire index)
- [RECOMMEND] collect+sort+truncate for top-N instead of bounded heap
- [RECOMMEND] Full index rebuild on incremental change (single file add/remove)
- [RECOMMEND] Per-item ListStore append instead of splice()
- [RECOMMEND] Scalar byte processing (str::find, iter().position(), std::str::from_utf8) where a SIMD crate (memchr, simdutf8) would provide 3-8x speedup on data >1KB

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Threshold. Impact. Fix.
```

### 3. file-tree-audit

**Triggers**: Changes to `file_tree.rs`, `workspace_section/`, or any file containing `TreeListModel`, `ListStore`, `build_children_model`.

**Subagent prompt** (self-contained — no reference file needed):
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for file tree scalability.

Changed files to review:
{changed_files}

{scale_thresholds_table}

{ram_preamble}

Review criteria:
- Batch updates: does the code use gio::ListStore::splice() for batch appends (single items-changed signal) instead of per-item append() loops?
- Directory entry cap: if scan_directory returns >10,000 entries, are they truncated with a sentinel FileTreeItem ("10,000+ items — showing first 10,000")?
- Scan deduplication: is there a generation counter or cancel token per TreeListRow? If a node is collapsed before scan completes, does the then callback no-op?
- ListStore memory: each FileTreeItem GObject costs ~300-400 bytes. 10k items = ~4MB. 50k items (no cap) = ~20MB of GObjects in one store.
- splice() vs append() memory: splice() does a single reallocation; per-item append() may trigger multiple reallocations with up to 2x the final memory during growth.

Anti-patterns to flag:
- [FLAG] Per-item append() loop for >100 items (fires items-changed N times, 2x memory during growth)
- [RECOMMEND] Missing directory entry cap (a single node_modules with 50k entries = ~20MB of GObjects)
- [RECOMMEND] No scan deduplication (rapid expand/collapse spawns multiple scans for same directory)
- [GOOD] Lazy TreeListModel population (empty store returned immediately, populated async)

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Threshold. Impact. Fix.
```

### 4. ram-budget-audit

**Triggers**: **Always runs** — memory is a cross-cutting concern.

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor specifically for memory efficiency and low RAM usage.

Read the reference file at: .claude/skills/gtk-perf-scale/references/ram-budget.md

Changed files to review:
{changed_files}

Review criteria — check every changed file for:
1. Clone avoidance: unnecessary String::clone(), PathBuf::clone() where &str or Arc would suffice
2. Arc sharing: PathBuf values that appear in multiple data structures — should they use Arc<PathBuf>?
3. Closure captures: signal handler closures capturing large state (Vec, HashMap, entire structs). Prefer @weak refs and imp() access.
4. Vec capacity: collections built in loops — is Vec::with_capacity used when the approximate size is known?
5. Background thread peak memory: does read_to_string or similar I/O peak at 2x+ file size? Can intermediates be scoped/dropped earlier?
6. retain() without shrink_to_fit(): after bulk removes, is excess Vec capacity reclaimed?
7. Buffer memory awareness: code that opens files or creates tabs — does it account for ~1.5-2x file size per tab in GtkTextBuffer?
8. Concurrent load budgeting: session restore or batch operations — is peak memory capped by the thread spawn guard (8 concurrent)?

Anti-patterns to flag:
- [FLAG] PathBuf clone per-file instead of Arc<PathBuf> per-workspace (50k files = ~4MB waste)
- [FLAG] Capturing entire FileIndex or Vec<IndexedFile> (~20MB for 100k files) in a signal closure
- [RECOMMEND] Missing Vec::with_capacity for known-size collections (up to 50% waste from doubling)
- [RECOMMEND] String::clone() where &str would suffice
- [RECOMMEND] to_string_lossy().to_string() where .into_owned() suffices — see gtk-perf-rust-optimize/references/allocation-patterns.md for full pattern catalog
- [RECOMMEND] retain() without shrink_to_fit() after removing >25% of entries
- [RECOMMEND] Unbounded concurrent spawn_blocking_then without a thread spawn guard — 50 simultaneous file loads peak at 50 * file_size RAM. Check for semaphore or batching (groups of 8).
- [CONSIDER] Buffer eviction for apps with many large-file tabs (threshold: >256MB total)

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Memory impact (quantified). Fix.
```

### 5. benchmark-audit

**Triggers**: Changes to `benches/`, `Cargo.toml` benchmark config, or when another subagent's findings suggest a new hot path lacks benchmarks.

**Subagent prompt**:
```
You are reviewing benchmark coverage for a GTK4/Libadwaita text editor's performance-sensitive code.

Read the reference file at: .claude/skills/gtk-perf-scale/references/benchmark-setup.md

Changed files to review:
{changed_files}

Review criteria:
- Are new performance-sensitive functions covered by Criterion benchmarks?
- Do existing benchmarks cover the range of input sizes (1k, 10k, 100k for indexing; 1MB, 10MB, 50MB for file ops)?
- Is the bench profile configured correctly (opt-level = 3, lto = "thin", codegen-units = 1, no strip)?
- Are benchmark baselines being used for regression detection?

Priority benchmark targets:
| Target | Function | Why |
|--------|----------|-----|
| Fuzzy scoring | fuzzy_score / nucleo scoring | Runs once per indexed file per query keystroke |
| Index search | search_items with 1k/10k/100k entries | End-to-end search latency |
| Index rebuild | FileIndex::rebuild on synthetic tree | Workspace change reflection speed |
| Directory scan | scan_directory on various sizes | File tree expansion time |

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. What to benchmark. Expected baseline.
```

## Aggregation

After all subagents return, produce the unified report:

1. **Merge findings** — combine all [FLAG], [RECOMMEND], [CONSIDER], [GOOD] items from all subagents
2. **Deduplicate** — if two subagents flag the same line (e.g., large-file-audit and ram-budget-audit both flag a `read_to_string`), keep the more specific finding
3. **RAM summary** — add a "Memory Impact" subsection: estimated per-operation memory cost of the changed code, any new allocations that scale with input size, improvements found
4. **Sort by severity** — FLAG first, then RECOMMEND, CONSIDER, GOOD
5. **Cross-references** — if a finding relates to `gtk-responsiveness` (e.g., blocking I/O), add the cross-reference

## Audit Report Format

```
## Performance Scale Audit

### Summary
- **Files reviewed**: N
- **Findings**: X flag, Y recommend, Z consider, W good
- **Estimated scale ceiling**: "current code handles ~Nk files / ~NMB documents comfortably"
- **RAM impact**: Brief summary of memory implications

### [FLAG] Title — file:line
Description of the issue.
**Threshold**: At what input size does this become a problem?
**Impact**: What does the user experience? (freeze, OOM, slow response)
**Memory**: Quantified RAM impact if applicable.
**Fix**: Concrete recommendation with code sketch or reference to pattern.

### [RECOMMEND] Title — file:line
...

### [CONSIDER] Title — file:line
...

### [GOOD] Title — file:line
Why this pattern is correct and what it handles well.
```

Always quantify thresholds ("10k files", "50MB", "100ms", "~20MB RAM") rather than using vague terms ("large", "many", "slow").

## Guidance Mode

When implementing new features (not reviewing existing code), answer these questions before the implementation is complete:

1. What is the maximum input size this code path can encounter?
2. Does any hot-path operation scale worse than O(n)?
3. Is there a cancellation mechanism if the operation becomes stale?
4. What happens at 10x and 100x the typical input size?
5. What is the peak RAM usage of this operation? Does it scale linearly with input?
6. Are there hard limits or graceful degradation for extreme inputs?

## Tone

Scale advice must be grounded in numbers. Instead of "this might be slow with many files," say "at 100k indexed files, `search_items` takes ~12ms per query." Instead of "this uses a lot of memory," say "50k PathBuf clones waste ~4MB vs Arc<PathBuf> sharing at ~0.4MB." Acknowledge what works well before suggesting improvements.
