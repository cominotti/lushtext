# GTK Lush — Umbrella Vision

Status: proposed (umbrella vision for the `establish-gtk-lush-program` OpenSpec
change and its follow-up changes)

GTK Lush is the extraction of LushText's hardened GTK4/Libadwaita patterns into
a small family of independently adoptable Rust crates, plus a reusable headless
proof toolchain, governed so that it can never quietly become a framework.

This document is the single narrative for the whole program: principles, crate
family, phase-by-phase plan, engineering bar, governance, upstreaming, risks,
and success metrics. The companion OpenSpec change captures the same program as
verifiable requirements and agent-executable tasks. If the two ever disagree,
the OpenSpec specs win and this document must be updated in the same change.

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
| Signal/binding handler lifetimes | ~10 `RefCell<Option<SignalHandlerId>>` fields per `imp.rs` + manual `dispose`/`Drop` choreography | `rust.md` (GObject subclassing), `widget-wiring.md` |
| Generation-counter debounce / settle bursts | Hand-rolled in 8+ modules (minimap settle, status-bar auto-dismiss, tree loading, drafts, search) | `widget-wiring.md` (Auto-Dismiss Timers), `ui.md` (minimap reflow burst) |
| Main-thread-safe background tasks | `services::async_task::spawn_blocking_then` + per-site generation stamping | `rust.md` (Background I/O, snapshot boundaries) |
| Viewport observation without dead vfuncs | `editor_page/overscroll.rs` adjustment observers + rest-state tracking | `widget-wiring.md` (size_allocate vs layout managers) |
| Zero-min clipping bin | `LushtextShrinkableBin` | `ui.md` (Split-View Rules) |
| Render-hold overlay (freeze last-good pixels across a layout storm) | Minimap reflow freeze (`editor_page/minimap.rs`) | `ui.md` (minimap guardrails), `widget-wiring.md` |
| Headless widget harness | `crates/lushtext/tests/widget.rs` + `scripts/run-widget-tests.sh` + `tests/widget/common.rs` wait helpers | `gtk-testing` skill |
| Readiness/snapshot automation spine | `ui/automation.rs`, `model/automation.rs`, D-Bus Automation1 | `docs/automation.md`, `widget-wiring.md` |
| Visual-geometry proof lane | `scripts/visual-geometry-smoke.py`, `test-visual-geometry.py`, `visual_geometry_png.py`, scenario JSONs, `check-visual-proof-policy.py` | `build.md`, `ui.md`, `gtk-agentic-debugging` skill |

The README of each extracted crate is the corresponding rule section, rewritten
around the type that now enforces it. The documentation is already paid for.

## 4. The crate family

Workspace layout: `crates/gtk-lush/<member>` with package names
`gtk-lush-<member>`. All leaves; no member depends on another member.

### 4.1 `gtk-lush-signals`

RAII lifetime management for GObject signal handlers, property bindings, and
controller registrations.

- **API sketch:** `SignalBag` / `BindingBag` value types stored on an `imp`
  struct; `bag.connect(&obj, obj.connect_changed(...))` records the pair;
  `Drop` (or explicit `clear()`) disconnects everything. Typed helpers for the
  common connect-and-track call shapes; `WeakBag` variants for handlers on
  objects that outlive the widget (Settings, StyleManager).
- **Rust features:** RAII/`Drop`, ownership to encode "handler must not
  outlive widget", zero unsafe.
- **Replaces in LushText:** every hand-tracked handler-id field plus the
  matching `dispose`/`Drop` blocks in `editor_page/imp.rs`, `window/imp.rs`,
  sidebar, and preferences bindings.
- **Acceptance:** all existing widget tests pass after migration; a doctest
  demonstrates leak-free disconnect on drop; a standalone example uses the
  crate with a plain `gtk::Label` and no other family crate.

### 4.2 `gtk-lush-settle`

Generation-counter scheduling: debounce, settle-bursts, and superseding timers
as one audited primitive.

- **API sketch:** `Debounce::new(duration)` with `.schedule(weak_target, f)`
  (generation bump + `timeout_add_local_once`); `SettleBurst` with
  `open()/extend()/on_settle(f)` semantics and a queryable `pending()` state
  for readiness integration; `SupersedingTimer` for auto-dismiss flows.
- **Rust features:** generic over the captured target via `glib::WeakRef`,
  `Cell`-based state, no interior panics; pure decision logic split out for
  unit and property tests.
- **Replaces in LushText:** the 8+ hand-rolled generation counters, including
  the minimap reflow settle, status-bar message dismiss, tree loading, draft
  autosave debounce, and search scheduling.
- **Acceptance:** migrated sites are line-for-line simpler; property tests for
  the pure generation logic run under `make test-prop`; readiness predicates
  (`minimap_work_pending` etc.) read `pending()` without behavior change.

### 4.3 `gtk-lush-tasks`

Main-thread-safe background work with staleness-proof completion.

- **API sketch:** `spawn_blocking_then(state, work, then)` extracted as-is,
  plus a typestate completion token: `then` receives a `Fresh<T>` that can only
  be applied to UI state after the embedded generation/identity check passes —
  the "stale worker result mutates UI" bug becomes unrepresentable.
- **Rust features:** typestate, `glib::thread_guard::ThreadGuard` wrapping,
  `Send` bounds that document the thread contract in signatures.
- **Replaces in LushText:** `services::async_task` and the per-call-site
  generation/weak-identity checks documented in `rust.md`.
- **Acceptance:** all async-backed widget tests pass unmodified in timing;
  the low-priority idle completion contract (required by `wait_until`) is
  preserved and documented as a stability guarantee.

### 4.4 `gtk-lush-viewport`

Allocation-derived geometry observation for widgets whose `size_allocate`
vfunc never fires (layout-manager classes — `GtkBox`, and anything else GTK4
routes through a layout manager).

- **API sketch:** `ViewportObserver::for_scrollable(&text_view)` (or any
  `Scrollable`) emitting `WidthChanged`/`HeightChanged` events from adjustment
  page-size deltas, plus `RestState` tracking (at-left/at-top recorded only
  outside reflow windows so GTK-preserved offsets cannot masquerade as user
  intent).
- **Rust features:** sealed event enum, callback registration that composes
  with `gtk-lush-signals`-style ownership without depending on it.
- **Replaces in LushText:** `editor_page/overscroll.rs::setup_allocation_reflow_observers`
  and friends.
- **Acceptance:** the documented dead-vfunc trap has a failing-then-passing
  doctest narrative; LushText's reflow behavior is unchanged under the full
  widget suite and the visual-geometry lane.

### 4.5 `gtk-lush-widgets`

The small custom widgets that exist to keep geometry honest.

- **`ClipBin`** (from `LushtextShrinkableBin`): zero-minimum clipping bin so a
  flexible region can never push fixed chrome out of the window.
- **`RenderHoldOverlay`** (from the minimap reflow freeze): capture a child's
  last rendered pixels synchronously (`snapshot_child` + renderer texture),
  hold them over the live child (opacity-managed) across a declared "storm",
  then reveal after a settle/warm window. Generalized API: `hold()`,
  `extend()`, `reveal_after(duration)`, with the opacity pairing enforced by
  one owner so an unbalanced hide cannot leave an invisible child.
- **Rust features:** ordinary GObject subclasses (these are widgets; the
  family does not avoid subclassing, it avoids *requiring consumers* to adopt
  patterns).
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
- **`cargo-gtk-proof`** (cargo subcommand, Rust): the visual-geometry runner —
  scenario schema (versioned, published), same-session before/after capture,
  protected regions, pixel anchors, relative anchors, animation-frame stream
  sampling, artifact layout, and the proof-policy checker. The current Python
  runners are the executable specification; the subcommand ports them with a
  pixel-for-pixel compatibility suite before any LushText script is deleted.

- **Acceptance:** LushText's existing lanes run unchanged on top of the
  extracted pieces (its scripts become thin wrappers during migration); the
  scenario schema is versioned and validated; a non-LushText demo app passes a
  trivial scenario end-to-end in CI.

Deliberately **not** in the family: anything covered by Libadwaita, anything
that owns app state, theming systems, and one-off LushText domain widgets.

## 5. Phase plan

Each phase is independently shippable and ends with LushText green on its full
gate set (`make check`, full widget suite, visual-geometry smoke where
visual-sensitive files changed). Phases map to OpenSpec changes; Phase 1 plus
the program scaffolding are covered by `establish-gtk-lush-program`, and each
later phase is proposed as its own change when its predecessor archives
(follow-up names reserved below).

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
   shape (the future `gtk-lush-settle` API, prototyped privately).
4. Re-run the complete proof set; update rules in the same changes.

Exit criteria: zero hand-animated paneds; one debounce idiom; rules updated.
(Follow-up change names: `migrate-preview-pane-to-adwaita`,
`normalize-declarative-bindings`.)

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
5. Reserve crate names on crates.io with `0.0.0` placeholders (squat
   protection only; real publishing is gated by Phase 5).
6. CI: extend existing container lanes to build/test/doc the family; add an
   MSRV verification job and a `cargo-semver-checks` job (advisory until the
   first real publish).

### Phase 2 — First extractions: `gtk-lush-signals`, `gtk-lush-settle`

1. Design each API against its rule section; write the README first
   (rule-rewritten-as-docs), then the API, then doctests.
2. Implement with the full bar from Section 6.
3. Migrate LushText mechanically, one module at a time, full gates after each
   module; handler-id fields and hand-rolled counters are deleted, not
   wrapped.
4. Rewrite the corresponding rule sections to point at crate docs, keeping
   only the LushText-specific judgment calls in the rules.
   (Follow-up change name: `extract-gtk-lush-signals-and-settle`.)

### Phase 3 — Geometry and tasking: `gtk-lush-tasks`, `gtk-lush-viewport`, `gtk-lush-widgets`

1. Extract `spawn_blocking_then` + typestate completion; migrate
   `services::async_task` consumers.
2. Extract the viewport observers; migrate `overscroll.rs`.
3. Generalize `ShrinkableBin` → `ClipBin` and the minimap freeze →
   `RenderHoldOverlay`; migrate the minimap to consume them.
4. These are visual-sensitive migrations: each lands with widget tests plus a
   passing visual-geometry run including the pixel-anchor and animation-stream
   scenarios.
   (Follow-up change name: `extract-gtk-lush-runtime-geometry`.)

### Phase 4 — Proof toolchain extraction

1. Extract the harness crate; LushText's `tests/widget.rs` becomes a consumer.
2. Extract the spine traits; Automation1 implements them with zero D-Bus
   surface drift (`make check-automation-docs` proves it).
3. Port the visual-geometry runner to `cargo-gtk-proof` behind a
   compatibility suite: identical artifacts, summaries, and pass/fail
   decisions on a frozen corpus of recorded scenario runs before the Python
   path is retired. The proof-policy checker moves last.
4. Publish the scenario schema (versioned JSON schema document) as part of the
   crate docs.
   (Follow-up change name: `extract-gtk-lush-proof-toolchain`.)

### Phase 5 — Second consumer and the publishing gate

1. Build the second consumer: a small real application (or gallery/demo app
   maintained in the family workspace) that uses every crate in anger, plus at
   least one crate adopted by an unrelated existing project.
2. Run the afternoon-adoption test literally: a fresh agent session adopts one
   crate into a stock gtk-rs starter app, timed, journaled, friction logged as
   issues.
3. API review pass driven by second-consumer friction; only then `0.1.0`
   publishes with docs.rs metadata, CHANGELOGs, and release automation.
4. Graduate the family to its own repository (`gtk-lush`), preserving history
   (`git filter-repo`), with LushText consuming published versions (path
   dependencies allowed only between graduation and first publish).
   (Follow-up change name: `graduate-and-publish-gtk-lush`.)

### Phase 6 — Upstreaming and steady-state maintenance

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
- `examples/standalone.rs` proving single-crate adoption on stock gtk-rs.
- Workspace lint table inherited from LushText's curated set.
- Tests: unit + doctests always; headless widget tests via the harness for
  anything that touches widgets; property tests for pure decision logic;
  mutation testing included in the deterministic scope.
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
- **Treadmill SLAs:** new gtk-rs major supported within one release cycle;
  GNOME SDK floor raised at most once per year; MSRV at most latest-stable
  minus two at each publish.
- **Publishing gates:** no `0.1.0` before (a) two real consumers, (b) the
  timed afternoon-adoption test passes, (c) semver tooling green, (d) docs
  complete. Placeholder `0.0.0` reservations carry a README pointing here.
- **Maintenance honesty:** each crate lists its bus-factor plan; if the
  family ever becomes unmaintained, the constitution requires archiving with
  migration notes rather than silent rot.
- **Repo split:** in-tree until Phase 5 gates pass; then a dedicated repo with
  history preserved; LushText pins published versions thereafter.

## 8. Risks and mitigations

- **Framework drift** — the central risk. Mitigation: the constitution, the
  per-PR checklist, the leaf-crate rule, and the periodic audit (Phase 6.3).
- **Premature generalization from one consumer.** Mitigation: Phase 5 gate;
  APIs stay `0.0.x` and in-tree until a second consumer has reshaped them.
- **gtk-rs version lag turning into consumer pain.** Mitigation: treadmill
  SLA, small surface area, CI matrix on GNOME containers, and a policy that a
  blocked bump halts publishing rather than forking behavior.
- **Proof-runner port regressions.** Mitigation: the frozen-corpus
  compatibility suite in Phase 4; Python stays until the corpus decides.
- **Maintenance load on a single maintainer.** Mitigation: five boring crates
  over one clever one; upstreaming knowledge instead of carrying it; archiving
  policy.
- **License friction.** Mitigation: dual MIT/Apache for the family decided up
  front; REUSE-compliant headers from the first commit.

## 9. Success metrics

- LushText deletes more lines than the family adds (measured at each phase
  archive).
- Zero new entries in `.agents/rules/*.md` for pain classes a family crate
  owns (the rule becomes a pointer to crate docs).
- The afternoon-adoption test passes, timed and journaled, before `0.1.0`.
- LushText's full gate set stays green at every phase boundary — including
  pixel-anchor and animation-stream visual scenarios for the geometry crates.
- At least three upstream contributions accepted (docs or issues) by the end
  of Phase 6.

## 10. Relationship to OpenSpec

- `establish-gtk-lush-program` (this change): program constitution and
  governance as capability specs, Phase 1 foundation implementation, and the
  reserved follow-up roadmap (Phases 0 and 2–6 named above), each later phase
  arriving as its own proposal that must conform to the
  `gtk-lush-program-governance` capability.
- This document is the umbrella narrative and must be updated in the same
  change whenever any phase alters scope, naming, or principles.
