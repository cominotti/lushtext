## 1. Workspace And Governance Setup

- [x] 1.1 Create `crates/gtk-lush/proof-harness` as `gtk-lush-proof-harness` with `0.0.0` version, workspace rust-version/lints, `MIT OR Apache-2.0` metadata, README, CHANGELOG, SPDX headers, crate docs, `#![forbid(unsafe_code)]`, and `#![deny(missing_docs)]`.
- [x] 1.2 Create `crates/gtk-lush/proof-spine` as `gtk-lush-proof-spine` with `0.0.0` version, workspace rust-version/lints, `MIT OR Apache-2.0` metadata, README, CHANGELOG, SPDX headers, crate docs, `#![forbid(unsafe_code)]`, and `#![deny(missing_docs)]`.
- [x] 1.3 Create `crates/cargo-gtk-proof` as a non-published workspace cargo tool outside `crates/gtk-lush/`, with crate docs, CLI docs, tests, and no family-crate policy exception.
- [x] 1.4 Add the proof crates and cargo tool to the root workspace and workspace dependencies where appropriate.
- [x] 1.5 Regenerate cargo-hakari `workspace-hack` and Flatpak `build-aux/cargo-sources.json` for the new dependencies.
- [x] 1.6 Update Makefile `GTK_LUSH_PACKAGES` and `GTK_LUSH_CRATES` so proof crates participate in doctest, example, MSRV, and advisory lanes.
- [x] 1.7 Update `scripts/check-gtk-lush-policy.py` so the proof family crates are enforced as leaves and `cargo-gtk-proof` is recognized as a workspace tool outside the family.
- [x] 1.8 Add ignore coverage for generated proof scratch/artifact roots while keeping checked-in compatibility fixtures bounded.
- [x] 1.9 Add a dated `extract-gtk-lush-proof-toolchain` governance review entry covering constitution conformance, leaf-crate status, cargo tool placement, Automation1 drift evidence, current visual parity evidence, and the Phase 5 boundary.

## 2. Proof Harness Crate And LushText Widget Migration

- [x] 2.1 Implement the public `gtk-lush-proof-harness` API around harness configuration, recommended pre-GTK environment settings, test registration, child process execution, retry policy, monitor configuration, and result reporting.
- [x] 2.2 Implement private headless launch through `dbus-run-session` and `mutter --headless --wayland --no-x11 --virtual-monitor`, with isolated `XDG_RUNTIME_DIR`, removed live-display variables, caller-owned monitor environment names, and stable unsupported-host exit behavior.
- [x] 2.3 Implement list mode, terse list output, name filters, `--exact`, `--skip`, per-test subprocess isolation, bounded retries, warning-safe flake reporting, and stable failure exit behavior.
- [x] 2.4 Implement `flush_events`, `flush_after_delay`, `present_window`, and `wait_until` with drain-all GLib main-loop semantics.
- [x] 2.5 Add unit tests for filter matching, environment recommendations, headless tooling detection, unsupported-host exit classification, warning-safe flake text, and low-priority GLib idle completion.
- [x] 2.6 Add a stock gtk-rs adoption example that uses the harness without importing LushText.
- [x] 2.7 Refactor `crates/lushtext/tests/widget.rs` and `crates/lushtext/tests/widget/common.rs` so generic harness behavior comes from `gtk-lush-proof-harness`.
- [x] 2.8 Keep LushText-specific GTK initialization, GResource registration, GSettings/data-dir isolation, application construction, filesystem fixtures, and widget registry integration in LushText adapter code.
- [x] 2.9 Preserve `scripts/run-widget-tests.sh`, `make test-widget`, and `make test-widget-headless` command shape, warning filtering, log capture, retry behavior, and monitor compatibility environment.
- [x] 2.10 Verify `make test-widget-headless` or record the exact unsupported-host/runtime reason if host tooling prevents it.

## 3. Proof Spine And Automation1 Adapter

- [x] 3.1 Implement `gtk-lush-proof-spine` value objects and traits for schema/interface versions, readiness predicates/results, blockers, workflow events, snapshot envelopes, visual surface summaries, artifact result envelopes, status vocabulary, and privacy classification.
- [x] 3.2 Add serde/unit coverage for proof-spine status/result/workflow/snapshot/artifact value objects and a non-LushText fake provider example.
- [x] 3.3 Map LushText Automation1 readiness state into proof-spine readiness results without renaming documented predicates or statuses.
- [x] 3.4 Map LushText Automation1 workflow events into proof-spine event objects while preserving workflow IDs, phases, statuses, ordering, and bounded summaries.
- [x] 3.5 Map bounded visual-surface snapshot data into proof-spine snapshot envelopes without broadening Automation1 privacy exposure.
- [x] 3.6 Add model-level conversion tests and an Automation1 introspection golden test that detects unintended D-Bus member or signature drift.
- [x] 3.7 Update `docs/automation.md` and `docs/automation-reference.md` to explain the proof-spine backing while keeping LushText Automation1 app-specific.
- [x] 3.8 Verify `make check-automation-docs`.
- [x] 3.9 Verify `make automation-client-self-test` or record why live host/tooling prevents it.

## 4. Cargo GTK Proof Tool

- [x] 4.1 Implement `cargo-gtk-proof` CLI parsing with stable `run`, `schema`, `summarize`, `corpus`, and `policy` subcommands.
- [x] 4.2 Emit stable JSON result envelopes for success, failure, usage error, unsupported host, artifact error, unsupported schema version, malformed field, and policy failure.
- [x] 4.3 Define versioned Rust data models for the current proof schema surface: visual scenario manifests, expanded cases, comparison reports, animation reports, root summaries, proof-policy metadata, and artifact envelopes.
- [x] 4.4 Publish machine-readable schema descriptors under `crates/cargo-gtk-proof/schemas/` and document them in `docs/gtk-proof-schemas.md`.
- [x] 4.5 Implement schema validation with clear `unsupported-schema-version` and `malformed-field` statuses, including coverage for current visual-geometry manifests.
- [x] 4.6 Port the pure PNG proof primitives needed for the current corpus: PNG chunk read/write, unfiltering, crop/diff helpers, masks, neutral/background detection, minimap/content detectors, viewport-highlight detection, and pixel-anchor checks.
- [x] 4.7 Add a bounded checked-in compatibility corpus plus embedded PNG corpus replay for pass/fail status fixtures, exact/masked comparison, synthetic minimap highlight, and drift regression cases.
- [x] 4.8 Port visual proof-policy path classification, required invariant mapping, changed-file fingerprinting, root summary validation, current-fingerprint checks, required invariant coverage checks, animation evidence checks, and negative self-tests.
- [x] 4.9 Keep `cargo gtk-proof run` reserved and non-authoritative, returning a stable non-coverage envelope until Rust live-runner parity is implemented in a later phase.
- [x] 4.10 Document CLI commands, default paths, schema versions, host/live-runner boundary, result statuses, artifact layout, and privacy boundaries.
- [x] 4.11 Verify `cargo test -p cargo-gtk-proof --lib --bins`.
- [x] 4.12 Verify `cargo clippy -p cargo-gtk-proof --tests --examples -- -D warnings`.

## 5. Python Live Runner Boundary And Existing Commands

- [x] 5.1 Keep `scripts/visual-geometry-smoke.py` and `make visual-geometry-smoke` on the existing Python live same-session runner until Rust corpus, live-runner, animation, and wrapper parity are recorded.
- [x] 5.2 Keep `scripts/check-visual-proof-policy.py` as the default local proof-policy wrapper while Rust policy parity is available through `cargo gtk-proof policy`.
- [x] 5.3 Keep `scripts/lushtext-automation.py artifact-summary` stable and document the future delegation boundary instead of changing result envelopes prematurely.
- [x] 5.4 Update `docs/end-user-coverage.md` so visual-geometry smoke describes the Rust proof tool's schema/policy/artifact role without claiming Python live-runner retirement.
- [x] 5.5 Confirm no Phase 5 publishing, second-consumer, timed adoption, repository split, or Phase 6 upstreaming work is included.
- [x] 5.6 Verify `make check-visual-proof-policy`.
- [x] 5.7 Verify `make check-end-user-smoke-workflow`.

## 6. Documentation And Policy Checks

- [x] 6.1 Update `docs/next/gtk-lush.md` to record the Phase 4 result, proof family crate names, cargo tool placement, Python live-runner/reference status, and deferred Phase 5/6 boundaries.
- [x] 6.2 Update `crates/gtk-lush/README.md`, `README.md`, and relevant agent guidance for the new proof crates/tool placement.
- [x] 6.3 Update `crates/gtk-lush/GOVERNANCE.md` with the Phase 4 review entry.
- [x] 6.4 Add `crates/cargo-gtk-proof/README.md` and `docs/gtk-proof-schemas.md`.
- [x] 6.5 Verify `make check-agent-docs`.
- [x] 6.6 Verify `make check-gtk-lush-policy`.
- [x] 6.7 Verify `cargo deny check`.

## 7. OpenSpec And Rust Validation

- [x] 7.1 Run `openspec validate extract-gtk-lush-proof-toolchain --strict`.
- [x] 7.2 Run `openspec validate --changes --strict`.
- [x] 7.3 Run `openspec validate --specs --strict`.
- [x] 7.4 Run `openspec validate --all --strict`.
- [x] 7.5 Run `cargo fmt --all -- --check`.
- [x] 7.6 Run `cargo hakari verify`.
- [x] 7.7 Run `cargo check -p gtk-lush-proof-harness -p gtk-lush-proof-spine -p cargo-gtk-proof -p lushtext --tests --examples`.
- [x] 7.8 Run focused proof/automation tests: `cargo test -p gtk-lush-proof-harness -p gtk-lush-proof-spine -p cargo-gtk-proof --lib --bins --examples`, `cargo test -p lushtext-core model::automation --lib`, and `cargo test -p lushtext-core ui::automation --lib`.
- [x] 7.9 Run focused Clippy for proof crates, cargo tool, `lushtext-core`, and widget harness consumers.
- [x] 7.10 Run `git diff --check`.

## 8. Delegated Review And Phase Notes

- [x] 8.1 Run delegated `gtk-testing` review of the extracted harness API, LushText widget migration, headless runner behavior, flake handling, and wait helper semantics.
- [x] 8.2 Run delegated `gtk-agentic-debugging` review of live headless Mutter behavior, D-Bus/session launch, visual runner artifact boundary, warning logs, and unsupported-host paths.
- [x] 8.3 Run delegated `gtk4-libadwaita-internals` review of GTK/main-loop/widget realization assumptions introduced by the harness or Automation1 snapshot mapping.
- [x] 8.4 Run delegated `gtk-perf-review` review of proof runner runtime cost, PNG/corpus processing, policy checks, CI timing, memory use, and Automation1 waits.
- [x] 8.5 Run delegated `data-safety` review of proof artifacts, automation snapshots, wrapper output, privacy boundaries, generated fixtures, and file I/O paths introduced by the tool.
- [x] 8.6 Run delegated `rust-hex-arch` review of crate boundaries, adapter ownership, command/query separation, domain purity, and the cargo-tool versus family-crate split.
- [x] 8.7 Run delegated `rust-comments` review of public API docs, tricky GTK/main-loop comments, proof-policy comments, and migration comments.
- [x] 8.8 Fix actionable delegated-review findings.
- [x] 8.9 Record accepted residual risks or host-specific skipped checks in archive notes before archive.

## Review Notes

- Delegated reviews covered GTK testing, live headless behavior, GTK/Libadwaita contracts, performance and scale, data safety, architecture, and comments. All blocking or actionable findings were fixed before completion.
- Python remains the authoritative same-session visual runner in this phase. `cargo gtk-proof run` intentionally returns a stable non-coverage envelope until Rust live-runner parity, wrapper parity, corpus breadth, and animation evidence are completed in a later phase.
- The proof harness returns stable unsupported-host results for missing headless tooling. If a host provides `mutter` but the compositor cannot start, the launcher preserves the raw child failure so true test failures remain distinguishable.
- Dedicated proof-tool benchmarks are deferred until before Rust live-runner wiring; this phase bounds corpus JSON/PNG reads, streams changed-file fingerprints, and keeps checked-in fixtures small.
