## Context

GTK Lush Phase 1 established the in-tree family workspace and governance.
Phase 2 proved the extraction model with functional `gtk-lush-signals` and
`gtk-lush-settle` crates that remain pre-publication `0.0.0` packages. Phase 3
is the next reserved roadmap phase: extract the runtime and geometry patterns
that currently sit in LushText's worker-task helper, editor viewport observer,
shrinkable content wrapper, and native minimap render-hold path.

The current source material is broad but coherent:

- `services::async_task::spawn_blocking_then` limits concurrent blocking work,
  wraps non-`Send` GTK-thread state in `glib::thread_guard::ThreadGuard`, and
  dispatches completion through `glib::idle_add_once`.
- LushText has about 62 `spawn_blocking_then` call sites. Some only need worker
  dispatch; others also carry app-owned generation checks, persistence
  ordering, path identity checks, or workflow-specific stale-result rejection.
- `editor_page/overscroll.rs` observes `GtkAdjustment` page-size and value
  changes because `GtkBox` subclasses with layout managers do not receive a
  useful `size_allocate` override. It preserves top/left rest anchors, refreshes
  dynamic EOF overscroll, and opens minimap reflow settles.
- `LushtextShrinkableBin` is a single-child GTK widget that reports zero minimum
  size, allocates its child to the available box, and clips snapshots so
  flexible content yields before persistent chrome is pushed away.
- The minimap reflow freeze captures native `GtkSourceMap` pixels with
  `snapshot_child`, shows them through a `GtkPicture`, hides the live map by
  opacity during the reflow storm, warms the live map under the cover, and
  removes the cover after settle or early user scroll.

The same constraints from the GTK Lush constitution still govern this phase:
each crate is a leaf, no family crate depends on another at runtime, GTK keeps
the main loop and widget lifecycle, Libadwaita remains authoritative for
adaptive behavior, and no crate introduces a view DSL, state/message system, or
framework.

## Goals / Non-Goals

**Goals:**

- Add functional in-tree `gtk-lush-tasks`, `gtk-lush-viewport`, and
  `gtk-lush-widgets` crates with docs, tests, examples, and governance entries.
- Migrate LushText fitting call sites to the new crates in one phase so the
  app does not carry duplicate runtime/geometry primitives.
- Preserve LushText behavior, timing contracts, visual geometry, warnings, and
  readiness semantics.
- Classify retained explicit sites with audits rather than forcing every
  domain-specific generation, idle repair, or visual workaround into a reusable
  abstraction.
- Keep Phase 4 proof-toolchain extraction out of scope while using the current
  proof lane heavily for Phase 3 verification.

**Non-Goals:**

- No crates.io functional publication, `0.1.0` release, or repository
  graduation.
- No app feature work, new user-facing UI, or behavior change beyond the
  internal migration.
- No custom executor, task runtime, animation scheduler, component framework,
  message loop, or app-owned replacement for GTK/GtkSourceView rendering.
- No runtime dependency from one GTK Lush crate to another. Composition happens
  in LushText or in examples/dev-dependencies only.
- No rewrite of the visual-geometry proof toolchain; Phase 3 consumes the
  existing Python and automation proof lanes.

## Decisions

### Decision: ship Phase 3 as one OpenSpec change

The roadmap names one follow-up for runtime geometry:
`extract-gtk-lush-runtime-geometry`. The task helper, viewport observer,
clipping bin, and render-hold overlay are separate crates, but they are tied by
the same user-visible shell and minimap invariants. Splitting them would leave
LushText with temporary duplicate scheduling and geometry layers, and the visual
proof for minimap/sidebar animations would be harder to interpret.

Alternative considered: split tasks, viewport, and widgets into three changes.
That lowers per-change size, but it creates a long-lived mixed state where the
phase boundary is not meaningful. The chosen design keeps one large proposal
and uses internal task sections, audits, tests, and reviews to make it tractable.

### Decision: keep every Phase 3 crate a runtime leaf

`gtk-lush-tasks`, `gtk-lush-viewport`, and `gtk-lush-widgets` must not depend on
each other. This is most tempting to violate between viewport/settle/widgets:
the viewport observer can open reflow work, and `RenderHoldOverlay` is normally
paired with a settle window. The reusable crates will instead expose small
callbacks or handles, while the consumer decides how to schedule follow-up
work.

Alternative considered: let `gtk-lush-widgets` depend on `gtk-lush-settle` for
`reveal_after`. That would be convenient, but it weakens single-crate adoption
and violates the family rule. The widget crate may offer synchronous hold,
warm, reveal, clear, and idempotent cleanup primitives; LushText can pair those
with `gtk-lush-settle` externally.

### Decision: separate task dispatch from domain freshness

`gtk-lush-tasks` should extract the stable worker boundary: concurrency
backpressure, panic-safe slot release, `ThreadGuard` ownership, and
GLib-main-loop completion delivery. It should also provide typed freshness or
completion-token helpers, but it must not pretend every stale-result check is
generic. LushText has call sites where freshness means tab identity, file path,
search generation, persistence ordering, undo safety generation, or current
encoding request. Those domain checks remain in the owning workflow unless the
new helper can represent them without hiding business rules.

Alternative considered: require every migrated call site to apply through a
single `Fresh<T>` type. That would overfit a reusable crate to LushText and
make app-specific correctness look crate-owned. The chosen design allows a
common typed helper while demanding an audit for every retained domain gate.

### Decision: make viewport observation adjustment-first

`gtk-lush-viewport` should treat adjustment page-size/value changes as the
primary observable contract. This encodes the hard-earned GTK4 lesson that
layout-manager widgets may not receive a useful allocation vfunc. The crate
should work for stock `Scrollable` users, surface axis-specific events, and
provide rest-state helpers that callers can query before running anchor repair.

Alternative considered: expose a widget subclass base or allocation hook.
That would be more invasive, less generally adoptable, and would preserve the
dead-vfunc trap the extraction is meant to eliminate.

### Decision: keep `ClipBin` small and builder-friendly

`ClipBin` should be the generic form of `LushtextShrinkableBin`: one child,
zero minimum, natural size delegated to the child, exact allocation of the
child into the available box, and clipped snapshots. It should remain an
ordinary GObject widget that can be used from Blueprint/GtkBuilder.

Alternative considered: fold clipping into an app-local layout manager. A
layout manager would make adoption harder and would not preserve the simple
single-widget fix for content that must yield before fixed chrome.

### Decision: make `RenderHoldOverlay` own visibility restoration, not timing

`RenderHoldOverlay` should own the dangerous part of the minimap freeze:
capturing the last rendered pixels, placing a non-interactive cover over the
live child, hiding the live child only through a paired opacity change, warming
the live child while the cover remains visible, and restoring opacity/texture
state on every exit path. It should not own the user's reflow detection,
animation schedule, or settle window.

Alternative considered: provide a high-level "hold during animation" API.
That would require assumptions about Libadwaita transitions and app readiness.
The chosen design keeps the crate honest: it prevents unbalanced invisible
children and stale covers, while the app remains responsible for when a hold is
safe and when proof must run.

### Decision: make reviewing part of the task contract

The implementation must include focused review checkpoints, not just a final
`make check`. Runtime/geometry extraction crosses data safety, GTK internals,
responsiveness, architecture, comments, and visual proof. The task list will
call for delegated reviews and retained-site audits after each migration class,
then require the final OpenSpec and repo validation ladder.

Alternative considered: leave review to general PR process. That makes the
largest risks invisible until late. The chosen design turns the review surface
into implementation tasks with explicit evidence.

## Risks / Trade-offs

- Risk: `gtk-lush-tasks` overgeneralizes domain freshness and weakens stale
  result protection. Mitigation: classify each call site as crate-owned,
  domain-retained, or intentionally explicit, and keep domain checks visible in
  the owning module.
- Risk: backpressure or idle-delivery timing changes make widget tests flaky.
  Mitigation: preserve the low-priority GLib completion contract and run the
  full widget suite with timing-sensitive async workflows.
- Risk: viewport observers miss width-only reflow or record GTK-preserved
  adjustment values as user intent. Mitigation: require page-size tests,
  rest-state exclusion during reflow, and visual geometry proof for sidebar
  transitions.
- Risk: `RenderHoldOverlay` leaves the live minimap invisible after an early
  exit. Mitigation: make opacity restoration idempotent and assert it through
  widget tests plus automation-visible state.
- Risk: a reusable render-hold abstraction accidentally replaces the native
  minimap highlight. Mitigation: require snapshot-only cover behavior, preserve
  GtkSourceMap as the live widget, and gate the change with pixel-anchor and
  animation-stream visual proof.
- Risk: Phase 3 becomes too large for one implementation pass. Mitigation:
  keep a strict internal order, require audits after each class, and do not
  proceed to visual migration until task/viewport/clip contracts are green.
- Risk: docs and specs drift from the actual family state. Mitigation: include
  README, GOVERNANCE, rules, and umbrella-vision updates as first-class tasks,
  and run the OpenSpec validation ladder before archive.

## Migration Plan

1. Preflight the GTK Lush guidance: update stale placeholder/example policy
   wording, confirm Phase 3 scope in `docs/next/gtk-lush.md`, and add a
   GOVERNANCE review-log section for the new crates.
2. Scaffold `gtk-lush-tasks`, `gtk-lush-viewport`, and `gtk-lush-widgets` as
   workspace leaf crates with README, CHANGELOG, examples, SPDX headers,
   doctests, unit tests, and policy integration.
3. Implement `gtk-lush-tasks`, migrate `services::async_task` and call sites
   in batches, and produce the retained task-freshness audit.
4. Implement `gtk-lush-viewport`, migrate `editor_page/overscroll.rs`, and
   produce the viewport/anchor retained-site audit.
5. Implement `gtk-lush-widgets::ClipBin`, migrate the window template and type
   registration from `LushtextShrinkableBin`, then delete or reduce the app
   wrapper to documented compatibility glue.
6. Implement `gtk-lush-widgets::RenderHoldOverlay`, migrate the minimap
   reflow-freeze path, and prove early reveal, warm-under-cover, and cleanup
   semantics.
7. Update LushText rules, docs, generated resources, automation docs if any
   exposed field changes, and the GTK Lush README/GOVERNANCE files.
8. Run focused tests, delegated reviews, the full repo gate set, visual
   geometry proof, OpenSpec validation, and `git diff --check`.

Rollback for implementation failures is straightforward before archive:
revert the new crates and restore the app-local helpers from Git. Partial
archive is not acceptable; the phase only completes when all three crates and
their migrations are proven.

## Open Questions

- Exact public type names for freshness helpers can be settled during
  implementation, as long as the resulting API satisfies the spec and
  examples.
- Exact visual-geometry command invocations may depend on the current host
  capability and existing scenario names. Unsupported-host results must be
  reported explicitly and cannot count as verification for minimap-sensitive
  changes.
