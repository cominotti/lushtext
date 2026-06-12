# Specialist Review Notes

Date: 2026-06-12

Status: complete. Actionable findings were fixed before the OpenSpec tasks
were marked complete.

## Lanes

- GTK testing: review `crates/gtk-lush-adoption-lab` tests,
  `fixtures/gtk-lush-adoption/stock-settle`, and headless-harness usage.
- GTK runtime/contracts: review lab allocation, viewport, RenderHoldOverlay,
  focus, and not-ready behavior.
- Performance: review lab worker usage, CI cost, policy script cost, and
  avoidance of large committed fixtures.
- Data safety/privacy: review adoption journals, ignored external checkouts,
  generated artifact roots, and absence of private user content.
- Rust architecture: review adoption-lab placement, family leaf policy, CQS,
  ownership, and consumer-vs-family boundaries.
- Comments/readability: review public examples, docs, and non-obvious GTK or
  proof-tool wording.

## Current Findings

- GTK testing/runtime: blocked archive until the adoption surface had mapped
  GTK coverage beyond compile-only lab tests. Fixed by adding
  `crates/lushtext/tests/widget/gtk_lush_adoption.rs`, which runs through the
  private headless harness and covers mapped `RenderHoldOverlay` capture,
  non-targetable cover behavior, `ClipBin` constrained width without a root
  horizontal scrollbar, real `TextView` scrollable adjustments observed by
  `ViewportObserver`, and `gtk-lush-tasks` main-loop completion. Focused proof:
  `cargo test -p lushtext --test widget gtk_lush_adoption`; broad proof:
  `make test-widget-headless`.
- GTK testing/runtime: proof-harness lab originally demonstrated the values
  but did not execute through LushText's registered harness. Accepted with the
  new widget module because the project-level harness registers and runs the
  GTK Lush adoption checks as ordinary non-LushText widget flows.
- Performance/responsiveness: stock fixture initially used its own fixture
  target directory, duplicating build artifacts. Fixed by running fixture
  checks with the repository `target/` through `make gtk-lush-stock-fixtures`.
- Performance/responsiveness: lab task buttons could enqueue repeated workers.
  Fixed with a local pending-work guard that collapses extra worker and
  panic-safe requests while one job is in flight.
- Performance/responsiveness: CI intentionally runs the adoption-lab target
  explicitly even though workspace tests also cover it. Accepted as a small
  duplicated cost because it keeps the phase evidence discoverable.
- Data safety/privacy: temporary external checkouts and agent worktrees needed
  stronger protection from accidental staging. Fixed by moving the approved
  external checkout root to ignored `build/gtk-lush-adoption/`, ignoring
  `/.claude/worktrees/`, removing the docs-local external checkout residue, and
  teaching `scripts/check-gtk-lush-adoption.py` to reject docs-local residue.
- Visual proof policy: the new widget adoption test is visual-sensitive, so
  `make visual-geometry-smoke` refreshed `build/smoke/visual-geometry/summary.json`
  and `make check-visual-proof-policy` verified the digest against the current
  diff.
- Data safety/privacy: adoption journals remain bounded command/friction
  summaries. They do not include document bodies, private user content,
  screenshots, frame streams, or copied external source trees.
- Rust architecture: the adoption lab must stay a consumer, not a family crate
  or LushText crate. Fixed by checking that the lab lives outside
  `crates/gtk-lush/`, is unpublished, and has no LushText crate dependencies.
- Comments/readability: stale proof-tool wording still described Rust proof as
  future or Python-authoritative. Fixed in `cargo-gtk-proof` README, module
  docs, comments, and canonical OpenSpec wording, while preserving
  `rust-staged` only as historical compatibility metadata.
- Comments/readability: workspace and family docs now describe Phase 5a as
  adoption validation and Phase 5b as future publication, so this phase does
  not imply crates.io release readiness.
