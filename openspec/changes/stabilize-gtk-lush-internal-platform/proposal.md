## Why

GTK Lush has already delivered its main value to LushText: the app now
consumes small in-tree crates for hardened GTK lifecycle, timing, tasking,
geometry, widget, and proof patterns, and adoption validation found no
blocking API friction. The next opportunity is to stabilize that as an
intentional internal platform so the project does not keep spending effort on
publication, repository split, or upstreaming work without a real external
pull signal.

## What Changes

- Reframe GTK Lush from an active expansion/publication track into a stable
  LushText-first internal platform.
- Keep the functional `0.0.0` family crates in-tree and path-dependent for
  LushText, with existing policy, doctest, example, adoption, public-API
  advisory, and proof gates maintained.
- Preserve publication and repository graduation as explicit future options
  only when a maintainer deliberately reopens that track with real external
  demand, not as the default next step.
- Add an internal-platform stewardship contract that covers when GTK Lush work
  is worth doing: real LushText pain, adoption evidence drift, proof-tooling
  improvement, or external adopter pull.
- Prune or rewrite roadmap, governance, README, adoption handoff, and local
  guidance that still implies Phase 5b publication/graduation or Phase 6
  upstreaming should happen automatically.
- Keep the larger phase-level OpenSpec posture: one coherent platform
  stabilization change, not many small per-crate follow-up specs.
- No functional crates are published, no `0.1.0` releases are prepared, no
  repository split is performed, and no LushText dependency is moved from
  workspace path dependencies to published crates.

## Capabilities

### New Capabilities

- `gtk-lush-internal-platform`: Defines GTK Lush as a stable in-tree
  LushText platform, including stewardship rules, allowed future work, frozen
  publication posture, evidence maintenance, and exit criteria.

### Modified Capabilities

- `gtk-lush-program-governance`: Change the governance posture so
  publication/graduation/upstreaming are optional reopened tracks rather than
  implied next phases, while preserving the constitution, publishing gates,
  maintenance honesty, and adoption evidence.
- `gtk-lush-workspace`: Clarify that workspace path dependencies are the
  steady-state LushText integration unless a later publication change is
  explicitly approved.
- `gtk-lush-adoption-validation`: Clarify that the archived adoption evidence
  becomes maintained baseline evidence for internal stewardship, not a mandate
  to publish.

## Impact

- Affected documentation includes `docs/next/gtk-lush.md`,
  `crates/gtk-lush/README.md`, `crates/gtk-lush/GOVERNANCE.md`,
  `docs/gtk-lush-adoption/archive-handoff.md`, and any local guidance that
  still points agents toward publication as the automatic next step.
- Affected specs are GTK Lush governance, workspace, adoption validation, and
  the new internal-platform capability.
- Affected checks include OpenSpec validation, GTK Lush policy/adoption gates,
  agent documentation checks, and the normal `make check` path.
- Application runtime behavior, public GTK Lush APIs, LushText features,
  persisted data, automation contracts, and visual design are not intended to
  change.
