## Why

Fourteen robustness changes landed between 2026-07-10 and 2026-07-24. They are
correct and they are worth keeping, but they left the codebase readable only to
someone who already holds the coordination machinery in their head. The cost is
now concrete and measurable:

- **The domain layer is a testability parking lot.** `model/` is pure, but half
  of its 29 files are named after mechanism (`save_admission`, `search_flight`,
  `search_retirement`, `plain_disposal`, `migration_ledger`) rather than domain.
  Six of those eight mechanism modules have exactly one consumer, almost always
  in the adjacent `ui/` directory. They live in `model/` because
  `.cargo/mutants.toml` only examines `model/**` and `services/**` — a tooling
  glob, not a design decision, is shaping the architecture.
- **Field bundles cross layer seams unnamed, and drift while crossing.** The
  save workflow carries a freshness/identity tuple through four functions as
  loose parameters. At `ui/editor_page/load_save.rs:1387` the same value is
  passed as `cancel_pending_load` and received as `explicit_destination`. A
  reader cannot verify the correctness of stale-save rejection by reading it.
  90 production functions take six or more parameters.
- **No workflow has a narrator.** Answering "what happens on Ctrl+S" requires 13
  hops across 6 files, with control inverted through an `idle_add_once` drain, and
  no document or module narrates it. `load_save.rs` is 1,795 lines holding two
  distinct workflows.
- **Two parallel introspection APIs exist and do not know about each other.**
  `model/automation.rs` + `ui/automation.rs` (3,995 lines) expose 18 typed
  `Automation*Snapshot` types, read-only over D-Bus, documented and protected by
  `make check-automation-docs`. Alongside them, 639 `#[cfg(feature =
  "test-utils")]` sites grew a shadow API of 300 `pub fn *_for_test` functions
  covering overlapping state, with no types, no documentation, and no drift gate.

None of this is a call for new abstraction layers. The hexagonal boundary holds
(`model/` and `services/` contain zero GTK imports) and the machinery modules are
individually well written. What is missing is that **a workflow has no home where
its whole story fits** — its pure policy, its coordination, its adapter, and its
evidence are scattered across four directories and two vocabularies.

This change establishes the target shape once, proves it on one workflow, and
enumerates every remaining workflow before any of them is migrated.

## What Changes

This is Phase 0 of a multi-change programme. It deliberately does **not** migrate
the codebase; it makes migration safe, sized, and governed.

- **Census before migration.** Audit and classify every LushText workflow into a
  new `docs/workflow-readability-matrix.md` with stable row ids, current shape,
  target shape, risk tier, and migration status. Outliers that may not fit the
  pattern (`minimap.rs` at 3,779 lines, `editor_memory` with five real consumers,
  the recently decomposed `markdown_preview`) must be classified explicitly as
  conforming, exempt, or deferred *before* the shape becomes normative.
- **Normative workflow shape.** Define the per-workflow module contract: a
  narrative facade that delegates, reified intent/identity value objects at every
  seam, pure `policy.rs` co-located with its consumer, coordination modules, and
  one typed evidence surface. Include naming rules that require workflow-intent
  names over mechanism names at public and cross-module boundaries.
- **Evidence consolidation contract.** Define typed per-workflow evidence
  surfaces that replace ad-hoc `*_for_test` inspection functions, and require the
  existing `Automation*Snapshot` types to project from those surfaces rather than
  read widget state independently. Classify the three distinct kinds currently
  conflated under one `_for_test` suffix: inspection (351 sites), configuration
  (45), and workflow actuation (~150). Actuation seams are explicitly deferred to
  a later change because they signal a missing workflow/dialog boundary, which is
  a design change rather than a relocation.
- **Mutation scope by convention, not by file list.** Add a `ui/**/policy.rs` glob
  to `.cargo/mutants.toml` so pure policy keeps mutation coverage wherever it
  lives. The existing hand-listed UI file entries and the roughly 40 `exclude_re`
  lines that enumerate GTK adapter method names are *not* retired by this change;
  each retires when its own workflow migrates, and the minimap entries retire in
  the last migration change. This change establishes the convention that makes
  those retirements possible.
- **Exemplar: the search panel.** Migrate `ui/search_panel/` plus
  `model/search_flight.rs` and `model/search_retirement.rs` to the target shape.
  Chosen because it is already closest to the target, has single-workflow
  consumers, touches no user data, and is cheap to revert — so the pattern is
  proven before `save`, `draft`, or `session` are touched.
- **Standing guidance alignment.** Review and revise `.agents/rules/*.md`,
  `.agents/skills/*/SKILL.md`, `.agents/skills/*/references/*.md`, `AGENTS.md`,
  and `README.md` so no standing instruction contradicts the convention. At least
  one direct conflict already exists: `.agents/rules/build.md:378-381` forbids
  adding UI modules to the cargo-mutants scope, which would reject the
  `ui/**/policy.rs` convention as written. `.agents/rules/rust.md`'s
  "Coordination Vocabulary" section teaches the machinery vocabulary as the thing
  to learn and must be reframed as an implementation tier beneath the domain
  vocabulary.
- **Retroactive-amendment governance.** Require that any later amendment to the
  convention re-migrates already-migrated workflows in the same change, so the
  programme cannot manufacture coexisting generations of its own convention.

Non-goals for this change: migrating `save`, `load`, `draft`, `session`,
`workspace`, `notes`, `palette`, or `minimap`; changing any user-visible
behavior; changing the public D-Bus automation contract; reifying any workflow as
an explicit state machine; extracting anything into a GTK Lush crate.

## Capabilities

### New Capabilities

- `workflow-readability-boundaries`: The normative per-workflow module shape —
  narrative facade, reified seam value objects, co-located pure policy,
  intent-first naming, the workflow census matrix, migration risk tiers,
  retroactive-amendment governance, and the requirement that standing agent
  guidance stay consistent with the convention.
- `workflow-evidence-surfaces`: Typed per-workflow evidence surfaces that replace
  ad-hoc test-only inspection functions, the classification of inspection versus
  configuration versus actuation seams, and the projection relationship between
  workflow evidence and the automation snapshot spine.

### Modified Capabilities

- `gtk-adapter-module-boundaries`: Currently requires only that extracted modules
  stay inside the owning UI subtree and that pure policy live in a plain Rust
  module. Adds the specific decomposition contract (facade/intent/policy/
  coordination/evidence roles), and permits pure policy previously hoisted into
  `model/` to move down beside its single consumer without losing purity or
  mutation coverage.
- `mutation-testing`: Replaces the hand-maintained UI file list and adapter-method
  `exclude_re` entries with a pure-policy naming convention, and requires
  mutation-coverage parity evidence whenever policy relocates.
- `dbus-automation-spine`: Requires automation snapshots to project from
  workflow evidence surfaces rather than gathering widget state independently,
  and extends existing documentation drift checking to cover evidence surfaces.
  The externally visible D-Bus contract is unchanged.

## Impact

**Code touched in this change**

- `ui/search_panel/**` (exemplar migration; 1,024-line `replace.rs` and 841-line
  `imp.rs` are the bulk), plus relocation of `model/search_flight.rs` and
  `model/search_retirement.rs`.
- `.cargo/mutants.toml` (scope convention; exclude_re retirement).
- `docs/workflow-readability-matrix.md` (new), `docs/automation-reference.md`
  (evidence projection), `README.md`, `AGENTS.md`.
- `.agents/rules/build.md`, `.agents/rules/rust.md`,
  `.agents/rules/widget-wiring.md`, `.agents/rules/documentation.md`, and the
  `rust-hex-arch`, `gtk-testing`, `rust-comments`, `gtk-perf-review`, and
  `data-safety` skills.
- `scripts/` — a policy check for matrix coverage and convention conformance,
  wired into `make check-policy`.

**Code enumerated but not touched**

Every other workflow gains a matrix row with a target shape and risk tier. The
programme that follows is expected to be roughly five vertical migration changes
plus one global sweep, ordered by increasing risk: search/replace and palette,
then save and load, then draft/recovery and session, then workspace tree and
notes, then minimap, then the residual-cleanup sweep.

**Verification**

`make check`, `make check-policy`, `make test`, `make test-widget-headless`,
`make mutants-diff` for policy-relocation parity, `make check-agent-docs`, and
`make check-automation-docs`. Behavior equivalence for the exemplar is the
acceptance bar: no user-visible change, no new runtime warnings, and identical
search/replace safety behavior.
