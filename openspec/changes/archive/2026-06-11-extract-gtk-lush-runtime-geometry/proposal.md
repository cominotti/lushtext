## Why

GTK Lush Phase 3 turns the next cluster of hard-won LushText GTK patterns into
small, independently adoptable crates: background task delivery, viewport
geometry observation, and geometry-protecting widgets. This phase is needed now
because Phase 2 has proven the family-crate governance model, and the remaining
runtime/geometry source material is tightly coupled enough that splitting it
would leave LushText between duplicate implementations.

## What Changes

- Add `gtk-lush-tasks`, a leaf crate for bounded worker-thread execution,
  GLib-main-loop completion delivery, panic-safe slot release, and typed
  freshness/completion helpers that make stale worker-result application
  explicit.
- Add `gtk-lush-viewport`, a leaf crate for observing viewport geometry through
  scroll adjustments/page-size changes, including rest-state tracking and
  anchor-preserving repair hooks for layout-manager widgets whose
  `size_allocate` vfuncs do not fire.
- Add `gtk-lush-widgets`, a leaf crate for reusable geometry widgets:
  `ClipBin` from `LushtextShrinkableBin`, and `RenderHoldOverlay` from the
  native minimap reflow-freeze pattern.
- Migrate LushText to consume the new crates in one phase:
  `services::async_task` callers, `editor_page/overscroll.rs`,
  `ui/shrinkable_bin`, and minimap reflow freeze behavior.
- Preserve all user-visible behavior. This proposal intentionally does not add
  new product UI, new app workflow semantics, publication to crates.io, a view
  DSL, a message/update loop, a custom executor, or inter-crate GTK Lush runtime
  dependencies.
- Require focused audits, widget tests, visual-geometry evidence, and delegated
  reviews for task freshness, viewport anchoring, clipping, and render-hold
  behavior before the change can archive.
- Clean up GTK Lush program drift discovered during exploration, including stale
  placeholder README language and overly specific example-file requirements.

## Capabilities

### New Capabilities

- `gtk-lush-tasks`: background task execution and stale-safe completion helpers
  for stock gtk-rs applications, plus LushText migration requirements.
- `gtk-lush-viewport`: adjustment/page-size based viewport observation,
  rest-state tracking, and anchor repair helpers for stock gtk-rs scrollable
  widgets, plus LushText overscroll migration requirements.
- `gtk-lush-widgets`: reusable GTK geometry widgets, including `ClipBin` and
  `RenderHoldOverlay`, plus LushText shrinkable-bin and minimap migration
  requirements.

### Modified Capabilities

- `gtk-lush-program-governance`: clarify Phase 3 conformance, delegated review
  expectations, and single-crate example policy without changing the
  anti-framework constitution.
- `gtk-lush-workspace`: update family scaffolding requirements for functional
  pre-publication crates and replace stale placeholder/example-name wording.
- `main-thread-responsiveness`: require fitting background GTK workflows to use
  `gtk-lush-tasks` while retaining app-owned snapshot, durability, and domain
  generation semantics where the reusable crate cannot own them.
- `adaptive-editor-geometry`: require the migrated clipping and render-hold
  abstractions to preserve existing persistent-chrome, anchor, warning-clean,
  and toolkit-rendered-effect contracts.
- `editor-minimap`: require the migrated render-hold behavior to preserve the
  native `GtkSourceMap` viewport highlight, semantic marker layering, early
  reveal behavior, and animation-frame pixel-anchor proof.

## Impact

- Affected crates and workspace files: root Cargo workspace metadata,
  `crates/gtk-lush/`, cargo-hakari output, dependency policy configuration,
  nextest/MSRV/semver/public-API lanes, and GTK Lush governance/docs.
- Affected LushText code: `crates/lushtext-core/src/services/async_task.rs`
  and its callers, `ui/editor_page/overscroll.rs`,
  `ui/shrinkable_bin/`, `ui/editor_page/minimap.rs`, generated resources that
  reference `LushtextShrinkableBin`, and related tests.
- Affected proof surface: family crate unit tests, doctests, examples,
  property tests, widget tests, visual-geometry pixel-anchor and
  animation-stream scenarios, GTK/GLib warning gates, API/semver checks,
  policy checks, and OpenSpec validation.
- No breaking external API changes are expected because GTK Lush is still
  pre-publication `0.0.0`; LushText behavior must remain compatible with the
  existing app specs.
