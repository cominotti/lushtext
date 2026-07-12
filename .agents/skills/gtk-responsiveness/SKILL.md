---
name: gtk-responsiveness
description: Explicit deep-dive checklist for GTK4/Libadwaita responsiveness in LushText, covering main-thread work, bounded worker dispatch, async freshness, signals, timers, list factories, and widget lifetime. Use when explicitly invoked or when assigned as the responsiveness leaf by gtk-perf-review; do not auto-invoke alongside the umbrella performance skill. Leaf reviewers must return findings directly and must not delegate.
---

# GTK Responsiveness Deep Dive

Keep the GLib main loop available for input, layout, drawing, actions, and completion callbacks. This is a leaf of `gtk-perf-review`; never spawn subagents from this skill.

## Load references selectively

Read [references/async-patterns.md](references/async-patterns.md) when the scope contains file I/O, `spawn_blocking_then`, cancellation/freshness, timers, or debounce. For GTK measurement, factory lifecycle, focus, parenting, or Adwaita container contracts, use `gtk4-libadwaita-internals` as the toolkit authority.

## Checklist

1. Inspect the supplied diff and trace each user-triggered path to its expensive operations.
2. Flag blocking filesystem, traversal, serialization, parsing, or CPU-heavy work on the GTK thread unless current evidence proves the operation is bounded and negligible. Do not bless I/O solely because a file is expected to be small.
3. Keep production I/O behind `services::filesystem`; responsiveness helpers must not bypass the filesystem boundary. Prefer `gtk_lush_tasks::spawn_blocking_then` or its weak-target variant for blocking work that returns to GTK. Verify the current task implementation before stating queue/cap semantics.
4. Check three independent async contracts:
   - lifetime: the target still exists;
   - freshness: the result still belongs to the current tab/path/query/generation;
   - ordering/data safety: an older completion cannot overwrite newer accepted state.
5. Treat persistence and cleanup results as observable state. Do not recommend detached `std::thread::spawn` for writes merely because the user did not click Save; failures, ordering, shutdown, and retry behavior still need an owner.
6. In signal handlers, reject blocking work and strong-reference cycles. Use local signal variants when GTK objects are captured, and disconnect handlers that can accumulate across rebinding or rebuilds.
7. In list factories, create widget structure in setup, project state in bind, and clear per-item state/bindings in unbind. Verify GTK contracts before alleging a lifecycle bug.
8. For rapid input and superseding one-shots, prefer the fitting `gtk_lush_settle` primitive. Require explicit cancellation/lifetime handling for raw `SourceId` timers. Timer intervals are workflow policy, not universal constants.
9. Keep main-loop completion callbacks bounded too: background work is not sufficient if the callback installs or transforms an unbounded result synchronously.
10. Validate runtime-only geometry/allocation fixes with live GTK logs when the symptom depends on measurement or animation.

## Finding rules

- **FLAG** a demonstrated freeze, blocking I/O, unbounded main-loop callback, stale-state application, persistence ordering hazard, or lifetime leak.
- **RECOMMEND** a clear responsiveness/lifecycle improvement with a concrete trigger.
- **CONSIDER** a measurement or helper adoption whose benefit is plausible but unproven.
- **GOOD** relevant correct async, signal, timer, or factory behavior.

Do not invent durations or desktop “not responding” thresholds. State whether timing is measured, derived from a bound, or unknown. Do not flag trivial clones or allocations. Return `file:line`, trigger, GTK-thread work, lifecycle/freshness analysis, user impact, and fix.
