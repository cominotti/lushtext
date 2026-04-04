---
name: gtk-perf-scale
description: "Review and guide Rust code for performance at scale in GTK4/Libadwaita applications — large files, huge directory trees, massive file indexes, SIMD-accelerated fuzzy search, and RAM efficiency at architectural level. Uses parallel subagents for deterministic, focused reviews. Auto-invoked when working on file loading/saving (editor_page), command palette search (palette.rs, command_palette/), file indexing (FileIndex, collect_files_recursive, nucleo), file tree scanning (file_tree.rs, workspace_section), async_task.rs, or any code that processes user files or directory listings. Also trigger when the user mentions performance, scalability, large files, slow search, memory usage, RAM, benchmarks, file count limits, or 'takes too long to open'. Use this skill proactively whenever new features touch file I/O, search, indexing, or tree traversal — even if the user doesn't mention performance."
---

Guide and review Rust code for handling large data volumes in LushText without degrading the user experience or exhausting RAM. While `gtk-responsiveness` ensures the GTK main thread stays free (the 16ms frame budget), this skill ensures the *data path* scales — what happens when a user opens a 100MB log file, has 200k files across workspace roots, or types a fuzzy query into a command palette backed by a massive index.

## Philosophy: Readability First

Performance at scale matters, but **code readability is the top priority**. Every recommendation must pass the readability gate:

> **Would a new contributor understand this code on first read?**

This means:
- **Only flag things a real user would notice at scale.** A UI freeze on a 10k-entry directory matters. Saving a few Vec reallocations on a 50-item collection does not.
- **Prefer simple, clear code over micro-optimized code.** `.clone()` is fine when the alternative adds indirection. `format!()` is fine. Small intermediate collections are fine.
- **Focus on architectural decisions, not allocation counting.** Does the code have a size limit? Does it degrade gracefully? Is there a cap? Those matter. Whether `Vec::with_capacity` was called for a 20-item list does not.

### What We Do NOT Flag

- **`Vec::with_capacity()` for collections under ~1000 items** — the doubling strategy is fine
- **`retain()` without `shrink_to_fit()`** — the OS reclaims pages; explicit shrinking adds noise
- **`String::clone()` or `PathBuf::clone()` in non-hot paths** — setup code, signal wiring, error paths
- **Trivial allocation patterns** — `.to_string()`, `format!()`, intermediate collections under 1000 items
- **Drop ordering micro-optimization** — scoping intermediates to free memory 1ms earlier

## Relationship to gtk-responsiveness

These two skills are complementary, not overlapping:

| Concern | gtk-responsiveness | gtk-perf-scale (this skill) |
|---------|-------------------|----------------------------|
| Core question | "Does this block the main thread?" | "Does this scale to large inputs?" |
| Primary metric | Frame time (<16ms) | Throughput, memory, latency at 10x-100x typical load |
| RAM focus | Signal handler leaks, reference cycles | Buffer memory budgets, index caps, concurrent load limits |
| Example fix | Move `fs::write` to background thread | Add file size check before loading; cap directory entries |

If you find a main-thread blocking issue while doing a scale audit, flag it but reference `gtk-responsiveness` for the fix pattern.

## Execution Model: Parallel Subagents

This skill uses **parallel subagents** for independent review concerns. Do NOT attempt to review all concerns inline — dispatch focused subagents instead.

### Workflow

1. **Identify changed files** — run `git diff --name-only` (or use the diff context if already available)
2. **Match trigger patterns** — for each subagent below, check its path globs and content patterns against the file list. A subagent triggers if any changed file matches a listed path glob OR contains a listed content pattern.
3. **Always include ram-budget-audit** — architectural memory concerns are cross-cutting; it always triggers regardless of which files changed.
4. **Dispatch all relevant subagents in parallel** via the Agent tool — even if only one triggers (beyond ram-budget-audit), always dispatch as a subagent for consistent output format. In each prompt, replace `{changed_files}` with the actual file list from step 1.
5. **Aggregate results** — merge findings, deduplicate, produce the final report

### Subagent Prompts

Each subagent prompt below is self-contained — all necessary context (scale thresholds, review criteria, anti-patterns) is included inline. No template expansion is needed except replacing `{changed_files}` with the actual file list from step 1.

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

- **[FLAG]** — User-visible degradation. UI freeze, OOM, or multi-second delay at a threshold real users can reach. Must fix.
- **[RECOMMEND]** — Scaling hazard. Works at current typical scale but breaks at a defined threshold that real users could reach. Fix proactively.
- **[CONSIDER]** — Future-proofing. Not a problem at any realistic current scale but good to know about.
- **[GOOD]** — Existing scalable pattern. Reinforce and protect from regression.

## Subagent Definitions

### 1. large-file-audit

**Triggers**:
- paths: `ui/editor_page/**/*.rs`, `services/file_limits.rs`
- content: `read_to_string|fs::write|fs::read|fs::metadata`

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for large-file handling correctness.

Read the reference file at: .claude/skills/gtk-perf-scale/references/large-file-patterns.md

Changed files to review:
{changed_files}

Scale Thresholds (calibrated for GTK4/GtkSourceView5 on 4-core, 8-16GB RAM, SSD):

| Threshold | Value | Behavior |
|-----------|-------|----------|
| Large file toast | 1 MB | Show informational toast |
| Disable syntax highlighting | 10 MB | `buffer.set_language(None)` |
| Disable undo history | 50 MB | `begin_irreversible_action()` permanent |
| Refuse to open | 500 MB | Show dialog |
| Search debounce | 150 ms | Delay `rebuild_results` |
| Index rebuild debounce | 300 ms | Coalesce `rebuild_file_index` calls |
| Max indexed files | 100,000 | Log warning, truncate |
| Max directory entries | 10,000 | Truncate ListStore with sentinel |
| Thread spawn guard | 8 concurrent | Queue additional calls |

Review criteria:
- Size-gated loading: does the code check fs::metadata size BEFORE read_to_string? Are thresholds applied (1MB toast, 10MB no syntax, 50MB no undo, 500MB refuse)?
- Background save: is fs::write moved to spawn_blocking_then? Is buffer.set_modified(false) called optimistically with rollback on failure?
- Cancel token: does EditorPage store Arc<AtomicBool>? Is it checked before AND after read_to_string? Is cancel_load() called on tab close?
- Syntax gate: is reapply_language() skipped for files >10MB?

IMPORTANT: Do NOT flag micro allocation patterns (drop ordering, intermediate scoping, clone avoidance). Focus on the architectural decisions: are size limits in place? Is there graceful degradation?

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

**Triggers**:
- paths: `services/palette.rs`, `ui/command_palette/**/*.rs`
- content: `FileIndex|nucleo|fuzzy_score|search_items`

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for search and indexing scalability.

Read the reference file at: .claude/skills/gtk-perf-scale/references/search-scaling.md

Changed files to review:
{changed_files}

Scale Thresholds (calibrated for GTK4/GtkSourceView5 on 4-core, 8-16GB RAM, SSD):

| Threshold | Value | Behavior |
|-----------|-------|----------|
| Large file toast | 1 MB | Show informational toast |
| Disable syntax highlighting | 10 MB | `buffer.set_language(None)` |
| Disable undo history | 50 MB | `begin_irreversible_action()` permanent |
| Refuse to open | 500 MB | Show dialog |
| Search debounce | 150 ms | Delay `rebuild_results` |
| Index rebuild debounce | 300 ms | Coalesce `rebuild_file_index` calls |
| Max indexed files | 100,000 | Log warning, truncate |
| Max directory entries | 10,000 | Truncate ListStore with sentinel |
| Thread spawn guard | 8 concurrent | Queue additional calls |

Review criteria:
- SIMD usage: is nucleo-matcher used for fuzzy scoring? This is an established pattern in the codebase — new search code should follow it.
- Search debounce: is there a 150ms debounce on search input? Empty queries should bypass debounce for instant clear.
- Generation counter: does the rebuild check for stale results via Cell<u32> counter?
- Index size guard: is FileIndex capped at 100,000 entries with a warning log?
- Top-N results: does search use collect + sort_unstable_by + truncate(max)? With k=50 fixed, Vec+sort is the preferred pattern for readability.
- Matcher reuse: does search_items reuse a single Matcher and char buffer across candidates?

IMPORTANT: Do NOT flag micro allocation patterns. Focus on algorithmic scaling and established SIMD patterns.

Anti-patterns to flag:
- [FLAG] No debounce on search input (every keystroke re-scores entire index)
- [FLAG] Missing index size cap (unbounded growth with large workspaces)
- [RECOMMEND] collect+sort+truncate for top-N instead of bounded heap
- [RECOMMEND] Full index rebuild on incremental change (single file add/remove)
- [RECOMMEND] Per-item ListStore append instead of splice()
- [GOOD] nucleo-matcher used for fuzzy search with Matcher reuse across candidates

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Threshold. Impact. Fix.
```

### 3. file-tree-audit

**Triggers**:
- paths: `services/file_tree.rs`, `ui/sidebar/workspace_section/**/*.rs`
- content: `TreeListModel|ListStore|build_children_model`

**Subagent prompt** (self-contained — no reference file needed):
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for file tree scalability.

Changed files to review:
{changed_files}

Scale Thresholds (calibrated for GTK4/GtkSourceView5 on 4-core, 8-16GB RAM, SSD):

| Threshold | Value | Behavior |
|-----------|-------|----------|
| Large file toast | 1 MB | Show informational toast |
| Disable syntax highlighting | 10 MB | `buffer.set_language(None)` |
| Disable undo history | 50 MB | `begin_irreversible_action()` permanent |
| Refuse to open | 500 MB | Show dialog |
| Search debounce | 150 ms | Delay `rebuild_results` |
| Index rebuild debounce | 300 ms | Coalesce `rebuild_file_index` calls |
| Max indexed files | 100,000 | Log warning, truncate |
| Max directory entries | 10,000 | Truncate ListStore with sentinel |
| Thread spawn guard | 8 concurrent | Queue additional calls |

Review criteria:
- Batch updates: does the code use gio::ListStore::splice() for batch appends (single items-changed signal) instead of per-item append() loops?
- Directory entry cap: if scan_directory returns >10,000 entries, are they truncated with a sentinel FileTreeItem ("10,000+ items — showing first 10,000")?
- Scan deduplication: is there a generation counter or cancel token per TreeListRow? If a node is collapsed before scan completes, does the then callback no-op?

IMPORTANT: Do NOT flag micro memory concerns (GObject overhead per item, splice vs append memory patterns). Focus on whether the code has caps and handles large directories gracefully.

Anti-patterns to flag:
- [FLAG] Per-item append() loop for >100 items (fires items-changed N times, visible jank in UI)
- [RECOMMEND] Missing directory entry cap (a single node_modules with 50k entries freezes the UI)
- [RECOMMEND] No scan deduplication (rapid expand/collapse spawns multiple scans for same directory)
- [GOOD] Lazy TreeListModel population (empty store returned immediately, populated async)

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Threshold. Impact. Fix.
```

### 4. ram-budget-audit

**Triggers**: **Always runs** — architectural memory decisions are cross-cutting.

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for architectural memory concerns — the kind that can lead to OOM or excessive RAM usage with many open tabs or large workspaces.

Read the reference file at: .claude/skills/gtk-perf-scale/references/ram-budget.md

Changed files to review:
{changed_files}

IMPORTANT: This audit is about ARCHITECTURAL memory decisions, not micro allocation patterns. Do NOT flag:
- Missing Vec::with_capacity() for small collections
- String::clone() or PathBuf::clone() in non-hot paths
- retain() without shrink_to_fit()
- Trivial allocation differences (to_string vs into_owned, format! vs push_str)
- Drop ordering or intermediate scoping

Review criteria — check for architectural memory concerns:
1. Buffer memory awareness: code that opens files or creates tabs — does it account for ~1.5-2x file size per tab in GtkTextBuffer? Are the established size limits (50MB undo disable, 500MB refuse) respected?
2. Concurrent load budgeting: session restore or batch operations — is peak memory capped by the thread spawn guard (8 concurrent)?
3. Index memory scaling: new code adding to FileIndex — does it respect the 100k file cap?
4. Large data in signal closures: closures that capture large indexes or collections by value (these live for widget lifetime and become stale) — should use @weak ref + imp() access instead
5. Buffer eviction: are new tab-opening paths aware of the 256MB eviction budget?

Anti-patterns to flag:
- [FLAG] Capturing entire FileIndex or Vec<IndexedFile> (~20MB for 100k files) in a signal closure — use @weak ref + imp() access
- [FLAG] Missing file size limits on a new file-loading path
- [RECOMMEND] Unbounded concurrent spawn_blocking_then without awareness of the thread spawn guard
- [RECOMMEND] New tab-opening code that doesn't trigger eviction check
- [GOOD] Correct use of Arc<PathBuf> sharing for workspace roots (established pattern)
- [GOOD] Buffer eviction on tab switch when memory budget exceeded

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Memory impact (quantified where possible). Fix.
```

### 5. benchmark-audit

**Triggers**:
- paths: `benches/**/*.rs`, `Cargo.toml`
- content: `criterion|bench|benchmark|Bencher`

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

1. **Merge findings** — combine all [FLAG], [RECOMMEND], [CONSIDER], [GOOD] items from all subagents verbatim. Do not add new findings beyond what was reported.
2. **Deduplicate** — if two subagents flag the same line (e.g., large-file-audit and ram-budget-audit both flag a `read_to_string`), keep the more specific finding
3. **Drop excluded items** — remove any finding that falls under the "What We Do NOT Flag" list above
4. **Sort by severity** — FLAG first, then RECOMMEND, CONSIDER, GOOD

## Audit Report Format

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

## Guidance Mode

When implementing new features (not reviewing existing code), answer these questions before the implementation is complete:

1. What is the maximum input size this code path can encounter?
2. Is there a cancellation mechanism if the operation becomes stale?
3. What happens at 10x the typical input size? Is there graceful degradation?
4. Are there hard limits for extreme inputs?

## Tone

Scale advice must be grounded in numbers. Instead of "this might be slow with many files," say "at 100k indexed files, `search_items` takes ~12ms per query." Acknowledge what works well before suggesting improvements. Never recommend micro-optimizations — focus on architectural decisions that prevent user-visible degradation.
