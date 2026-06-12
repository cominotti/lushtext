## 1. Preflight And Audit Setup

- [x] 1.1 Read `docs/next/gtk-lush.md`, the Phase 2 GTK Lush specs, this change's `proposal.md`, `design.md`, and all delta specs before editing implementation files.
- [x] 1.2 Record baseline `git status --short`, `openspec status --change extract-gtk-lush-runtime-geometry --json`, and the current `spawn_blocking_then` caller count.
- [x] 1.3 Create a Phase 3 audit note under the change directory for task call-site classifications, viewport/anchor classifications, widget/render-hold classifications, retained explicit sites, and review outcomes.
- [x] 1.4 Audit every `spawn_blocking_then` caller and classify it as crate-owned dispatch, crate-owned dispatch plus domain-retained freshness, persistence-order retained, visual/readiness retained, or intentionally explicit.
- [x] 1.5 Audit `editor_page/overscroll.rs` and adjacent minimap/editor geometry code for page-size observers, value observers, rest-state tracking, edge clamps, EOF overscroll, focus-mode refresh, and user-scroll reveal behavior.
- [x] 1.6 Audit `LushtextShrinkableBin`, resource/template references, generated UI references, and status-bar/minimum-height tests that define the clipping contract.
- [x] 1.7 Audit minimap reflow-freeze state, source-map opacity transitions, cover picture lifecycle, warm-under-cover, early reveal, automation-visible state, and visual-geometry scenarios touched by render-hold migration.
- [x] 1.8 Identify all docs and rules that must change, including `crates/gtk-lush/README.md`, `crates/gtk-lush/GOVERNANCE.md`, `docs/next/gtk-lush.md`, root README or AGENTS references, `.agents/rules/*.md`, and automation docs if exposed fields change.

## 2. Workspace And Policy Scaffolding

- [x] 2.1 Add `crates/gtk-lush/tasks`, `crates/gtk-lush/viewport`, and `crates/gtk-lush/widgets` with `Cargo.toml`, `src/lib.rs`, `README.md`, `CHANGELOG.md`, examples, tests, SPDX headers, `#![forbid(unsafe_code)]`, and `#![deny(missing_docs)]`.
- [x] 2.2 Wire the three crates into the root Cargo workspace, workspace dependency table, `crates/lushtext-core/Cargo.toml` as needed, and generated lockfile state.
- [x] 2.3 Update `Makefile` GTK Lush package and crate lists so doctests, examples, MSRV checks, and advisory API checks include the Phase 3 crates.
- [x] 2.4 Update `scripts/check-gtk-lush-policy.py` so Phase 3 crates are required, named examples under `examples/*.rs` are accepted, runtime inter-crate GTK Lush dependencies are rejected, and functional pre-publication crates are not treated as crates.io placeholders.
- [x] 2.5 Regenerate `workspace-hack` with `cargo hakari generate` after dependency changes and inspect the diff for unintended feature churn.
- [x] 2.6 Update dependency policy configuration if the new crates require allowlisted dev dependencies, keeping runtime dependencies minimal and leaf-safe.
- [x] 2.7 Add or update public-API snapshot/advisory outputs only through the established `make gtk-lush-api-advisory` workflow.
- [x] 2.8 Run `make check-gtk-lush-policy` after scaffolding and fix all policy failures before migrating LushText.

## 3. gtk-lush-tasks Implementation

- [x] 3.1 Implement the bounded worker-slot state machine with atomic acquisition, saturated backpressure through the GLib main loop, and panic-safe slot release.
- [x] 3.2 Implement the main-thread completion helper using GLib dispatch and `glib::thread_guard::ThreadGuard` or an equivalent documented boundary for non-`Send` GTK-thread state.
- [x] 3.3 Implement typed completion or freshness helpers that make current-generation or current-identity application explicit without owning app-specific domain rules.
- [x] 3.4 Split deterministic task/freshness decisions into pure logic that can be unit-tested without a GTK window.
- [x] 3.5 Add task crate unit tests for worker limit, saturated backpressure, slot release on success, slot release on panic, main-thread completion, dead-target or dropped-state behavior where supported, and stale-token rejection.
- [x] 3.6 Add doctests and at least one stock gtk-rs example showing single-crate adoption, worker dispatch, main-thread completion, and a stale-result guard.
- [x] 3.7 Document in the README when to use `gtk-lush-tasks`, when to keep domain freshness explicit, and why the crate does not create a runtime or message loop.
- [x] 3.8 Run `cargo test -p gtk-lush-tasks`, `make gtk-lush-doctests`, and `make gtk-lush-examples` for the task crate work before app migration.

## 4. LushText Task Migration

- [x] 4.1 Replace fitting `services::async_task::spawn_blocking_then` imports and call sites with `gtk-lush-tasks` in small module batches.
- [x] 4.2 Preserve GTK-thread snapshot boundaries for editor text, widget state, file paths, selected rows, visible search state, and other GTK-owned inputs before worker scheduling.
- [x] 4.3 Preserve domain freshness checks for tab identity, path identity, search generations, encoding requests, undo generations, persistence ordering, and visual/readiness state where the new crate cannot own the rule.
- [x] 4.4 Preserve persistence latest-state-wins behavior for session, workspace, drafts, sidecars, local history, Replace All undo backup, search history, and saved-search flows.
- [x] 4.5 Preserve slow-filesystem backpressure and widget-test main-loop wait behavior for task completions.
- [x] 4.6 Delete `services::async_task` or reduce it to documented compatibility glue with no duplicate dispatcher for fitting sites.
- [x] 4.7 Update focused unit, integration, or widget tests for migrated task-backed workflows where existing tests do not cover stale completion or persistence ordering.
- [x] 4.8 Update the Phase 3 audit with every retained explicit task/freshness site and the reason it remains outside `gtk-lush-tasks`.

## 5. gtk-lush-viewport Implementation And Migration

- [x] 5.1 Implement `gtk-lush-viewport` observer types for horizontal and vertical adjustment page-size changes, unchanged-dimension filtering, and value-change rest-state updates.
- [x] 5.2 Implement reflow-pause or burst-aware rest-state exclusion so GTK-preserved adjustment values during layout storms do not overwrite user intent.
- [x] 5.3 Expose caller-owned hooks for width/height repair, edge clamps, dynamic overscroll refresh, focus-mode geometry refresh, and early reveal without depending on other GTK Lush crates.
- [x] 5.4 Add viewport crate tests for width-only reflow, height-only reflow, unchanged page size, at-left/at-top tracking, paused reflow state, value-change ordering, and dead-target cleanup.
- [x] 5.5 Add doctests and a stock gtk-rs example showing adjustment-based observation for a `Scrollable` and documenting the layout-manager `size_allocate` trap.
- [x] 5.6 Migrate `editor_page/overscroll.rs` to consume `gtk-lush-viewport` while preserving left-edge clamp, top-edge clamp, minimap reflow scheduling, dynamic EOF overscroll, and focus-mode refresh.
- [x] 5.7 Preserve user-scroll early reveal behavior for minimap render holds during viewport value changes.
- [x] 5.8 Update or add focused widget tests for top-left anchor preservation across sidebar show/hide, passive narrow transitions, and height-affecting layout changes.
- [x] 5.9 Update the Phase 3 audit with retained explicit viewport, idle repair, or app-policy sites and the reason each remains outside `gtk-lush-viewport`.

## 6. ClipBin Implementation And Migration

- [x] 6.1 Implement `gtk-lush-widgets::ClipBin` as a single-child GTK widget with a builder-friendly `child` property, zero minimum size, child natural-size delegation, child allocation, and snapshot clipping.
- [x] 6.2 Add `ClipBin` tests for empty state, populated state, constrained geometry, child replacement, duplicate child assignment, unparent on dispose, baseline behavior, and snapshot clipping.
- [x] 6.3 Add doctests or examples showing `ClipBin` adopted in a stock gtk-rs layout without LushText.
- [x] 6.4 Migrate LushText resources/templates and type registration from `LushtextShrinkableBin` to `ClipBin` or a documented temporary alias.
- [x] 6.5 Delete the app-local shrinkable-bin implementation after migration unless a temporary compatibility alias is required and tracked for removal in the same change.
- [x] 6.6 Verify short-window shell behavior with focused widget tests covering status bar visibility, tab strip visibility, open search panel, side surfaces, minimap, and awkward editor content.
- [x] 6.7 Update the Phase 3 audit with any retained app-local clipping or shell geometry sites and the reason they remain outside `ClipBin`.

## 7. RenderHoldOverlay Implementation And Minimap Migration

- [x] 7.1 Implement `gtk-lush-widgets::RenderHoldOverlay` capture, cover display, live-child opacity pairing, warm-under-cover, reveal, clear, supersede, child-change, and drop cleanup behavior.
- [x] 7.2 Keep `RenderHoldOverlay` scheduling caller-owned; do not add runtime dependencies on `gtk-lush-settle`, `gtk-lush-viewport`, or other GTK Lush crates.
- [x] 7.3 Add render-hold tests for successful capture, failed capture, stale cover prevention, opacity restoration, non-targetable cover, warm-under-cover, early reveal, superseded hold, and drop cleanup.
- [x] 7.4 Add a stock gtk-rs example demonstrating a temporary render hold without LushText-specific minimap or sidebar assumptions.
- [x] 7.5 Migrate minimap reflow freeze state from app-local picture/opacity fields to `RenderHoldOverlay` or a minimal compatibility adapter around it.
- [x] 7.6 Preserve native `GtkSourceMap` highlight rendering, marker layering, read-only behavior, focus behavior, minimap navigation, and final settled source-map geometry.
- [x] 7.7 Preserve warm-under-cover before reveal and early reveal on user scroll, click, drag, or other direct minimap/editor interaction.
- [x] 7.8 Update automation-visible bounded state only if needed to distinguish an intentional in-progress hold from a stuck invisible source map.
- [x] 7.9 Update or add widget tests for minimap render-hold lifecycle, source-map opacity restoration, early reveal, tab close/drop cleanup, and rapid sidebar toggles.
- [x] 7.10 Run or extend visual-geometry minimap/sidebar scenarios so pixel-anchor and animation-frame proof covers the migrated render-hold path.
- [x] 7.11 Update the Phase 3 audit with retained minimap rendering, visual proof, or app-owned timing sites and the reason each remains outside `RenderHoldOverlay`.

## 8. Documentation, Governance, And Rule Updates

- [x] 8.1 Update `crates/gtk-lush/README.md` so it accurately distinguishes functional in-tree `0.0.0` APIs from crates.io placeholder reservation releases.
- [x] 8.2 Update `crates/gtk-lush/GOVERNANCE.md` with a dated Phase 3 review entry, constitution checklist, retained exceptions if any, and proof summary.
- [x] 8.3 Update `docs/next/gtk-lush.md` to mark Phase 3 implementation status, preserve future Phase 4 and Phase 5 boundaries, and avoid claiming publication readiness.
- [x] 8.4 Update root README, AGENTS guidance, and `.agents/rules/*.md` references so new fitting work points to `gtk-lush-tasks`, `gtk-lush-viewport`, and `gtk-lush-widgets` instead of app-local copies.
- [x] 8.5 Update automation docs and run `make check-automation-docs` plus `make automation-client-self-test` if any D-Bus member, readiness blocker, snapshot field, helper flag, or visual-geometry artifact contract changes.
- [x] 8.6 Update comments and public docs so non-obvious GTK, GLib, ThreadGuard, viewport, custom widget, and render-hold invariants explain why the shape exists.
- [x] 8.7 Ensure all new public Rust items have documentation and observable examples or tests before running lint gates.

## 9. Focused Reviews And Fix Passes

- [x] 9.1 Run a data-safety review for task-backed persistence, save-adjacent flows, Replace All undo backup, drafts, session/workspace persistence, and stale completion handling; fix actionable findings.
- [x] 9.2 Run a GTK responsiveness/performance review for worker scheduling, main-thread callbacks, viewport observers, widget snapshots, minimap frame-path work, and large-file/search flows; fix actionable findings.
- [x] 9.3 Run a GTK/Libadwaita internals review for adjustment observation, custom widget measurement/allocation/snapshot behavior, overlay parenting, source-map capture, and warning-clean geometry; fix actionable findings.
- [x] 9.4 Run a Rust architecture review for crate boundaries, leaf dependency rules, app-vs-crate ownership, CQS/domain freshness placement, and compatibility glue; fix actionable findings.
- [x] 9.5 Run a Rust comments/documentation review for changed Rust code, public APIs, complex GTK invariants, and retained-site audits; fix actionable findings.
- [x] 9.6 Re-run focused tests after every review fix that touches task, viewport, clipping, render-hold, persistence, or visual-sensitive code.
- [x] 9.7 Record review summaries, accepted findings, fixed findings, and any maintainer-approved retained exceptions in the Phase 3 audit note or GOVERNANCE entry.

## 10. Verification Gates

- [x] 10.1 Run `cargo fmt --check` and fix formatting drift.
- [x] 10.2 Run `make check-clippy` or the repo's all-targets/all-features Clippy gate and fix warnings.
- [x] 10.3 Run `make check-gtk-lush-policy` and fix family policy failures.
- [x] 10.4 Run `make gtk-lush-doctests` and `make gtk-lush-examples` and fix documentation or example failures.
- [x] 10.5 Run `make gtk-lush-msrv` and fix MSRV regressions.
- [x] 10.6 Run `make gtk-lush-api-advisory` and review semver/public-API output for unintended API drift.
- [x] 10.7 Run `cargo nextest run --workspace` and fix non-widget test failures.
- [x] 10.8 Run `make test-widget-headless` and fix widget or GTK warning failures.
- [x] 10.9 Run `make visual-geometry-smoke SMOKE_ARTIFACT_DIR=build/smoke` for minimap/sidebar pixel-anchor and animation-stream coverage; preserve unsupported-host reasons if the host cannot run it, but do not count unsupported coverage as verified.
- [x] 10.10 Run `make check-visual-proof-policy` and fix missing or stale visual proof evidence.
- [x] 10.11 Run `make check-policy` and fix policy failures, including filesystem-boundary, Blueprint, automation docs, Flatpak permission, end-user smoke workflow, visual proof, GTK Lush policy, and automation client gates.
- [x] 10.12 Run `cargo deny check advisories bans sources licenses` if it is not already covered by the selected aggregate gate.
- [x] 10.13 Run `make check` as the final aggregate repo gate and fix all failures.
- [x] 10.14 Run `openspec validate extract-gtk-lush-runtime-geometry --strict`.
- [x] 10.15 Run `openspec validate --changes --strict`.
- [x] 10.16 Run `openspec validate --specs --strict`.
- [x] 10.17 Run `openspec validate --all --strict`.
- [x] 10.18 Run `git diff --check`.
- [x] 10.19 Confirm `openspec status --change extract-gtk-lush-runtime-geometry` reports all tasks complete and archive-ready only after implementation, docs, reviews, and proof gates are done.
