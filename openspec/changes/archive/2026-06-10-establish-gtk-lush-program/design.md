## Context

LushText's most valuable engineering output is not its features but the
toolkit discipline it accumulated: RAII-shaped signal bookkeeping conventions,
generation-counter scheduling, main-thread task safety, allocation observation
that survives GTK4's layout-manager vfunc skip, render-hold widgets, and a
headless pixel-proof toolchain. Today that value exists as prose in
`.agents/rules/*.md` plus in-tree code, so it cannot be reused and each new
module re-derives it. The umbrella vision (`docs/next/gtk-lush.md`) defines
the GTK Lush program; this change implements its foundation (governance +
workspace + placeholder members for the two lowest-risk crates) and
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
- Seed placeholder `gtk-lush-signals` and `gtk-lush-settle` workspace members
  with the shared scaffolding, README direction, standalone examples, and
  placeholder reservation metadata.
- Reserve and define all follow-up phases so agents can execute the program
  without re-deriving intent.

**Non-Goals:**

- No extraction of tasks/viewport/widgets/proof tooling in this change (those
  are Phases 3-4, reserved as named follow-ups).
- No implementation of `gtk-lush-signals` or `gtk-lush-settle`, and no
  LushText migration onto those crates; the dedicated
  `extract-gtk-lush-signals-and-settle` follow-up owns that work.
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

### Decision: Implementation migrations are reserved and gate-bracketed

This change creates the family rails only. The first implementation follow-up
(`extract-gtk-lush-signals-and-settle`) migrates each LushText module in its
own commit-sized step with the full relevant gate set run after it (widget
suite for widget modules; automation contract checks where readiness is
touched; visual-geometry smoke whenever a visual-sensitive file changes, per
the existing proof policy). Because the minimap files are concurrently owned
by the visual-geometry program, those specific call sites migrate last and
rebase onto whatever main carries at that time. Alternative (one big-bang
migration inside this foundation change) rejected: it makes the program
contract harder to review and the behavioral question ("did the settle window
change?") impossible to bisect.

### Decision: Rule rewrites wait until crates enforce the rule

After each implementation migration, the corresponding `.agents/rules`
section is rewritten to (a) name the crate as the required mechanism, (b)
keep only LushText-specific judgment, and (c) link the crate docs that now
carry the full rationale. The crate README starts as the rule text, rewritten
around the type. In this foundation change, README seeds may quote the future
rule direction, but the global rules keep their current LushText guidance
until the replacement mechanisms exist. This keeps `make check-agent-docs`
meaningful and stops rule drift.

### Decision: Licensing split is decided now

Family crates: dual `MIT OR Apache-2.0` (Rust-ecosystem default, maximizes
adoption, compatible with the GPL application consuming them). App stays
GPL-3.0-or-later. Deciding at foundation time avoids re-licensing consent
problems after outside contributions arrive. Alternative (LGPL to match GTK
culture) rejected: static-linking ambiguity in Rust scares off exactly the
consumers the family targets.

## Risks / Trade-offs

- [Concurrent minimap program touches the same files] → The follow-up
  signals/settle migrations for `editor_page` land last, rebase onto main,
  and re-run the visual lane; this foundation change has no runtime file
  overlap.
- [Framework drift over time] → Constitution checklist in GOVERNANCE.md,
  leaf-crate dependency audit, periodic audit task reserved in Phase 6.
- [API frozen too early around one consumer] → No `0.1.0` before the
  two-consumer and afternoon-test gates; in-tree APIs stay `0.0.x` and
  breakable.
- [Settle migration silently changes timing] → Deferred to the
  `extract-gtk-lush-signals-and-settle` follow-up; each site keeps its
  existing window constants there, widget tests that wait on pending states
  are the regression net, and the minimap scenarios pixel-verify the riskiest
  site.
- [New CI jobs add flake surface] → MSRV and semver jobs are containerized,
  pinned, and advisory until first publication.
- [Maintenance treadmill underestimated] → SLAs are specified, and the
  archiving policy is part of governance from day one.

## Migration Plan

1. Land workspace + governance + placeholder crates (no LushText behavior
   change).
2. Wire policy, CI, MSRV, docs, examples, and placeholder reservation gates.
3. Record the constitution checklist, exception register, treadmill SLAs,
   publishing gates, bus-factor/archiving policy, and repo-graduation plan in
   `crates/gtk-lush/GOVERNANCE.md`.
4. Reserve the follow-up roadmap, including
   `extract-gtk-lush-signals-and-settle`, without implementing those APIs in
   this change.
5. Run OpenSpec validation, documentation checks, and the foundation gate set.

## Open Questions

- Whether the afternoon-adoption starter app should live in the family
  workspace as a maintained example or be created fresh per test (default:
  fresh per test, journaled; a maintained gallery arrives with Phase 5).
- Final public names and API boundaries for `gtk-lush-signals` and
  `gtk-lush-settle` — settled during the dedicated extraction follow-up,
  before any global rule rewrite.
