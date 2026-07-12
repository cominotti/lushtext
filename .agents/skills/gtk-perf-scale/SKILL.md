---
name: gtk-perf-scale
description: "Explicit deep-dive checklist for LushText performance at scale: large files, directory traversal, file indexes, search, bounded concurrency, memory budgets, and benchmark coverage. Use when explicitly invoked or when assigned as the scale leaf by gtk-perf-review; do not auto-invoke alongside the umbrella performance skill. Leaf reviewers must return findings directly and must not delegate."
---

# Scale and Memory Deep Dive

Review architectural behavior as inputs grow. This is a leaf of `gtk-perf-review`; never spawn subagents from this skill.

## Load references selectively

- File open/save or eviction: [references/large-file-patterns.md](references/large-file-patterns.md)
- Search/index changes: [references/search-scaling.md](references/search-scaling.md)
- Memory/concurrency changes: [references/ram-budget.md](references/ram-budget.md)
- Benchmark changes or claimed regressions: [references/benchmark-setup.md](references/benchmark-setup.md)

Treat references as navigation, not immutable facts. Verify every constant, cap, dependency version, benchmark group, and implementation status against current code.

## Checklist

1. Identify the growing dimension: bytes, files, directory entries, open tabs, search candidates/results, pending work, snapshots, or retained GTK objects.
2. Trace bounds and cancellation from input through worker result and GTK application. Check stale-result rejection as well as worker cancellation.
3. Verify file I/O uses `services::filesystem`, performs expensive work off the GTK main thread, and preserves durable-write/data-safety contracts.
4. Check that traversal, indexing, search results, previews, histories, and queues have explicit limits or a justified streaming/backpressure design.
5. For `spawn_blocking_then`, verify current `gtk-lush-tasks` semantics in code. The dispatcher presently uses a process-wide worker cap and a main-thread FIFO awakened on slot release; do not describe it as timer polling or assume the cap bounds every application-owned buffer.
6. Evaluate peak live data, not only steady state. Include worker results waiting for main-loop consumption, snapshots, GTK buffer copies, indexes, and protected user work.
7. Check GTK models for bounded reconciliation and virtualization-friendly updates. Do not require arbitrary caps when lazy materialization or streaming provides a stronger bound.
8. Require benchmark changes when a materially changed established hot path lacks representative coverage. Compare on the same machine/build/profile; never enforce copied absolute timings.

## Finding rules

- **FLAG** unbounded growth, OOM/data-loss risk, main-thread work that scales with user input, missing freshness checks, or bypassed policy limits.
- **RECOMMEND** a concrete bound, coalescing, streaming, batching, or benchmark improvement supported by evidence.
- **CONSIDER** a measurement or alternative design with explicit tradeoffs.
- **GOOD** relevant bounds and scale-safe patterns.

Do not flag micro allocations, harmless clones, or capacity hints without a demonstrated scale effect. State assumptions and show the input that reaches the failure mode. Return `file:line`, growth dimension, complexity or memory reasoning, user impact, and fix. Include a short peak-memory assessment when applicable.
