## Why

GTK Lush Phase 0 still has one prerequisite simplification open: LushText needs
one audited debounce/settle timer idiom before `gtk-lush-settle` can become a
public crate API. Today the app has repeated generation-counter timers across
preview, minimap, search, notes, persistence, status pulse cleanup, and
workspace refresh paths, while nearby polling, chunking, and stale async
freshness guards look similar enough to invite unsafe over-conversion.

## What Changes

- Audit every GLib timer, idle deferral, generation counter, and `SourceId`
  cancellation site in `lushtext-core` and classify it before conversion.
- Introduce a private in-tree helper shape that prototypes the future
  `gtk-lush-settle` concepts for superseding one-shot timers, debounce, and
  layout settle bursts.
- Migrate all safe generation-counter one-shot/debounce/settle call sites to
  the private helper without changing user-visible timing, readiness, focus,
  persistence, or rendering behavior.
- Record deliberate exceptions for recurring pollers, heartbeat timers,
  chunked-yield loops, stale async freshness tokens, and domain/model
  generations that should not become settle helpers yet.
- Update GTK Lush roadmap/governance guidance and local rules after proof so
  Phase 0 has an explicit `normalize-settle-timer-helpers` follow-up before
  `extract-gtk-lush-signals-and-settle`.
- No public GTK Lush crate API is introduced in this change.

## Capabilities

### New Capabilities

- `settle-timer-normalization`: Defines the audit categories, private helper
  contract, conversion boundaries, and proof requirements for Phase 0 timer and
  settle normalization.

### Modified Capabilities

- `gtk-lush-program-governance`: Adds the missing Phase 0.3 follow-up name and
  clarifies that settle timer normalization remains LushText-internal until the
  later extraction phase.

## Impact

- Affected Rust areas include `crates/lushtext-core/src/ui/**` timer,
  debounce, layout-settle, status, notes, search, preview, minimap, workspace,
  draft/session, and file-monitor workflows, plus only the service-layer
  helpers directly needed by the private timer abstraction.
- Affected documentation includes `docs/next/gtk-lush.md`,
  `.agents/rules/widget-wiring.md`, and any related UI/rules guidance touched by
  the proven helper pattern.
- Tests and proof are expected across unit/property coverage for the helper,
  focused widget tests for converted workflows, full repository checks, and
  visual-geometry proof when minimap, preview, adaptive layout, or other
  rendered/timed UI surfaces change.
- Public APIs, crate publishing, GTK Lush package layout, and family crate
  dependencies do not change.
