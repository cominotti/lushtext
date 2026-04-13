<!--
Sync Impact Report
Version change: unversioned template -> 1.0.0
Modified principles:
- principle 1 placeholder -> I. Native GNOME UX Is A Contract
- principle 2 placeholder -> II. Data Safety Over Convenience
- principle 3 placeholder -> III. Deterministic Verification And Blocker Ownership
- principle 4 placeholder -> IV. Layered Rust Architecture
- principle 5 placeholder -> V. Documentation And Delivery Stay In Lockstep
Added sections:
- Engineering Constraints
- Delivery Workflow & Quality Gates
Removed sections:
- None
Templates requiring updates:
- ✅ .specify/templates/plan-template.md
- ✅ .specify/templates/spec-template.md
- ✅ .specify/templates/tasks-template.md
- ✅ .specify/templates/commands/*.md (no command templates present; no updates required)
Follow-up TODOs:
- None
-->
# LushText Constitution

## Core Principles

### I. Native GNOME UX Is A Contract
Every user-visible change MUST preserve or intentionally refine the documented
GTK4 and Libadwaita interaction contract for LushText. Focus behavior, sizing,
animation, pane geometry, action enabled-state, copy, and narrow-window
behavior MUST be described explicitly before implementation when they change.
GTK, GLib, and pixman warnings during normal usage are product defects, not
acceptable polish debt. Interactive controls MUST be fully wired and critical
actions MUST stay readable and usable at supported widths.

Rationale: LushText competes on native-feeling behavior. Visual similarity to
GNOME tools only matters if runtime behavior is equally exact and warning free.

### II. Data Safety Over Convenience
Features that touch files, drafts, session restore, search and replace, undo,
or close flows MUST prefer recoverability over convenience. Writes MUST be
atomic where feasible, destructive operations MUST provide confirmation or undo
when appropriate, and restored state MUST be deterministic after restart,
crash, or external modification. A feature is incomplete if it can plausibly
lose user work without a tested mitigation.

Rationale: Text editors lose user trust fastest through data loss. Recovery and
predictable persistence are product behavior, not optional hardening.

### III. Deterministic Verification And Blocker Ownership
Every change MUST ship with verification proportional to its risk. Changed
logic requires automated coverage at the lowest practical layer; interactive
GTK behavior requires widget or integration coverage; paned, animation, focus,
allocation, and warning-sensitive UI changes also require live validation
through `make run` while watching stderr. Pre-existing blockers discovered
during implementation or verification MUST be fixed in the same work stream
before sign-off.

Rationale: Partial verification and "pre-existing" exceptions let regressions
ship. Deterministic proof and blocker ownership keep the editor trustworthy.

### IV. Layered Rust Architecture
Domain types MUST live in `model/`, business logic and pure I/O in `services/`,
and GTK adaptation in `ui/`. Blocking or potentially slow work MUST move off
the GTK main thread through the project's background execution primitives, and
GTK objects MUST never cross thread boundaries unsafely. New abstractions MUST
reduce responsibility and clarify ownership rather than hiding complexity
behind generic managers or cross-layer leakage.

Rationale: LushText depends on clear GTK thread boundaries and maintainable Rust
layering to stay fast, testable, and resilient as features accumulate.

### V. Documentation And Delivery Stay In Lockstep
When behavior, commands, dependencies, workflows, or packaging inputs change,
the affected documentation and delivery artifacts MUST be updated in the same
change. This includes `README.md`, `AGENTS.md`, relevant `.agents/rules/*.md`
files, and any impacted build, Meson, Flatpak, or cargo source metadata. Git
hooks, signed conventional commits, workspace dependency hygiene, and Flatpak
input regeneration are required delivery work, not follow-up chores.

Rationale: Stale docs and stale build metadata recreate the same failures in
future sessions and releases.

## Engineering Constraints

- Rust changes MUST preserve the repository's pinned toolchain and gtk-rs
  version alignment across workspace dependencies.
- `make` targets are the canonical local workflow. Work completion requires the
  relevant `make test`, `make check`, and feature-specific verification paths
  to pass.
- UI changes MUST preserve the documented split-view shell, status-bar toggle
  symmetry, infobar usability, and workspace/sidebar contracts unless the
  change explicitly amends those contracts.
- Large-file, search, file-tree, draft, and session paths MUST keep the GTK
  main thread responsive and use bounded or background execution where needed.
- Dependency or packaging changes MUST update `build-aux/cargo-sources.json`,
  Meson inputs, schemas, resources, or manifests in the same work stream.

## Delivery Workflow & Quality Gates

- Every spec and implementation plan MUST state the exact user-visible
  contract, data-safety impact, architecture touch points, verification
  strategy, and documentation/build fallout for the feature.
- Every task list MUST include constitution-driven work where applicable:
  tests, live runtime validation, documentation/rule sync, and packaging
  follow-through.
- Code review and self-review MUST block on unresolved constitution violations,
  unwired controls, runtime warnings, broken tests, stale docs, or skipped
  build metadata updates.
- Complexity exceptions MUST be recorded in the implementation plan before
  implementation. "Pre-existing" is never a valid justification for leaving a
  blocker unresolved.

## Governance

This constitution supersedes conflicting guidance in Spec Kit templates and all
lower-level project workflow documents. When this constitution changes, the
same change set MUST update every affected template, rule, and guidance file.

Amendments require an explicit edit to `.specify/memory/constitution.md`, an
updated Sync Impact Report at the top of this file, and same-stream
propagation to dependent artifacts. Amendments are approved when the updated
document and its dependent artifacts are reviewed together.

Versioning follows semantic versioning for governance:
- MAJOR: remove a principle, redefine a principle incompatibly, or weaken a
  governance requirement in a way that changes compliance expectations.
- MINOR: add a principle, add a section, or materially expand mandatory
  guidance.
- PATCH: clarify wording, tighten phrasing, or make non-semantic editorial
  refinements.

Compliance review is mandatory for every spec, plan, task list, implementation,
and final verification summary. Work that cannot satisfy a principle MUST
record the reason in the plan's Complexity Tracking section and obtain explicit
review agreement before implementation continues.

**Version**: 1.0.0 | **Ratified**: 2026-04-13 | **Last Amended**: 2026-04-13
