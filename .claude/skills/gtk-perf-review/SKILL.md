---
name: gtk-perf-review
description: "Unified performance review entry point for GTK4/Libadwaita Rust code — dispatches parallel subagents for responsiveness (main-thread blocking, signal handlers, timers), Rust optimization (SIMD coverage, allocations, modern idioms, benchmarks), and scale (large files, search indexing, file trees, RAM budgets). Use this skill whenever reviewing or writing Rust code that touches UI, file I/O, async patterns, search, indexing, signal handlers, TreeListModel, or any performance-sensitive path. This is the single entry point that replaces invoking gtk-responsiveness, gtk-perf-rust-optimize, and gtk-perf-scale separately. Trigger on any .rs file change in ui/ or services/, any mention of performance, responsiveness, memory, RAM, threading, large files, slow search, SIMD, allocations, zero-copy, memchr, simdutf8, benchmarks, or 'app not responding'. Also trigger on pull request reviews involving Rust code."
---

Unified performance review for LushText. This skill is a lightweight orchestrator that dispatches three focused subagents — one for **responsiveness** (main-thread blocking), one for **scale** (data-path performance and RAM), and one for **Rust optimization** (SIMD coverage, allocations, modern idioms) — then merges their reports into a single unified audit.

## Why This Exists

LushText has three complementary performance skills:
- **gtk-responsiveness**: Ensures the GTK main thread stays free (<16ms per frame)
- **gtk-perf-scale**: Ensures data paths scale to large inputs without OOM or multi-second delays
- **gtk-perf-rust-optimize**: Ensures Rust code is maximally efficient (SIMD coverage, allocation waste, modern idioms, benchmark completeness)

They cover different concerns but often all apply to the same code change. This umbrella skill dispatches all three in parallel and produces one unified report, avoiding duplicate work and ensuring complete coverage.

## Execution Model

This skill ALWAYS dispatches exactly 3 subagents in parallel. Each subagent internally dispatches its own focused sub-subagents based on which files were changed. Do NOT attempt to review code inline — delegate everything.

### Workflow

1. **Identify changed files** — run `git diff --name-only` to get the list of changed `.rs` files. If reviewing a PR, use the PR's file list instead.

2. **Dispatch three subagents in parallel** via the Agent tool:

   **Subagent A: Responsiveness Review**
   ```
   You are performing a GTK4/Libadwaita responsiveness review for the LushText text editor.

   Read the skill file at: .claude/skills/gtk-responsiveness/SKILL.md

   Follow its Execution Model exactly:
   1. Match the changed files against subagent trigger patterns
   2. Dispatch relevant subagents (blocking-io-audit, signal-handler-audit, tree-factory-audit, debounce-timer-audit) in parallel
   3. Aggregate their findings into a single responsiveness report

   Changed files:
   {changed_files}

   Return the aggregated report with all findings tagged [FLAG], [RECOMMEND], [CONSIDER], or [GOOD].
   ```

   **Subagent B: Rust Optimization Review**
   ```
   You are performing a Rust-level optimization review for the LushText text editor.

   Read the skill file at: .claude/skills/gtk-perf-rust-optimize/SKILL.md

   Follow its Execution Model exactly:
   1. Match the changed files against subagent trigger patterns
   2. Always include allocation-audit (cross-cutting)
   3. Dispatch relevant subagents (simd-coverage-audit, allocation-audit, modern-rust-audit, benchmark-gap-audit) in parallel
   4. Aggregate their findings into a single optimization report

   Changed files:
   {changed_files}

   Return the aggregated report with all findings tagged [FLAG], [RECOMMEND], [CONSIDER], or [GOOD], including SIMD coverage and allocation impact summaries.
   ```

   **Subagent C: Scale & RAM Review**
   ```
   You are performing a GTK4/Libadwaita performance-at-scale and RAM review for the LushText text editor.

   Read the skill file at: .claude/skills/gtk-perf-scale/SKILL.md

   Follow its Execution Model exactly:
   1. Match the changed files against subagent trigger patterns
   2. Always include ram-budget-audit (cross-cutting)
   3. Dispatch relevant subagents (large-file-audit, search-index-audit, file-tree-audit, ram-budget-audit, benchmark-audit) in parallel
   4. Aggregate their findings into a single scale report

   Changed files:
   {changed_files}

   Return the aggregated report with all findings tagged [FLAG], [RECOMMEND], [CONSIDER], or [GOOD], including a Memory Impact summary.
   ```

3. **Merge reports** — when all three subagents return, produce the unified report:
   - Combine all findings from all three reports
   - **Deduplicate**: if multiple subagents flag the same issue (e.g., responsiveness flags `fs::write` on main thread, scale flags missing file size check, rust-optimize flags missing simdutf8 on the same load path), keep all perspectives but group them under one heading
   - Sort by severity: FLAG → RECOMMEND → CONSIDER → GOOD
   - Add a **Cross-Cutting Summary** section noting where responsiveness, scale, and optimization concerns overlap

## Unified Report Format

```
## Performance Review

### Overview
- **Files reviewed**: N
- **Responsiveness findings**: X flag, Y recommend, Z consider
- **Rust optimization findings**: X flag, Y recommend, Z consider
- **Scale findings**: X flag, Y recommend, Z consider
- **SIMD coverage**: Brief status
- **RAM impact**: Brief summary

### Cross-Cutting Concerns
Issues that span multiple skills (e.g., a blocking I/O call that also lacks a file size check and uses scalar UTF-8 validation). List them here with all perspectives.

### Responsiveness
#### [FLAG] Title — file:line
...
#### [RECOMMEND] Title — file:line
...

### Rust Optimization
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

For trivially small changes (e.g., fixing a typo in a comment, updating a string literal), skip the subagent dispatch entirely and note: "No performance-sensitive code changed — no review needed."

The heuristic: if no changed file is in `ui/`, `services/`, `model/`, `benches/`, or contains I/O, signal handlers, search/index code, string conversions, or error handling, the change is not performance-relevant.
