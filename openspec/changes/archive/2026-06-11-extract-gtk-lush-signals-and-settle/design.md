## Context

GTK Lush is past the scaffolding stage. `establish-gtk-lush-program` created
the in-tree family workspace, governance, policy checks, placeholder
`gtk-lush-signals` and `gtk-lush-settle` crates, standalone examples, MSRV
rails, and follow-up roadmap. The canonical `gtk-lush-program-governance` and
`gtk-lush-workspace` specs already require leaf crates, no control-flow
ownership, no view DSL, no state/message system, no inter-family runtime
dependencies, no Libadwaita replacement, and proof appropriate to each crate.

Phase 0 also prepared the extraction surface:

- `normalize-declarative-bindings` separated pure state-to-view projections
  from imperative workflow side effects.
- `normalize-settle-timer-helpers` audited timer-like UI sites and introduced
  the private `crate::ui::settle::{Debounce, SettleBurst, SupersedingTimer}`
  helper shape.
- `.agents/rules/widget-wiring.md` now treats that helper as app-private
  source material and explicitly defers the public crate API to this change.

The source material is therefore unusually mature: the next phase is not a
greenfield library design. It is a careful extraction of patterns already
proven in LushText, followed by a mechanical migration that keeps the app's
observable behavior, readiness predicates, and visual proof lanes stable.

## Goals / Non-Goals

**Goals:**

- Turn `gtk-lush-signals` from a `0.0.0` placeholder into a documented
  RAII helper crate for GObject signal-handler, property-binding, and
  registration lifetimes.
- Turn `gtk-lush-settle` from a `0.0.0` placeholder into a documented
  GLib-main-loop helper crate for debounce, settle-burst, and superseding
  one-shot work.
- Keep both crates independently adoptable by stock gtk-rs applications; each
  crate must work without the other and without any LushText crate.
- Migrate LushText one module family at a time, deleting replaced manual
  handler fields, disconnect blocks, generation gates, and the private settle
  module after the last consumer moves.
- Preserve all existing UX contracts, including search debounce behavior,
  latest-state-wins persistence, status-pulse cleanup, minimap readiness, and
  visual geometry.
- Update rule docs and crate docs after proof so future agents reach for the
  crates instead of rebuilding the old patterns.

**Non-Goals:**

- No Phase 5 publication. This phase can create functional in-tree APIs, but
  it does not publish `0.1.0`, graduate the family, run the second-consumer
  adoption gate, or claim external stability.
- No framework move. The crates do not own the GTK main loop, wrap widget
  lifecycle, introduce a component hierarchy, add a message system, or define
  a view DSL.
- No extraction of background task freshness, viewport observation, geometry
  widgets, proof harnesses, automation spine, or visual-geometry runners. Those
  remain reserved for later phases.
- No forced conversion of recurring pollers, chunked UI yields, idle repair
  loops, async worker freshness tokens, or domain generations unless this
  change explicitly proves a site matches the settle contract.
- No behavior cleanup bundled into migration. If a LushText call site has a
  pre-existing UX quirk, this phase may preserve and document it; changing it
  requires a separate requirement or follow-up.

## Decisions

### 1. Extract two leaf crates together, but keep their APIs independent

`gtk-lush-signals` and `gtk-lush-settle` are extracted in the same phase
because the migration blast radius is similar: both replace app-local GTK
lifecycle boilerplate and both need full widget/warning proof. They still
remain separate crates and cannot depend on each other at runtime.

Alternative considered: split into two OpenSpec changes. That would reduce
per-change size, but it would also leave one of the two placeholder crates and
rule sections in limbo after Phase 2 starts. The user explicitly wants the
entire next phase in one shot, and the governance roadmap already reserves the
combined change name.

### 2. Design from proven call sites, not from an abstract GTK wrapper

The public APIs should start from the concrete LushText shapes:

- signal handlers stored as `RefCell<Option<glib::SignalHandlerId>>` fields
  and disconnected from either the emitting object, `gio::Settings`,
  `libadwaita::StyleManager`, buffers, list rows, or transient row data;
- `glib::Binding` values currently stored and explicitly unbound on recycle or
  teardown;
- private settle helpers whose deterministic generation logic is already unit
  tested and whose scheduling uses weak GTK targets plus GLib main-loop
  timers.

The crates may generalize names and ergonomics, but the first functional API
must stay close enough that LushText migration proves it directly. This keeps
the afternoon-adoption test plausible later: consumers get small ownership
helpers, not a new app architecture.

Alternative considered: design an idealized macro-heavy API first. Rejected
because additive macros may be useful later, but a macro-first design would be
harder to audit against the constitution and easier to accidentally turn into
a framework or DSL.

### 3. Make `gtk-lush-signals` ownership explicit and boring

The core type should be a small bag/container that owns registrations and
clears them exactly once. The likely surface is:

- a `SignalBag` for `(WeakRef<Object>, SignalHandlerId)` style registrations;
- binding storage that unbinds `glib::Binding` values on clear/drop;
- typed helper methods for common gtk-rs call shapes, but no replacement for
  ordinary `connect_*` APIs;
- clear/drop idempotence, with already-finalized sources skipped quietly;
- optional per-owner bags so modules can clear one family of handlers without
  disturbing another.

The crate should avoid requiring consumers to subclass widgets. A plain
`gtk::Label` or `gtk::Button` example must be enough to show adoption.

Alternative considered: a trait extension that shadows all `connect_*`
methods. Rejected for the first functional API because it would be too broad,
would duplicate gtk-rs naming, and would make it harder to reason about which
registrations are actually owned.

### 4. Make `gtk-lush-settle` split pure state from GLib scheduling

The private helper already separates generation decisions from scheduled GTK
callbacks enough to unit test stale-token rejection and pending-state
transitions. The public crate should make that split deliberate:

- pure generation or state-machine pieces that can be unit-tested and
  property-tested without GTK;
- GLib-main-context adapters that schedule callbacks with weak targets and
  no-op when stale or destroyed;
- `Debounce` for trailing latest-generation work;
- `SettleBurst` for quiet-window repairs with a queryable pending state;
- `SupersedingTimer` for one delayed latest-generation action.

The public API should preserve GTK's control flow. Scheduling goes through the
GLib main loop; the crate does not create its own runtime, executor, message
queue, or component lifecycle.

Alternative considered: use cancellable `SourceId` bookkeeping as the primary
model. Rejected because Phase 0 deliberately normalized around generation
tokens: stale callbacks no-op even if the underlying source fires, and call
sites no longer need fragile cancellation ownership.

### 5. Migrate LushText in proof-friendly slices

The apply phase should migrate and verify in this order:

1. public crate APIs, docs, doctests, standalone examples, and pure tests;
2. low-risk LushText signal/binding sites with obvious teardown;
3. higher-risk signal sites that swap buffers, rows, or long-lived settings;
4. low-risk settle call sites such as status pulses and input debounces;
5. persistence and readiness-sensitive settle call sites;
6. minimap/preview/render-sensitive settle paths;
7. rule/doc rewrites and deletion of obsolete private helpers.

This order lets failures point to one ownership family at a time. It also
keeps visual-sensitive migrations late enough that the crate primitives are
already stable before the pixel lanes judge them.

Alternative considered: migrate all handlers first, then all timers. Rejected
because the safest sequencing is not strictly by crate; it is by behavioral
risk and verification cost.

### 6. Treat retained explicit timers and handlers as audited exceptions

Not every remaining `SignalHandlerId`, `Binding`, timeout, idle callback, or
generation counter is automatically a violation. The apply work must produce
an audit of retained explicit sites and classify them, for example:

- row data or factory recycle hooks that GTK owns in a shape the bag cannot
  safely own yet;
- recurring pollers and heartbeats;
- chunked UI yields and idle repair loops;
- async worker freshness tokens reserved for `gtk-lush-tasks`;
- domain/model generations that are not UI settle scheduling.

This audit prevents the rule rewrite from becoming dishonest. The new rules
should say "use the crate for these ownership classes" and "keep these classes
explicit until a later phase proves a better primitive."

### 7. Keep documentation rewrites proof-driven

The README of each crate should become the rewritten rule section: what bug
class the crate prevents, what it deliberately does not do, and how a stock
gtk-rs app adopts it. `.agents/rules/rust.md`,
`.agents/rules/widget-wiring.md`, and `docs/next/gtk-lush.md` should be
updated after migration proof, not before, so they describe the API that
actually survived contact with LushText.

Alternative considered: rewrite rules up front to force implementation.
Rejected because Phase 0 learned from real call sites; Phase 2 should keep
that habit.

## Risks / Trade-offs

- API over-generalizes from LushText → Keep helpers small, require standalone
  gtk-rs examples, and reject any surface that needs app restructuring.
- Signal bags accidentally keep widgets alive → Store weak sources for
  long-lived emitters, document closure-capture rules, and verify finalization
  or no-post-drop callback behavior in tests.
- Drop-time disconnect causes GLib warnings for finalized sources → Use weak
  source upgrade before disconnect/unbind where applicable and test
  already-finalized source behavior.
- Binding ownership differs from signal ownership → Give bindings their own
  explicit storage/clear path; do not pretend every registration is a signal.
- Settle pending state clears too early → Preserve the Phase 0 `SettleHandle`
  style: pending remains true through the repair callback and clears in the
  same dispatch only after repair completes.
- Debounce migration loses immediate-empty behavior → Each search-like site
  gets a state-extreme test for empty input, representative input, and rapid
  input changes.
- Persistence debounce regresses latest-state-wins semantics → Keep existing
  ordered save generations where they are domain persistence contracts and use
  settle tokens only for timer coalescing.
- Visual-sensitive minimap/preview migrations drift pixels → Run widget tests
  plus visual-geometry scenarios that cover sidebar animation, minimap anchors,
  and animation-frame sampling before archive.
- The phase is large → Keep task slices small and gate each slice; if a slice
  exposes a design flaw, update the spec/design in the same change before
  continuing.
- Functional in-tree APIs look publishable too early → Keep version posture
  and docs explicit: Phase 2 is not `0.1.0`, not graduated, and not externally
  stable until Phase 5 gates pass.

## Migration Plan

1. Implement public crate APIs with crate-level docs, READMEs, doctests,
   standalone examples, unit/property tests, lint compliance, and no runtime
   family dependencies.
2. Add dependency wiring from LushText to the two family crates through the
   workspace path setup already established by `gtk-lush-workspace`.
3. Migrate signal/binding ownership in narrow module batches. After each
   batch, run focused tests or compile/lint checks that exercise teardown.
4. Migrate settle/timer ownership in narrow risk-ordered batches. Preserve
   existing delay windows and pending/readiness behavior unless a spec says
   otherwise.
5. Delete replaced private code and manual fields only after their last
   consumer is gone.
6. Produce retained-site audits for signal/binding and timer-like patterns.
7. Rewrite rule docs, crate READMEs, CHANGELOGs, and `docs/next/gtk-lush.md`
   to reflect the proven API and remaining exceptions.
8. Run the phase gate set: family tests/docs/examples, policy checks, full
   LushText checks, widget suite, warning gates, automation docs/client checks
   where readiness fields are touched, and visual-geometry proof for rendered
   geometry changes.

Rollback is ordinary git rollback before merge. During implementation, each
module batch should remain small enough to revert independently if its proof
fails without undoing unrelated successful batches.

## Open Questions

- Exact public type names and method names should be finalized during
  implementation against compiler ergonomics and doctest readability, but the
  normative behavior is fixed by the specs.
- Whether signal and binding ownership live in one `SignalBag` type or a
  `SignalBag` plus `BindingBag` split can be decided during API drafting. The
  requirement is explicit RAII ownership and idempotent clear/drop, not a
  specific type split.
- Whether `SettleBurst` exposes `schedule_follow_up` in the first public API
  depends on the minimap/preview migration. If it is required, it must preserve
  pending semantics and be documented as same-generation follow-up work, not a
  general task scheduler.
