# GTK Lush — Umbrella Vision

Status: current posture is stable in-tree internal platform. Phases 1 through
4 are complete in-tree functional API/tooling phases, and Phase 5a adoption
validation is complete and archived (`validate-gtk-lush-adoption-surface`).
GTK Lush is now maintained as LushText-first workspace infrastructure.
Publication, repository graduation, LushText migration to published
dependencies, and broad upstreaming are dormant future tracks that require a
dedicated maintainer-approved OpenSpec change with refreshed evidence.

GTK Lush is the extraction of LushText's hardened GTK4/Libadwaita patterns into
a small family of independently adoptable Rust crates, plus a reusable headless
proof toolchain, governed so that it can never quietly become a framework.

This document remains the umbrella narrative for the whole program:
principles, crate family, historical phase plan, engineering bar, governance,
risks, and success metrics. The canonical OpenSpec specs now include
`gtk-lush-internal-platform`, which supersedes any older roadmap wording that
looks like automatic publication or upstreaming work. If the narrative and
OpenSpec specs ever disagree, the OpenSpec specs win and this document must be
updated in the same change.

## 1. Mission

Turn the lessons LushText paid for in debugging hours — currently encoded as
prose in `.agents/rules/*.md` and `.agents/skills/*` — into types, crates, and
tools that:

1. make LushText itself smaller and safer, and
2. can be adopted by any stock `gtk4-rs` + `libadwaita-rs` application, one
   crate at a time, in an afternoon, without restructuring anything else.

The second clause is the product. The program is "rules become types."

## 2. The discriminating test and the anti-framework constitution

Every API, crate, and follow-up in this program is judged against one test:

> Can a stock gtk-rs application adopt exactly one piece in an afternoon,
> without restructuring anything else?

If the answer is ever "no", the piece is redesigned or rejected. From this test
follow the non-negotiable principles (the "constitution"). These are enforced
as review gates, not aspirations:

- **No control-flow ownership.** GTK owns the main loop, widget lifecycle, and
  rendering. GTK Lush crates only ever react to GTK; they never wrap, replace,
  or schedule around it.
- **No view DSL.** Blueprint and GtkBuilder XML remain the only declarative UI
  layers. GTK Lush macros may generate boilerplate that an author would
  otherwise write by hand (derive-style, additive), but must never introduce a
  custom syntax that replaces ordinary widget code.
- **No state or message system.** No model/update loops, no component
  hierarchy, no message passing. Relm4 exists for people who want that trade;
  GTK Lush's differentiation is precisely that it does not ask for it.
- **No inter-crate dependencies inside the family.** Every crate is a leaf.
  A consumer can take one and ignore the rest forever.
- **Adwaita stays authoritative for adaptive behavior.** Nothing in the family
  re-implements split views, breakpoints, sheets, or animations that
  Libadwaita provides.
- **Pixels and contracts over claims.** Each crate ships the same class of
  proof LushText demands of itself: headless widget tests, doctests, and —
  where rendering is involved — visual-geometry evidence.

Any proposal that needs an exception to these principles is, by definition, a
new program, not part of GTK Lush.

## 3. Where the source material lives

The extraction sources are already specified — by the repository's own rules
and the code they fence:

| Pattern | Today (LushText) | Rule/skill that documents it |
| --- | --- | --- |
| Signal/binding handler lifetimes | `gtk-lush-signals` owns fitting signal, binding, and row-registration lifetimes; retained explicit sites are documented in the Phase 2 audit | `rust.md` (GObject subclassing), `widget-wiring.md` |
| Debounce / settle helpers | `gtk-lush-settle` replaces the former private `crates/lushtext-core/src/ui/settle.rs` helper for fitting debounce, superseding-timer, and readiness-settle call sites | `widget-wiring.md` (Superseding Timers And Settle Helpers), `ui.md` (minimap reflow burst) |
| Main-thread-safe background tasks | `gtk-lush-tasks` owns bounded worker dispatch and freshness tokens; per-site domain freshness remains explicit | `rust.md` (Background I/O, snapshot boundaries) |
| Viewport observation without dead vfuncs | `gtk-lush-viewport` owns adjustment observers and lower-edge rest state; LushText owns repairs | `widget-wiring.md` (size_allocate vs layout managers) |
| Zero-min clipping bin | `gtk-lush-widgets::ClipBin` registered as `GtkLushClipBin` | `ui.md` (Split-View Rules) |
| Render-hold overlay (freeze last-good pixels across a layout storm) | `gtk-lush-widgets::RenderHoldOverlay` owns capture/cover/opacity mechanics; minimap owns timing | `ui.md` (minimap guardrails), `widget-wiring.md` |
| Headless widget harness | `crates/lushtext/tests/widget.rs` + `scripts/run-widget-tests.sh` + `tests/widget/common.rs` wait helpers | `gtk-testing` skill |
| Readiness/snapshot automation spine | `ui/automation.rs`, `model/automation.rs`, D-Bus Automation1 | `docs/automation.md`, `widget-wiring.md` |
| Visual-geometry proof lane | `cargo gtk-proof run`, `scripts/visual-geometry-smoke.py` as Python oracle, `test-visual-geometry.py`, `visual_geometry_png.py`, scenario JSONs, `check-visual-proof-policy.py` | `build.md`, `ui.md`, `gtk-agentic-debugging` skill |

The README of each extracted crate is the corresponding rule section, rewritten
around the type that now enforces it. The documentation is already paid for.

## 4. The crate family

Workspace layout: `crates/gtk-lush/<member>` with package names
`gtk-lush-<member>`. All leaves; no member depends on another member.

### 4.1 `gtk-lush-signals`

RAII lifetime management for GObject signal handlers, property bindings, and
controller registrations.

- **API:** `SignalBag`, `BindingBag`, and `RegistrationBag` value types stored
  on an `imp` struct or row lifecycle owner. Callers use ordinary gtk-rs
  `connect_*` / `bind_property` APIs, then record the returned handler or
  binding with the appropriate bag. Signal sources are held weakly, so bags can
  outlive already-finalized widgets or shared settings objects.
- **Rust features:** RAII/`Drop`, ownership to encode "handler must not
  outlive widget", zero unsafe.
- **Replaces in LushText:** hand-tracked handler-id fields, row-data
  `SignalHandlerId` storage, and explicit row binding unbind paths where the
  lifecycle fits the crate contract.
- **Acceptance:** unit tests, doctests, and a standalone gtk-rs example prove
  disconnect-on-clear/drop, dead-source tolerance, binding unbind, rebinding,
  and single-crate adoption.

### 4.2 `gtk-lush-settle`

Generation-counter scheduling: debounce, settle-bursts, and superseding timers
as one audited primitive.

- **API:** `Debounce` schedules trailing latest-generation work on GLib's main
  loop with weak target cancellation; `SettleBurst` exposes readiness-visible
  `pending()` state and generation-bound repair handles; `SupersedingTimer`
  owns delayed one-shot cleanup/reveal work where each arm replaces the
  previous arm.
- **Rust features:** generic over the captured target via `glib::WeakRef`,
  `Cell`-based state, no interior panics; pure decision logic split out for
  unit and property tests.
- **Replaces in LushText:** the deleted private `crate::ui::settle` prototype
  and its converted minimap reflow settle, status-bar message dismiss, draft
  autosave, workspace persistence, file-monitor, preview, search, and indexing
  scheduling call sites.
- **Acceptance:** unit tests, property tests, doctests, and migrated LushText
  tests prove generation advancement, stale-token rejection, invalidation,
  wrapping, weak-target cancellation, pending-state repair, and re-arm
  semantics.

### 4.3 `gtk-lush-tasks`

Main-thread-safe background work with staleness-proof completion.

- **API:** `spawn_blocking_then(state, work, then)`,
  `spawn_blocking_then_weak`, and `FreshnessToken`/`Fresh`/`Stale` helpers.
  The crate owns bounded dispatch and main-thread completion; callers keep the
  actual generation, path, tab, or search identity policy beside the workflow.
- **Rust features:** typestate, `glib::thread_guard::ThreadGuard` wrapping,
  `Send` bounds that document the thread contract in signatures.
- **Replaces in LushText:** the former `services::async_task`; per-call-site
  generation/weak-identity checks remain explicit in their workflow modules.
- **Acceptance:** all async-backed widget tests pass unmodified in timing;
  the low-priority idle completion contract (required by `wait_until`) is
  preserved and documented as a stability guarantee.

### 4.4 `gtk-lush-viewport`

Allocation-derived geometry observation for widgets whose `size_allocate`
vfunc never fires (layout-manager classes — `GtkBox`, and anything else GTK4
routes through a layout manager).

- **API:** `ViewportObserver::for_scrollable(&text_view)` (or any
  `Scrollable`) emitting `ViewportBoundsChange` and `ViewportValueChange`
  structs from adjustment page-size and value deltas, plus `RestState` and
  `RestPause` tracking so GTK-preserved offsets during reflow windows cannot
  masquerade as user intent.
- **Rust features:** public value objects, callback registration that composes
  with drop-owned signal cleanup without depending on `gtk-lush-signals`.
- **Replaces in LushText:** `editor_page/overscroll.rs::setup_allocation_reflow_observers`
  and friends.
- **Acceptance:** the documented dead-vfunc trap has a failing-then-passing
  doctest narrative; LushText's reflow behavior is unchanged under the full
  widget suite and the visual-geometry lane.

### 4.5 `gtk-lush-widgets`

Small widget and overlay-owner primitives that exist to keep geometry honest.

- **`ClipBin`** (from the former `LushtextShrinkableBin`): zero-minimum clipping bin so a
  flexible region can never push fixed chrome out of the window.
- **`RenderHoldOverlay`** (from the minimap reflow freeze): capture a child's
  last rendered pixels synchronously (`snapshot_child` + renderer texture),
  hold them over the live child (opacity-managed) across a caller-declared
  storm, warm the live child underneath, then reveal/clear through one owner so
  an unbalanced hide cannot leave an invisible child.
- **Rust features:** `ClipBin` is a GObject widget subclass, while
  `RenderHoldOverlay` is a plain Rust owner installed around a caller-provided
  `GtkOverlay`. The family does not avoid subclassing, it avoids requiring
  consumers to adopt app-specific widget patterns.
- **Acceptance:** widget tests for measure/clip contracts; visual-geometry
  scenarios proving pixel-hold and seamless reveal; the GTK warning gate stays
  clean (no adjusted-size or measure warnings under the harness).

### 4.6 `gtk-lush-proof` + `cargo-gtk-proof` (the crown jewel)

The headless verification toolchain, split into:

- **`gtk-lush-proof-harness`** (dev-dependency crate): self-supervising
  headless Mutter + private D-Bus session bootstrap, the per-test subprocess
  runner with loud flake reporting, and the shared wait helpers
  (`wait_until` drain semantics, realization/async budgets) as a documented
  API instead of copy-paste lore.
- **`gtk-lush-proof-spine`** (optional runtime crate): the readiness/snapshot
  protocol scaffolding — interface versioning, readiness predicates/blockers,
  bounded snapshot envelope — as traits the consumer implements with their own
  app state. LushText's Automation1 becomes the first implementation.
- **`cargo-gtk-proof`** (cargo subcommand, Rust): the visual-proof tool surface
  for scenario schema descriptors, bounded result envelopes, PNG comparison
  primitives, compatibility corpus replay, proof-policy checks, and the default
  same-session live visual runner. The Python live runner remains an explicit
  oracle/diagnostic path after Rust parity.

- **Acceptance:** LushText's existing lanes run unchanged on top of the
  extracted pieces where parity is recorded; the scenario schema descriptors
  are versioned and validated; a non-LushText harness example compiles without
  importing LushText.

Deliberately **not** in the family: anything covered by Libadwaita, anything
that owns app state, theming systems, and one-off LushText domain widgets.

## 5. Phase plan

This is now a historical phase plan plus a dormant-track map, not an automatic
work queue. Completed phases remain documented because they explain the
current platform. Future GTK Lush work starts only when there is current
LushText pain, evidence/check drift, proof-tooling value, or a real external
adopter pull signal. It should still begin as one coherent phase-level
OpenSpec change and split only when ownership, validation, or artifact clarity
requires it.

Each active phase is independently shippable and ends with LushText green on
its full gate set (`make check`, full widget suite, visual-geometry smoke
where visual-sensitive files changed). Historical phases map to OpenSpec
changes; Phase 1 plus the program scaffolding are covered by
`establish-gtk-lush-program`.

### Phase 0 — Prerequisite simplifications inside LushText

Goal: shrink and normalize the extraction surface before any API freezes.

1. Migrate the Markdown preview pane off the hand-animated `GtkPaned` onto an
   Adwaita-native container (`AdwOverlaySplitView` or a third
   `AdwMultiLayoutView` slot), deleting `window/preview.rs` animation code and
   retiring the bulk of the paned-animation rules.
2. Convert pure state→view projections to declarative bindings (Blueprint
   `bind`, GObject property bindings, derived properties) where a handler does
   nothing but copy values.
3. Normalize the remaining debounce/timer call sites onto one in-tree helper
   shape in `normalize-settle-timer-helpers` (the future Phase 2
   `gtk-lush-settle` API, prototyped privately at the time).
4. Re-run the complete proof set; update rules in the same changes.

Exit criteria: zero hand-animated paneds; one debounce idiom; rules updated.
(Follow-up change names: `migrate-preview-pane-to-adwaita`,
`normalize-declarative-bindings`, `normalize-settle-timer-helpers`.)

`normalize-settle-timer-helpers` was a Phase 0.3 proving ground only: it could
rename or reshape the private helper while LushText learned from real call
sites, but it could not publish a public `gtk-lush-settle` API or add
family-crate dependencies. Public settle APIs moved to
`extract-gtk-lush-signals-and-settle`.

### Phase 1 — Workspace foundation and governance (this change)

1. Create `crates/gtk-lush/` workspace members (leaf crates, placeholder
   `signals` and `settle` first), wired into the root workspace, cargo-hakari,
   nextest, and cargo-deny.
2. Establish the engineering bar as enforced configuration (Section 6): lints,
   `missing_docs`, doctests, examples, MSRV metadata, semver tooling.
3. Write the governance files: `crates/gtk-lush/GOVERNANCE.md` (constitution,
   review gates, treadmill SLA, publishing gates) and per-crate README seeds
   from the corresponding rules.
4. Licensing: family crates are dual `MIT OR Apache-2.0` (Rust-ecosystem
   default; compatible with the GPL-3.0-or-later application). REUSE-style
   SPDX headers throughout.
5. Prepare `0.0.0` placeholder packages for optional crates.io reservation
   (squat protection only; publication requires explicit maintainer approval,
   and real functional publishing is gated by the dormant publication track).
6. CI: extend existing container lanes to build/test/doc the family; add an
   MSRV verification job and a `cargo-semver-checks` job (advisory until the
   first real publish).

### Phase 2 — First extractions: `gtk-lush-signals`, `gtk-lush-settle`

Status: completed in-tree functional API phase. These crates remain `0.0.0`
workspace APIs for the internal platform and are not stable external
dependencies.

1. Design each API against its rule section; write the README first
   (rule-rewritten-as-docs), then the API, then doctests. Complete.
2. Implement with the full bar from Section 6. Complete for in-tree Phase 2.
3. Migrate LushText mechanically; handler-id fields and the private settle
   helper are deleted for fitting sites. Complete for Phase 2 scope.
4. Rewrite the corresponding rule sections to point at crate docs, keeping
   only the LushText-specific judgment calls in the rules. Complete for Phase 2
   scope.
   (Follow-up change name: `extract-gtk-lush-signals-and-settle`.)

### Phase 3 — Geometry and tasking: `gtk-lush-tasks`, `gtk-lush-viewport`, `gtk-lush-widgets`

Status: completed in-tree functional API phase. These crates remain `0.0.0`
workspace APIs for the internal platform and are not stable external
dependencies.

1. Extract `spawn_blocking_then` + explicit freshness helpers; migrate
   former `services::async_task` consumers. Complete for in-tree Phase 3.
2. Extract the viewport observers; migrate `overscroll.rs`. Complete for
   in-tree Phase 3.
3. Generalize `ShrinkableBin` → `ClipBin` and the minimap freeze →
   `RenderHoldOverlay`; migrate the minimap to consume them. Complete for
   in-tree Phase 3.
4. These are visual-sensitive migrations: each lands with widget tests plus a
   passing visual-geometry run including the pixel-anchor and animation-stream
   scenarios.
   (Follow-up change name: `extract-gtk-lush-runtime-geometry`.)

### Phase 4 — Proof toolchain extraction

Status: completed in-tree functional API phase. `gtk-lush-proof-harness` and
`gtk-lush-proof-spine` exist as functional `0.0.0` family APIs, and
`cargo-gtk-proof` exists as a separate workspace tool outside `crates/gtk-lush/`.
The Rust tool now owns typed schema validation, bounded result envelopes, the
pure PNG/detector corpus, proof-policy checks, same-session live visual proof,
animation-stream evidence, and the default `make visual-geometry-smoke`
wrapper. LushText's widget harness consumes `gtk-lush-proof-harness`, and
Automation1 has proof-spine readiness/workflow projections plus a D-Bus
introspection golden. The Python visual runner remains available only as a
   Rust-supervised oracle/diagnostic path. Adoption validation is complete;
   publishing, repository split, and upstreaming are dormant separate tracks.

1. Extract the harness crate; LushText's `tests/widget.rs` becomes a consumer.
2. Extract the spine traits; Automation1 implements them with zero D-Bus
   surface drift (`make check-automation-docs` proves it).
3. Port the visual-geometry runner to `cargo-gtk-proof` behind a
   compatibility suite: identical artifacts, summaries, and pass/fail
   decisions on a frozen corpus of recorded scenario runs before the Python
   path is retired. Complete for Phase 4; Python is now diagnostic/oracle only.
4. Publish the scenario schema (versioned JSON schema document) as part of the
   crate docs. Complete for Phase 4 in `docs/gtk-proof-schemas.md` and the
   workspace schema descriptors.
   (Follow-up change name: `extract-gtk-lush-proof-toolchain`.)

### Phase 5a — Adoption validation before publication

Status: complete and archived (`validate-gtk-lush-adoption-surface`). The
maintained evidence lives in `crates/gtk-lush-adoption-lab`,
`fixtures/gtk-lush-adoption/`, and `docs/gtk-lush-adoption/`. This evidence
is now the maintained internal-platform baseline. It can support a later
publication proposal, but it does not require one.

1. Build the second consumer: a small real application or gallery/demo app
   maintained in this workspace but outside `crates/gtk-lush/`, using every
   functional GTK Lush crate in anger without importing LushText app crates.
2. Maintain a crate-by-crate adoption matrix that names each lab workflow,
   standalone example, stock fixture, proof/test evidence, friction status,
   and API decision.
3. Run the afternoon-adoption test literally: a fresh agent session adopts one
   crate into a stock gtk-rs starter app, timed, journaled, and friction
   classified into documentation, example, naming, type-shape, feature-flag,
   missing-helper, overreach, accepted-limitation, or follow-up work.
4. Attempt at least one adoption spike in an unrelated existing gtk-rs or
   Libadwaita project, preserving only bounded notes or patch summaries here,
   not the outside project source tree.
5. Complete an API review pass driven by second-consumer, stock-starter, and
   external-project friction while all functional APIs are still `0.0.0`.
   Breaking pre-publication changes are expected when they reduce ceremony,
   remove LushText-shaped assumptions, or better satisfy the constitution.
   (Follow-up change name: `validate-gtk-lush-adoption-surface`.)

### Dormant track — Publishing and repository graduation

This track is not active. It requires a future dedicated, maintainer-approved
OpenSpec change that cites current evidence, refreshes stale evidence, and
records release, semver, docs.rs, changelog, credential, repository-history,
and rollback plans before implementation.

1. Use the archived Phase 5a evidence as the publishing gate input: two real
   consumers, timed adoption journal, unrelated-project spike, API review,
   semver/public-API advisory output, and complete crate docs.
2. Publish `0.1.0` only after the adoption gate is satisfied, with docs.rs
   metadata, CHANGELOGs, release automation, and explicit maintainer approval.
3. Graduate the family to its own repository (`gtk-lush`), preserving history
   (`git filter-repo`), with LushText consuming published versions (path
   dependencies allowed only between graduation and first publish).
   (Follow-up change name: `graduate-and-publish-gtk-lush`.)

### Optional dormant track — Upstreaming and external maintenance

This track is not active as a broad phase. Small upstream documentation or
issue work remains worthwhile when it directly removes LushText maintenance
cost, but it does not require GTK Lush publication.

1. Upstream the pure-knowledge wins where maintenance cost drops to zero:
   gtk-rs/GTK documentation issues for the layout-manager `size_allocate`
   skip; a GtkSourceView issue documenting the map-slider anchoring formula
   and margin interaction; gtk-rs cookbook entries for the capture-and-hold
   technique and `WidgetPaintable` semantics.
2. Steady-state treadmill (Section 7 SLAs): gtk-rs major bumps, GNOME SDK
   floors, MSRV reviews, dependency audits.
3. Periodic constitution audit: any accumulated feature that fails the
   discriminating test is deprecated and removed.
   (Follow-up change name: `gtk-lush-upstreaming-round-one`.)

## 6. The state-of-the-art engineering bar (every crate, no exceptions)

- `#![forbid(unsafe_code)]` (exceptions require a documented invariant and a
  dedicated review entry in GOVERNANCE.md).
- `#![deny(missing_docs)]`; every public item documented with a runnable
  doctest where behavior is observable.
- `examples/*.rs` proving single-crate adoption on stock gtk-rs.
- Workspace lint table inherited from LushText's curated set.
- Tests: unit + doctests always; headless widget tests via the harness for
  anything that touches widgets; property tests for pure decision logic.
  Mutation testing: the family's pure decision logic joins the workspace's
  deterministic cargo-mutants scope as a dormant publication-track input; until
  then `.cargo/mutants.toml` intentionally scopes mutation runs to LushText's
  model/service code, and GTK-bound family code stays out of mutation scope
  permanently per the workspace mutation rules.
- `cargo-semver-checks` and a `public-api` snapshot in CI; MSRV declared in
  `Cargo.toml` (`rust-version`) and verified by a dedicated job; `cargo-deny`
  advisories/licenses/bans/sources green.
- Versioning: SemVer with pre-1.0 minor-as-breaking discipline; conventional
  commits; per-crate CHANGELOG (Keep a Changelog format); release automation
  modeled on the repo's existing release tooling.
- Docs: README = rewritten rule; docs.rs metadata with feature flags
  documented; a top-level family page stating the constitution and the
  discriminating test verbatim.
- GNOME floor policy: each release documents the minimum GNOME SDK feature
  level it is tested against (containers in CI mirror LushText's matrix).

## 7. Governance

- **Constitution enforcement:** every PR touching the family answers a
  checklist derived from Section 2; a "constitution exception" label requires
  explicit human approval and a GOVERNANCE.md entry.
- **Treadmill SLAs:** new gtk-rs major supported within one release cycle when
  the family is on a publication track; GNOME SDK floor raised at most once
  per year; MSRV at most latest-stable minus two at each publish.
- **Publishing gates:** dormant until a dedicated publication/graduation
  proposal is approved. No `0.1.0` before (a) the Phase 5a
  adoption-validation change archives, (b) two real consumers exist, (c) the
  timed afternoon-adoption test passes, (d) at least one unrelated existing
  project adoption spike is recorded, (e) semver tooling is green, and (f)
  docs are complete. Placeholder `0.0.0` reservations carry a README pointing
  here. Functional in-tree `0.0.0` APIs may exist indefinitely as workspace
  APIs, but they must continue to state that external publication stability is
  not promised.
- **Maintenance honesty:** each crate lists its bus-factor plan; if the
  family ever becomes unmaintained, the constitution requires archiving with
  migration notes rather than silent rot.
- **Repo split:** in-tree workspace path dependencies are the current intended
  state. A dedicated repo and published LushText dependencies require the
  dormant publication/graduation track to be explicitly reopened.

## 8. Risks and mitigations

- **Framework drift** — the central risk. Mitigation: the constitution, the
  per-PR checklist, the leaf-crate rule, and demand-driven periodic audit.
- **Premature generalization from one consumer.** Mitigation: keep APIs
  `0.0.x` and in-tree unless a reopened publication track proves enough
  external adopter signal to reshape them.
- **gtk-rs version lag turning into consumer pain.** Mitigation: treadmill
  SLA, small surface area, CI matrix on GNOME containers, and a policy that a
  blocked bump halts publishing rather than forking behavior.
- **Proof-runner port regressions.** Mitigation: the frozen-corpus
  compatibility suite and Rust live visual proof in Phase 4; Python remains as
  an explicit diagnostic oracle rather than the default proof authority.
- **Maintenance load on a single maintainer.** Mitigation: small leaf crates,
  demand-driven GTK Lush work, upstream notes when they remove carried cost,
  and archiving policy.
- **License friction.** Mitigation: dual MIT/Apache for the family decided up
  front; REUSE-compliant headers from the first commit.

## 9. Success metrics

- LushText deletes more lines than the family adds (measured at each phase
  archive).
- Zero new entries in `.agents/rules/*.md` for pain classes a family crate
  owns (the rule becomes a pointer to crate docs).
- The adoption-validation baseline records a maintained second consumer,
  crate-by-crate matrix, timed afternoon-adoption journal, unrelated-project
  spike, and friction-driven API review without forcing `0.1.0`.
- LushText's full gate set stays green at every phase boundary — including
  pixel-anchor and animation-stream visual scenarios for the geometry crates.
- Optional upstream contributions reduce carried LushText maintenance cost
  when they are worthwhile.

## 10. Relationship to OpenSpec

- `gtk-lush-internal-platform`: current posture. GTK Lush is stable in-tree
  LushText infrastructure; publication, repository graduation, published
  dependency migration, and broad upstreaming are dormant future tracks.
- `establish-gtk-lush-program`: program constitution and governance as
  capability specs, Phase 1 foundation implementation, and the original
  reserved roadmap context.
- Future GTK Lush work should start as one coherent phase-level proposal and
  name its demand signal. Publication or graduation work must be its own
  maintainer-approved proposal with refreshed evidence.
- This document is the umbrella narrative and must be updated in the same
  change whenever any phase alters scope, naming, or principles.
