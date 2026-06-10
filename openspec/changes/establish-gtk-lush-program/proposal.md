## Why

LushText's hardest-won engineering value — signal-lifetime discipline,
generation-counter settle scheduling, main-thread task safety, allocation
observation that survives GTK4's layout-manager vfunc skip, render-hold
widgets, and the headless visual-proof toolchain — exists only as prose rules
plus in-tree code, so every new Rust + GTK4/Libadwaita project (and every new
LushText module) re-pays for it. This change establishes the GTK Lush program:
the governed extraction of those patterns into a small family of independently
adoptable leaf crates, with the umbrella vision recorded in
`docs/next/gtk-lush.md` and the program's constitution enforced as
requirements so it can never quietly become a framework.

## What Changes

- Add the umbrella vision document `docs/next/gtk-lush.md` (program narrative:
  principles, crate family, Phases 0-6, engineering bar, governance,
  upstreaming, metrics) and keep it authoritative alongside these specs.
- Establish program governance as a verifiable capability: the anti-framework
  constitution (no control-flow ownership, no view DSL, no state/message
  system, leaf crates only, Adwaita stays authoritative), the
  afternoon-adoption test, treadmill SLAs, licensing (dual MIT OR Apache-2.0
  for the family), publishing gates, and the named follow-up roadmap.
- Create the in-tree `crates/gtk-lush/` workspace area wired into the existing
  workspace, hakari, nextest, lint table, and cargo-deny, with the
  state-of-the-art per-crate engineering bar enforced as configuration.
- Implement the first two leaf crates against that bar: `gtk-lush-signals`
  (RAII signal/binding lifetime bags) and `gtk-lush-settle`
  (generation-counter debounce, settle bursts, superseding timers).
- Migrate LushText onto both crates (handler-id bookkeeping deleted from imp
  structs; the hand-rolled debounce/settle/timer sites collapsed onto the
  shared primitives) with zero behavior change, proven by the full existing
  gate set including visual-geometry smoke.
- Rewrite the corresponding `.agents/rules/*.md` sections to point at crate
  documentation, keeping only LushText-specific judgment in the rules.
- Reserve the program's later phases as named follow-up changes
  (`migrate-preview-pane-to-adwaita`, `normalize-declarative-bindings`,
  `extract-gtk-lush-runtime-geometry`, `extract-gtk-lush-proof-toolchain`,
  `graduate-and-publish-gtk-lush`, `gtk-lush-upstreaming-round-one`), each of
  which MUST conform to the governance capability introduced here.

## Capabilities

### New Capabilities

- `gtk-lush-program-governance`: the program constitution, discriminating
  adoption test, engineering bar, licensing, treadmill SLAs, publishing gates,
  follow-up roadmap conformance, and vision-document consistency requirements
  that every GTK Lush phase and crate must satisfy.
- `gtk-lush-workspace`: the in-tree family workspace — crate layout, build and
  CI integration (workspace, hakari, nextest, deny, MSRV verification, semver
  tooling), per-crate scaffolding (docs, doctests, standalone examples,
  changelogs), and crates.io name reservation policy.
- `gtk-lush-signals`: RAII lifetime management for GObject signal handlers,
  property bindings, and controller registrations, replacing manual
  handler-id bookkeeping in LushText.
- `gtk-lush-settle`: generation-counter scheduling primitives (debounce,
  settle bursts with queryable pending state, superseding timers) replacing
  LushText's hand-rolled copies while preserving readiness semantics.

### Modified Capabilities

- None. LushText's existing capabilities (editor minimap, visual geometry
  invariants, automation spine, etc.) must hold unchanged through the
  migrations; preserving them is an explicit requirement of the new
  capabilities, not a change to their specs.

## Impact

- New code: `crates/gtk-lush/signals`, `crates/gtk-lush/settle`,
  `crates/gtk-lush/GOVERNANCE.md`, workspace/CI/policy wiring.
- Migrated code: `crates/lushtext-core/src/ui/**` imp structs and dispose/Drop
  blocks (handler-id fields), the debounce/timer sites in `editor_page/`
  (minimap settle, overscroll scheduling), `window/` (status messages, drafts,
  search, notes, focus indexing), and `sidebar/` (tree loading) — behavior
  preserved.
- Docs/rules: `docs/next/gtk-lush.md` (new), `README.md` (architecture note),
  `AGENTS.md` module layout, `.agents/rules/rust.md` and
  `.agents/rules/widget-wiring.md` (sections rewritten to reference crate
  docs), CI workflow updates for the new lanes.
- Dependencies: dev tooling additions pinned in CI (`cargo-semver-checks`,
  `cargo-public-api`); no new runtime dependencies for LushText beyond the
  family crates themselves.
- Tests: existing suites must stay green unmodified in behavior; new unit,
  doctest, property, and widget coverage for the two crates; mutation scope
  extended to their pure logic.
