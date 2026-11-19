# Proposal

## Why

The refactor-1 code-quality audit (its disposition, with every measured fact and decision, is `docs/next/refactor-1-disposition.md`) found that the local gate and CI disagree, that one shell subscription leaks, and that a durable-write error drops its cause. The figures below were measured on `origin/main` `761866ff` on 2026-11-18. The change starts only after the programme's queued work has finished, so task 0.1 re-measures every one of them first.

- **Widget tests are not format-checked in CI.** `.github/workflows/ci.yml` runs `cargo fmt --all -- --check`, which never reaches the 42 files of `crates/lushtext/tests/widget/`; their modules come from a registry `build.rs` generates. `make check-fmt` (`Makefile:844-848`) does reach them.
- **Most policy audits never run in CI.**
  - `check-policy` has 33 prerequisites (`Makefile:1054`). The CI `Lint` job runs 8 of them as `make <target>` steps.
  - Two more reach CI through their scripts: `install-cargo-public-api.sh --self-test` and `widget-shards.py github-outputs`.
  - The other 23 run only through the local hook.
- **`workspace-hack` drift is not checked, and three manifests disagree with the rules.** Baseline with cargo-hakari 0.9.37 on the local `main`:
  - `generate --diff` exits 0, `verify` exits 0, and `manage-deps --dry-run` exits 1;
  - ten crates lack the dependency, contrary to `.agents/rules/rust.md:12`.
  
  The investigation of 2026-11-19 (findings in `docs/next/refactor-1-disposition.md`, "workspace-hack") found two cases:
  - **The eight GTK Lush family crates are rightly excluded, but the exclusion is not recorded.** None ever had the dependency, and adding it breaks `make gtk-lush-stock-fixtures` under `--locked`, taking the stock consumer from 84 to 143 packages.
  - **`cargo-gtk-proof` lacks it by omission.**
  
  `gtk-lush-adoption-lab` is excluded too, because `docs/gtk-lush-adoption/matrix.toml:77` cites its one-dependency graph as adoption evidence.
- **`proof-strength.py --self-test` runs twice** in every `make check-policy` (`Makefile:460`, `:570`).
- **Each closed window leaves two handlers on the process-wide settings object.**
  - `ui/window/imp.rs:853` drops both ids that `workspace_visibility::connect_visibility_changed` returns.
  - The settings object is `settings_writes::shared()`, which outlives every window.
  - `sidebar/imp.rs:211` tracks the same pair in a `SignalBag`.
  - `check-settings-writes.py` allows `connect_changed` by design, so no gate sees it.
- **`DurableWriteError` has no cause chain.** It implements `std::error::Error` without `source()`, and its `Display` repeats the inner I/O error, so neither the phase (before or after the rename) nor the cause survives into a chain.
  - It is rendered: `search_backup.rs:1805` returns it through `?` into an `anyhow::Result`, and `draft_service.rs:2082` wraps it with `.with_context`. A new `Display` therefore changes message text (design D6).
- **The print test hook can leak.** `ui/window/print/test_policy.rs:27-37` restores the previous runner after `f()` without a `Drop` guard.
- **Two production comments cite evidence in the gitignored `.claude/handoffs/`:** `model/protected_fold.rs:74` and `services/content_search/replace/sr_measure.rs:8`.

## What Changes

- **Same format check in CI.** CI runs `make check-fmt`.
- **One aggregate CI policy target.**
  - Every `check-policy` prerequisite is on exactly one of three lists: `check-policy-ci` (run by the `Lint` job), covered by another named CI job, or local-only with a reason.
  - A self-tested script enforces that.
  - `check-policy` lists one prerequisite per line, if the coordinator moves that item here from `smoke-gate-coherence`.
- **`workspace-hack`:**
  - the family's eight crates and the adoption lab go into `[final-excludes]` of `.config/hakari.toml`, which leaves the generated crate unchanged;
  - `cargo-gtk-proof` gains the dependency;
  - `rust.md` and `build.md` say so;
  - CI runs a pinned `cargo hakari generate --diff`, `verify` and `manage-deps --dry-run` when dependencies change;
  - `check-gtk-lush-policy` checks that the exclusion list equals the family's package list.
- **One self-test run.** `proof-strength.py --self-test` runs once.
- **Owned subscriptions.** `connect_visibility_changed` takes its owner and disconnects with it. `check-settings-writes` fails on a discarded handler id of a two-argument settings `connect_changed(Some(…))`.
- **Typed durable-write errors.** `DurableWriteError` gets a `Display` per phase and `source()` returning the I/O error.
- **Panic-safe print hook.** `with_print_runner_for_test` restores the previous runner through a `Drop` guard.
- **Committed evidence.** The C2 sweep table and `main`'s side of the M9 measurement are committed, and the two comments cite them.
- **Disposition record.** `docs/next/refactor-1-disposition.md` records, for each audit item, the change that takes it or the reason it is declined. It is committed with this plan; task 5.1 updates it with the re-measured figures.

## Non-goals

- **The other 49 `.claude/handoffs/` references in `docs/`.** Their audit is its own change.
- **Moving work into or out of the pre-commit hook.** `smoke-gate-coherence` owns the smoke demand.
- **The audit's other proposals.** They have their own changes (`spike-list-row-construction`, `adopt-typed-row-state-and-dialog-futures`, `adopt-glib-clone-weak-captures`, `type-cargo-gtk-proof-errors`, `fix-settle-burst-sources-and-weak-refs`), or the disposition record gives their reason.

## User-visible impact

In the application, an error message that renders a durable-write failure's full chain gains the failed phase next to the I/O cause (design D6). Otherwise none. Contributors see CI fail on the same policy findings the local hook already rejects. Nothing is lost compared with `main`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rust-linting-policy`:
  - CI runs every fast policy audit or names it as covered elsewhere or local-only;
  - the CI format check covers every source the local gate formats;
  - CI fails on `workspace-hack` drift.
- `gtk-lush-signals`: LushText subscriptions to the shared application settings are owned.
- `gtk-lush-workspace`: the family counts for feature unification but does not depend on `workspace-hack`, and the exclusion is recorded and checked.
- `durable-file-write-contract`: a durable-write failure names its phase and exposes its I/O cause.

## Impact

- **Code:**
  - `Makefile`, `.github/workflows/ci.yml`, `.config/hakari.toml`;
  - `crates/cargo-gtk-proof/Cargo.toml`, `Cargo.lock`;
  - `scripts/check-settings-writes.py`, `scripts/check-gtk-lush-policy.py`, a new `scripts/check-policy-ci.py`;
  - `ui/workspace_visibility.rs`, `ui/window/imp.rs`, `ui/sidebar/imp.rs`, `ui/window/print/test_policy.rs`;
  - `services/durable_write.rs`, comments in `model/protected_fold.rs` and `services/content_search/replace/sr_measure.rs`.
- **Docs:**
  - `docs/next/refactor-1-disposition.md`, `docs/benchmarks/c2-head-sweep.md`, `docs/next/long-line-documents.md`;
  - `.agents/rules/{rust,build,ui,widget-wiring}.md`, `AGENTS.md`, `README.md` if it lists CI gates.
- **Order:** see the Start line in `tasks.md`. `services/durable_write.rs` is also edited by `verify-shell-conformance-against-proven-cores`, which must have merged first.
- **Shared with the other refactor-1 changes:** `crates/cargo-gtk-proof/Cargo.toml` (with `type-cargo-gtk-proof-errors`), `ui/window/imp.rs` (with `adopt-glib-clone-weak-captures`), and `check-policy` / `check-policy-ci` (`adopt-glib-clone-weak-captures` adds an audit to them). This change lands first.
