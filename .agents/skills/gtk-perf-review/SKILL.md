---
name: gtk-perf-review
description: "Unified performance review entry point for GTK4/Libadwaita Rust code — dispatches parallel subagents for responsiveness (main-thread blocking, signal handlers, timers), Rust code quality (SIMD hot paths, modern idioms, benchmarks), and scale (large files, search indexing, file trees, RAM budgets). Use this skill whenever reviewing or writing Rust code that touches UI, file I/O, async patterns, search, indexing, signal handlers, TreeListModel, GTK Lush responsiveness/proof primitives, or any performance-sensitive path. This is the single entry point that replaces invoking gtk-responsiveness, gtk-perf-rust-optimize, and gtk-perf-scale separately. Trigger on any .rs file change in ui/ or services/, any mention of performance, responsiveness, memory, RAM, threading, large files, slow search, SIMD, benchmarks, gtk-lush-tasks, gtk-lush-settle, or 'app not responding'. Also trigger on pull request reviews involving Rust code."
---

Unified performance review for LushText. This skill is a lightweight orchestrator that dispatches three focused subagents — one for **responsiveness** (main-thread blocking), one for **scale** (data-path performance and RAM), and one for **Rust code quality** (SIMD hot paths, modern idioms, benchmark coverage) — then merges their reports into a single unified audit.

## Boundary with gtk4-libadwaita-internals

Before treating a bug as purely "performance", check whether the report or diff contains a GTK or Adwaita contract question:

- `Trying to measure ...`, allocation or snapshot warnings, or `GtkPaned` / `GtkRevealer` layout math
- `GtkSignalListItemFactory` lifecycle, `GtkListView` row reuse, or `GtkTreeListModel` semantics
- builder-template child types, parentage, disposal, focus, CSS-node, or adaptive container rules

If so, read `gtk4-libadwaita-internals` first so the performance review does not misclassify a contract violation as a throughput bug.

## GTK Lush Performance Posture

The responsiveness subreview should prefer existing GTK Lush primitives where
they fit: `gtk-lush-tasks` for bounded worker-to-main dispatch,
`gtk-lush-settle` for UI debounce/settle timers, `gtk-lush-viewport` for
adjustment observation, and `gtk-lush-widgets` for clipping or render-hold
geometry. Do not turn a performance review into a new GTK Lush API proposal;
use `gtk-lush-stewardship` when the existing platform contract itself would
need to change.

## Philosophy: Readability First

Performance matters, but **code readability is the top priority**. Every recommendation from this review must pass the readability gate:

> **Would a new contributor understand this code on first read?**

If an optimization makes the code harder to understand — introduces unfamiliar abstractions, adds indirection, or trades clarity for marginal throughput — it is a net negative and should not be recommended. The best performance patterns are the ones that are also the clearest to read.

This means:
- **Only flag things a real user would notice.** A 50ms UI freeze is worth fixing. Saving 4KB on a heap extraction is not.
- **Prefer simple, idiomatic Rust over clever micro-optimizations.** `format!()` is fine. `.to_string()` is fine. `.clone()` is fine when the alternative adds complexity.
- **Sophisticated patterns need clear abstractions.** If SIMD code is recommended, the wrapper must be self-documenting. If an async pattern is complex, the helper function must have a clear name and purpose.
- **File I/O goes through `services::filesystem`.** Flag raw filesystem access outside the approved backend/fixture modules, prefer cheap status helpers for presence/kind probes, and judge whether the boundary call belongs on a background thread or needs scale guards.

### What We Do NOT Flag

These are explicitly out of scope — do not report findings about:

- **Trivial allocation differences**: `.to_string_lossy().to_string()` vs `.into_owned()`, `format!()` vs `push_str()`, intermediate collections under 1000 items
- **Speculative `Cow<str>` opportunities**: Cow adds complexity; only worth it when profiling proves a hot path
- **`Vec::with_capacity()` for small collections**: Under ~1000 items, the doubling strategy is fine
- **`retain()` without `shrink_to_fit()`**: The OS reclaims pages; explicit shrinking is rarely worth the code noise
- **Clone avoidance in non-hot paths**: A few extra clones in setup code, signal wiring, or error paths are perfectly fine
- **Drop ordering micro-optimization**: Scoping intermediates to free memory 1ms earlier is not worth the nested blocks

The threshold is simple: **if you need a benchmark to prove the difference exists, it's a microoptimization.**

## Why This Exists

LushText has three complementary performance skills:
- **gtk-responsiveness**: Ensures the GTK main thread stays free (<16ms per frame)
- **gtk-perf-scale**: Ensures data paths scale to large inputs without OOM or multi-second delays
- **gtk-perf-rust-optimize**: Ensures Rust code is idiomatic, correct, and uses SIMD where the codebase already has established patterns

They cover different concerns but often all apply to the same code change. This umbrella skill dispatches all three in parallel and produces one unified report, avoiding duplicate work and ensuring complete coverage.

When a change touches `GtkPaned` / `GtkRevealer` animation around a heavy sidebar or tree, treat live geometry warnings as part of the responsiveness review. A fix that feels smooth in widget tests but still logs `Trying to measure GtkBox ...` under `make run` is not complete. Use `gtk4-libadwaita-internals` to establish the measurement contract before judging the performance fix.

Also treat allocation-frame churn as a first-class responsiveness issue even when there is no blocking I/O. In LushText, the locally installed Flatpak showed visibly low-refresh sidebar/file-info animations because each animation frame re-synced split-view widths, rewrote GSettings through split-view notify handlers, and reparsed/reinstalled an adaptive `AdwBreakpoint` condition. A valid performance fix keeps `size_allocate()` to cheap width/threshold comparisons and runtime clamps, caches derived breakpoint thresholds, and moves persistence to explicit user intent or animation completion.

Snapshot-based pane optimizations need two separate checks:
- geometry correctness: the snapshot surface must preserve the live child's minimum width, or GTK may warn on the opposite child instead
- host correctness: if a `GtkStack` or similar wrapper is the actual `GtkPaned` child, it needs the same legal width floor as the live pane it wraps; a descendant `width-request` alone may still leave a one-pixel warning
- interaction-path cost: the snapshot must not be generated synchronously on the click path if that capture itself causes hide-time stutter
- host behavior: if a `GtkStack` or similar wrapper is only being used as a stable swap host, disable its own transitions or you may accidentally introduce a second animation and extra allocation work
- visual validity: a cached paintable can still be visually empty or black; confirm the frozen surface is actually valid, not just present
- freeze strategy: if a one-shot capture is visually invalid, a persistent `GtkWidgetPaintable::current_image()` can be the correct frozen surface even though it is not the cheapest-looking abstraction at first glance
- pane scope: do not freeze panes symmetrically by default. If the sidebar is the expensive subtree and the content pane already animates smoothly, freezing the content pane can add distortion or end-of-animation artifacts with no performance win

## Execution Model

This skill ALWAYS dispatches exactly 3 subagents in parallel. Each subagent internally dispatches its own focused sub-subagents based on which files were changed. Do NOT attempt to review code inline — delegate everything.

### Workflow

1. **Identify changed files** — run `git diff --name-only` to get the list of changed `.rs` files. If reviewing a PR, use the PR's file list instead.

2. **Dispatch three subagents in parallel** via the Agent tool. In each prompt below, replace `{changed_files}` with the actual file list from step 1 — paste the `git diff --name-only` output verbatim. Do not leave `{changed_files}` as a literal string.

   **Subagent A: Responsiveness Review**
   ```
   You are performing a GTK4/Libadwaita responsiveness review for the LushText text editor.

   Read the skill file at: .agents/skills/gtk-responsiveness/SKILL.md

   Follow its Execution Model exactly:
   1. Match the changed files against subagent trigger patterns
   2. Dispatch relevant subagents (blocking-io-audit, signal-handler-audit, tree-factory-audit, debounce-timer-audit) in parallel
   3. Aggregate their findings into a single responsiveness report

   Changed files:
   {changed_files}

   Return the aggregated report with all findings tagged [FLAG], [RECOMMEND], [CONSIDER], or [GOOD].
   ```

   **Subagent B: Rust Code Quality Review**
   ```
   You are performing a Rust code quality review for the LushText text editor, focusing on idiomatic patterns, latest-stable Rust readability improvements, correctness, and SIMD coverage on established hot paths.

   Read the skill file at: .agents/skills/gtk-perf-rust-optimize/SKILL.md

   Follow its Execution Model exactly:
   1. Match the changed files against subagent trigger patterns
   2. Dispatch relevant subagents (simd-coverage-audit, modern-rust-audit) in parallel
   3. Aggregate their findings into a single code quality report

   IMPORTANT: Do NOT flag microoptimizations. Only flag issues where: (a) a user would notice the difference, (b) the fix improves both correctness AND readability, or (c) an established SIMD pattern is missing on a proven hot path.

   Changed files:
   {changed_files}

   Return the aggregated report with all findings tagged [FLAG], [RECOMMEND], [CONSIDER], or [GOOD].
   ```

   **Subagent C: Scale & RAM Review**
   ```
   You are performing a GTK4/Libadwaita performance-at-scale and RAM review for the LushText text editor.

   Read the skill file at: .agents/skills/gtk-perf-scale/SKILL.md

   Follow its Execution Model exactly:
   1. Match the changed files against subagent trigger patterns
   2. Always include ram-budget-audit (cross-cutting)
   3. Dispatch relevant subagents (large-file-audit, search-index-audit, file-tree-audit, ram-budget-audit, benchmark-audit) in parallel
   4. Aggregate their findings into a single scale report

   IMPORTANT: Focus on architectural memory concerns (buffer budgets, index caps, concurrent load limits), not micro allocation patterns.

   Changed files:
   {changed_files}

   Return the aggregated report with all findings tagged [FLAG], [RECOMMEND], [CONSIDER], or [GOOD], including a Memory Impact summary.
   ```

3. **Merge reports** — when all three subagents return, produce the unified report:
   - Combine all findings from all three reports verbatim — do not add new findings beyond what the subagents reported
   - **Deduplicate**: if multiple subagents flag the same issue (e.g., responsiveness flags a filesystem-boundary write on the main thread, scale flags a missing file size check), keep all perspectives but group them under one heading
   - **Drop excluded items**: remove any finding that falls under the "What We Do NOT Flag" list above
   - Sort by severity: FLAG → RECOMMEND → CONSIDER → GOOD
   - If findings from different subagents affect the same file and line, group them under a **Cross-Cutting Concerns** heading

## Unified Report Format

```
## Performance Review

### Overview
- **Files reviewed**: N
- **Responsiveness findings**: X flag, Y recommend, Z consider
- **Code quality findings**: X flag, Y recommend, Z consider
- **Scale findings**: X flag, Y recommend, Z consider

### Cross-Cutting Concerns
Issues that span multiple skills (e.g., a blocking filesystem-boundary call that also lacks a file size check). List them here with all perspectives.

### Responsiveness
#### [FLAG] Title — file:line
...
#### [RECOMMEND] Title — file:line
...

### Code Quality
#### [FLAG] Title — file:line
...
#### [RECOMMEND] Title — file:line
...

### Scale & Memory
#### [FLAG] Title — file:line
...
#### [RECOMMEND] Title — file:line
...

### Good Patterns
Patterns from all three reviews that are correct and should be preserved.
```

## When NOT to Dispatch

For trivially small changes, skip the subagent dispatch entirely and note: "No performance-sensitive code changed — no review needed."

The rule: skip if no changed `.rs` file has a path under `src/ui/`, `src/services/`, `src/model/`, or `benches/`. Do not inspect file content to decide whether to skip — use paths only. Non-Rust file changes (docs, config, XML, CSS) are never performance-relevant.
