## 1. Baseline Inventory

- [x] 1.1 Capture the current blocking gate commands and confirm `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, rustdoc linting, `cargo deny check advisories bans sources`, and `scripts/check-filesystem-boundary.sh` starting status.
- [x] 1.2 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` and record any all-feature-only failures before changing the gate.
- [x] 1.3 Run advisory Clippy probes for `pedantic`, `nursery`, and `cargo`, then summarize counts by lint code and first location.
- [x] 1.4 Run targeted Clippy probes for high-signal lint candidates: `manual_midpoint`, `unchecked_time_subtraction`, `case_sensitive_file_extension_comparisons`, `significant_drop_tightening`, `needless_collect`, `redundant_clone`, `derive_partial_eq_without_eq`, and `wildcard_imports`.
- [x] 1.5 Run targeted Clippy probes for design-smell candidates: `cognitive_complexity`, `too_many_lines`, `too_many_arguments`, `type_complexity`, boolean excess, `implicit_hasher`, `multiple_crate_versions`, print stdout/stderr, panic/expect, and indexing/slicing.
- [x] 1.6 Run rustc advisory probes through `RUSTFLAGS` for future compatibility, Edition 2024 compatibility, `unused_qualifications`, `unreachable_pub`, `unused_crate_dependencies`, `missing_debug_implementations`, `missing_docs`, and `unsafe_code`.
- [x] 1.7 Run `cargo deny check licenses` and record all missing license metadata, rejected licenses, and duplicate-version findings that must be resolved.
- [x] 1.8 Inspect CI workflows for unpinned Rust validation helper installs, including cargo-deny, cargo-nextest, cargo-fuzz, cargo-mutants, and any actionlint setup.

## 2. Blocking Gate Alignment

- [x] 2.1 Update Makefile `check-clippy` to run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 2.2 Update `.github/workflows/ci.yml` Clippy step to match the all-targets/all-features local command.
- [x] 2.3 Confirm `.githooks/pre-commit` continues to invoke the updated `make pre-commit` path without duplicating stale command text.
- [x] 2.4 Add or update a fast policy-audit Makefile target for `scripts/check-filesystem-boundary.sh` if one does not already exist.
- [x] 2.5 Add the filesystem-boundary audit to CI or to a documented aggregate lint/policy target that CI runs.
- [x] 2.6 Ensure rustdoc linting remains in CI and documented as part of the lint policy.

## 3. Curated Clippy Cleanup And Promotion

- [x] 3.1 Fix every `clippy::manual_midpoint` finding and add the lint to `[workspace.lints.clippy]`.
- [x] 3.2 Fix every `clippy::unchecked_time_subtraction` finding and add the lint to `[workspace.lints.clippy]`.
- [x] 3.3 Fix every `clippy::case_sensitive_file_extension_comparisons` finding and add the lint to `[workspace.lints.clippy]`.
- [x] 3.4 Fix every `clippy::significant_drop_tightening` finding or add narrow `#[expect]` reasons where the current drop span is intentional, then add the lint to `[workspace.lints.clippy]`.
- [x] 3.5 Fix every `clippy::needless_collect` finding or add narrow `#[expect]` reasons where collecting is needed to end a borrow or preserve model stability, then add the lint to `[workspace.lints.clippy]`.
- [x] 3.6 Fix every `clippy::redundant_clone` finding that is not required by GTK object ownership, signal closure lifetime, or test readability; add narrow `#[expect]` reasons for retained clones and decide whether the lint becomes blocking globally or must-stay-zero advisory for production modules.
- [x] 3.7 Fix every `clippy::derive_partial_eq_without_eq` finding and add the lint to `[workspace.lints.clippy]` if the cleaned tree passes.
- [x] 3.8 Fix every `clippy::wildcard_imports` finding or add narrow `#[expect]` reasons for macro-generated or GTK prelude-shaped exceptions, then add the lint to `[workspace.lints.clippy]` if the cleaned tree passes.
- [x] 3.9 Re-run the standard all-features Clippy gate after each promoted-lint batch and keep the workspace green before moving to the next batch.

## 4. Advisory Lint Lane

- [x] 4.1 Add a script or Makefile target such as `make lint-advisory` that runs the selected broad Clippy, targeted Clippy, rustc, and dependency-policy advisory probes.
- [x] 4.2 Make the advisory target emit deterministic grouped output with lint code, count, first file, first line, and first message.
- [x] 4.3 Add a checked-in advisory policy or baseline that classifies every current advisory category as blocking candidate, must-stay-zero advisory, accepted advisory, generated-code noise, or resolved policy exception.
- [x] 4.4 Make the advisory target fail when a new lint category appears without classification.
- [x] 4.5 Classify noisy categories such as `doc_markdown`, `missing_const_for_fn`, `too_many_lines`, generated widget registry findings, and broad restriction findings with rationale rather than leaving them implicit.
- [x] 4.6 Mark categories that should remain zero, such as unexpected debug output or unclassified unsafe-code inventory, so future regressions are visible.
- [x] 4.7 Document how to refresh the advisory policy after a Rust or Clippy toolchain update.

## 5. rustc And Visibility Cleanup

- [x] 5.1 Clean `unused_qualifications` findings where shortening paths improves readability.
- [x] 5.2 Classify retained qualified paths with reasons such as GTK trait disambiguation, macro clarity, or generated-code constraints.
- [x] 5.3 Narrow `unreachable_pub` findings where the item is not part of an externally reachable, test-facing, macro-required, or GTK subclass-facing API.
- [x] 5.4 Classify retained unreachable public items with owning-module rationale.
- [x] 5.5 Remove real unused direct dependencies reported by `unused_crate_dependencies`.
- [x] 5.6 Classify `workspace-hack`, cargo-hakari, build, dev, bench, and generated-crate dependency noise so it is not mistaken for real unused dependencies.
- [x] 5.7 Ensure future-compatibility and Edition 2024 rustc probes remain clean under Rust 1.96.0.

## 6. clippy.toml And Project-Specific Policy

- [x] 6.1 Evaluate whether any globally safe `disallowed-methods` or `disallowed-types` entries exist for LushText.
- [x] 6.2 If globally safe bans exist, add `clippy.toml` entries with reason and replacement metadata and verify the blocking Clippy gate passes.
- [x] 6.3 If no globally safe bans exist, document that decision in lint-policy guidance and do not create an empty or misleading `clippy.toml`.
- [x] 6.4 Keep path-sensitive filesystem-boundary enforcement in `scripts/check-filesystem-boundary.sh` and verify the script still covers raw filesystem, backend, durable-write, status-probe, engine-adapter, and leftover dependency drift.
- [x] 6.5 Extend the filesystem-boundary audit only if the lint-hardening work uncovers a policy gap not already covered by the script.

## 7. cargo-deny Dependency Policy

- [x] 7.1 Add license metadata for `workspace-hack` or configure cargo-deny so the generated package is intentionally licensed.
- [x] 7.2 Add a `[licenses]` policy in `deny.toml` with a GPL-compatible allow-list based on the actual dependency graph.
- [x] 7.3 Replace the CI dependency-policy command with `cargo deny check advisories bans sources licenses`.
- [x] 7.4 Change duplicate-version policy toward deny-by-default where practical.
- [x] 7.5 Remove avoidable duplicate versions through dependency updates when safe.
- [x] 7.6 Add narrow `skip` or `skip-tree` entries with reasons for unavoidable foundational duplicate versions such as platform crates.
- [x] 7.7 Revisit `[bans.workspace-dependencies]` duplicate and unused settings so they are intentional around cargo-hakari rather than broad unexamined allowances.
- [x] 7.8 Run the full cargo-deny policy and keep advisories, bans, sources, and licenses green.

## 8. CI Tool Pinning

- [x] 8.1 Pin cargo-deny installation to an exact version in CI or switch to a pinned action/version that runs the same policy.
- [x] 8.2 Pin cargo-nextest installation to an exact version in every workflow that installs it.
- [x] 8.3 Pin cargo-fuzz installation or document the exact nightly/tooling exception if an exact pin is not practical.
- [x] 8.4 Confirm cargo-mutants remains pinned and document the expected version in guidance.
- [x] 8.5 Centralize CI validation tool versions in workflow env variables or another obvious location where practical.
- [x] 8.6 Run `actionlint` on every changed GitHub workflow.

## 9. Documentation And Agent Guidance

- [x] 9.1 Update README validation sections to describe all-feature Clippy, rustdoc, cargo-deny with licenses, filesystem-boundary audit, advisory linting, and tool pinning.
- [x] 9.2 Update root AGENTS.md with the new Rust linting policy, command list, and any changed rules-index entries.
- [x] 9.3 Update `.agents/rules/build.md` with the aligned local and CI validation commands.
- [x] 9.4 Update `.agents/rules/rust.md` with curated lint promotion rules, broad-group advisory policy, `#[expect(..., reason = "...")]` exception style, and clippy.toml guidance.
- [x] 9.5 Update any nested AGENTS.md files affected by lint policy or filesystem-boundary gate changes.
- [x] 9.6 Update any Rust review or performance skills that mention the old Clippy command, old tool pins, or incomplete lint policy.
- [x] 9.7 Run `make check-agent-docs` after guidance changes.

## 10. Source Validation

- [x] 10.1 Run `cargo fmt --all -- --check`.
- [x] 10.2 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 10.3 Run the rustdoc lint gate with broken intra-doc links, private intra-doc links, and bare URLs denied.
- [x] 10.4 Run `scripts/check-filesystem-boundary.sh`.
- [x] 10.5 Run `cargo deny check advisories bans sources licenses`.
- [x] 10.6 Run the advisory lint target and confirm no unclassified findings remain.
- [x] 10.7 Run relevant tests for Rust cleanup that changes behavior, including package-specific unit/integration/property/widget or benchmark-compile checks as needed.
- [x] 10.8 Run `actionlint` for changed workflows.
- [x] 10.9 Run `openspec validate --all --strict`.
- [x] 10.10 Run `git diff --check`.

## 11. Completion Audit

- [x] 11.1 Re-run `openspec status --change "harden-rust-linting-policy" --json` and verify all tasks are complete.
- [x] 11.2 Confirm no untracked generated lint reports, temporary advisory outputs, or accidental lockfile changes remain unless intentionally committed.
- [x] 11.3 Summarize final promoted lints, advisory categories, cargo-deny policy changes, tool pins, and validation evidence for review.
