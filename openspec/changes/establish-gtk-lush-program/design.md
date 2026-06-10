## Context

LushText's most valuable engineering output is not its features but the
toolkit discipline it accumulated: RAII-shaped signal bookkeeping conventions,
generation-counter scheduling, main-thread task safety, allocation observation
that survives GTK4's layout-manager vfunc skip, render-hold widgets, and a
headless pixel-proof toolchain. Today that value exists as prose in
`.agents/rules/*.md` plus in-tree code, so it cannot be reused and each new
module re-derives it. The umbrella vision (`docs/next/gtk-lush.md`) defines
the GTK Lush program; this change implements its foundation (governance +
workspace + the two lowest-risk crates and their LushText migrations) and
binds all later phases to the governance capability.

Constraints that shape the design: another active program (minimap/visual
geometry hardening) touches the same `ui/editor_page` files, so migrations
must be mechanical and rebase-friendly; the application is GPL-3.0-or-later
while reusable crates need permissive licensing; and the repo's verification
culture (warning gates, visual proof policy, mutation scope) must apply to the
new crates from their first commit, not retroactively.

## Goals / Non-Goals

**Goals:**

- Make the program's anti-framework constitution and quality bar verifiable
  requirements, not aspirations.
- Land `crates/gtk-lush/` with full workspace/CI integration and the
  state-of-the-art per-crate bar enforced by configuration.
- Ship `gtk-lush-signals` and `gtk-lush-settle` and migrate LushText onto
  them with zero observable behavior change.
- Convert the corresponding rules prose into crate documentation, leaving
  pointers plus LushText-specific judgment behind.
- Reserve and define all follow-up phases so agents can execute the program
  without re-deriving intent.

**Non-Goals:**

- No extraction of tasks/viewport/widgets/proof tooling in this change (those
  are Phases 3-4, reserved as named follow-ups).
- No crates.io functional publication (Phase 5 gate); placeholders only.
- No repo split; the family stays in-tree until publishing gates pass.
- No Phase 0 UI simplifications here (`migrate-preview-pane-to-adwaita`,
  `normalize-declarative-bindings` are independent follow-ups; this change
  does not depend on them).
- No view DSL, state system, or any feature requiring a constitution
  exception.

## Decisions

### Decision: Library family, not a framework — enforced by tests, not taste

The discriminating test ("one crate, one afternoon, no restructuring") is
encoded three ways: leaf-crate dependency policy checked in the workspace
audit, a mandatory `examples/standalone.rs` per crate, and the journaled
afternoon-adoption test as a publishing gate. Alternatives considered:
adopting Relm4 (rejected: paradigm migration cost, competes with Blueprint,
does not touch the genuinely hard toolkit layers) and keeping patterns
in-tree as documentation only (rejected: prose does not compose, and the
second consumer never materializes).

### Decision: Extract in place, graduate later

The family lives in this repository as workspace members consumed by path
until the Phase 5 gates pass, then graduates to its own repository with
history preserved. Rationale: LushText is the only honest API reviewer the
crates have today; in-tree extraction keeps every migration inside the
existing gate set. Alternative (new repo immediately) rejected: it forks CI,
slows iteration, and invites API freezing before a second consumer exists.

### Decision: Order extractions by risk, not by value

Signals and settle go first because their migrations are mechanical, widely
exercised by existing tests, and not visually sensitive (no pixel output
changes). The geometry widgets and proof toolchain — higher value, higher
risk — wait for the foundation plus the Phase 0 simplifications. Alternative
(lead with the proof toolchain as the most differentiated asset) rejected:
its genericization is the hardest design problem in the program and deserves
a dedicated change once the family conventions exist.

### Decision: `SignalBag` owns disconnects; sources are held weakly

The bag records `(weak source, handler id)` pairs and disconnects on clear or
drop, tolerating finalized sources. Widgets own their bags in `imp` structs;
handlers on app-global objects (Settings, StyleManager) use the same bag so
the existing `Drop`-based disconnect blocks are deleted rather than wrapped.
Alternative (strong source references) rejected: it converts today's leak
*conventions* into leak *guarantees* by keeping cycles alive. Alternative
(scope-guard per handler) rejected: per-field guards reproduce exactly the
bookkeeping noise the crate exists to delete.

### Decision: Settle primitives expose `pending()` and complete in one dispatch

`SettleBurst` mirrors the minimap reflow design proven in this repo: open or
extend restarts the window, one on-settle action runs from a single main-loop
callback, and `pending()` covers open-through-repair so readiness predicates
can consume it without a new blocker name. The pure generation arithmetic
lives in a GLib-free module for unit, property, and mutation testing.
Alternative (futures/async-based debounce) rejected: it drags an executor
opinion into a crate whose whole point is to follow GTK's main context.

### Decision: Migrations are per-module and gate-bracketed

Each LushText module migrates in its own commit-sized step with the full
relevant gate set run after it (widget suite for widget modules; automation
contract checks where readiness is touched; visual-geometry smoke whenever a
visual-sensitive file changes, per the existing proof policy). Because the
minimap files are concurrently owned by the visual-geometry program, those
specific call sites migrate last and rebase onto whatever main carries at
that time. Alternative (one big-bang migration) rejected: it makes the
inevitable behavioral question ("did the settle window change?") impossible
to bisect.

### Decision: Rules become pointers, docs become the rule

After each migration, the corresponding `.agents/rules` section is rewritten
to (a) name the crate as the required mechanism, (b) keep only
LushText-specific judgment, and (c) link the crate docs that now carry the
full rationale. The crate README starts as the rule text, rewritten around
the type. This keeps `make check-agent-docs` meaningful and stops rule drift.

### Decision: Licensing split is decided now

Family crates: dual `MIT OR Apache-2.0` (Rust-ecosystem default, maximizes
adoption, compatible with the GPL application consuming them). App stays
GPL-3.0-or-later. Deciding at foundation time avoids re-licensing consent
problems after outside contributions arrive. Alternative (LGPL to match GTK
culture) rejected: static-linking ambiguity in Rust scares off exactly the
consumers the family targets.

## Risks / Trade-offs

- [Concurrent minimap program touches the same files] → Signals/settle
  migrations for `editor_page` land last, rebase onto main, and re-run the
  visual lane; the crates themselves have no file overlap.
- [Framework drift over time] → Constitution checklist in GOVERNANCE.md,
  leaf-crate dependency audit, periodic audit task reserved in Phase 6.
- [API frozen too early around one consumer] → No `0.1.0` before the
  two-consumer and afternoon-test gates; in-tree APIs stay `0.0.x` and
  breakable.
- [Settle migration silently changes timing] → Each site keeps its existing
  window constants; widget tests that wait on pending states are the
  regression net; the minimap scenarios pixel-verify the riskiest site.
- [New CI jobs add flake surface] → MSRV and semver jobs are containerized,
  pinned, and advisory until first publication.
- [Maintenance treadmill underestimated] → SLAs are specified, and the
  archiving policy is part of governance from day one.

## Migration Plan

1. Land workspace + governance + empty crates (no LushText behavior change).
2. Implement `gtk-lush-signals`; migrate non-editor modules first
   (preferences bindings, sidebar, window), then `editor_page` last.
3. Implement `gtk-lush-settle`; migrate sites in the same order; minimap
   settle last, immediately followed by the full visual-geometry run.
4. Rewrite rules sections; run `make check-agent-docs` and the full gate set.
5. Roll back strategy: each migration step is independently revertible to the
   hand-rolled pattern because the crates wrap, not change, the underlying
   GLib calls.

## Open Questions

- Whether `SignalBag` should also absorb tick-callback and idle-source
  lifetimes in this change or defer that to the settle/tasks boundary
  (default: defer; revisit when `gtk-lush-tasks` is designed).
- Whether the afternoon-adoption starter app should live in the family
  workspace as a maintained example or be created fresh per test (default:
  fresh per test, journaled; a maintained gallery arrives with Phase 5).
- Final public names (`SignalBag` vs `HandlerBag`, `SettleBurst` vs
  `ReflowBurst`) — settled during API review in implementation, before the
  rules rewrite.
