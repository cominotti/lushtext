## 1. Mutation Configuration

- [x] 1.1 Install or invoke `cargo-mutants` in the development environment and capture the exact supported flags needed by this change.
- [x] 1.2 Add `.cargo/mutants.toml` with the default deterministic mutation scope, nextest runner configuration, timeout policy, and narrow documented exclusions.
- [x] 1.3 Add ignore rules for generated mutation output such as `mutants.out`.
- [x] 1.4 Calibrate the initial scope against model and service code, then add only pure helper-heavy UI modules that produce stable non-widget mutation results.

## 2. Local Developer Workflow

- [x] 2.1 Add local command surfaces for mutation smoke, changed-code mutation, and configured full-scope mutation runs.
- [x] 2.2 Ensure any local in-place mutation command refuses dirty worktrees or runs in a disposable checkout.
- [x] 2.3 Document mutation triage: missed mutants, unviable mutants, timeouts, equivalent mutants, and acceptable exclusion rationale.
- [x] 2.4 Document how mutation testing relates to the existing nextest, widget, benchmark, lint, and dependency-policy gates.

## 3. Pull Request CI

- [x] 3.1 Add a pull-request mutation job that depends on or repeats a passing non-widget nextest baseline before using `--baseline=skip`.
- [x] 3.2 Generate the pull-request diff with full git history and run cargo-mutants against the diff.
- [x] 3.3 Make the pull-request mutation job pass cleanly when there are no mutation-scoped changes or no relevant mutants.
- [x] 3.4 Upload `mutants.out` from the pull-request mutation job whenever it exists.

## 4. Full-Scope CI

- [x] 4.1 Add a scheduled and manual full-scope mutation workflow or job.
- [x] 4.2 Shard full-scope mutation runs with identical arguments except for shard identity.
- [x] 4.3 Ensure full-scope shards run only after the non-widget baseline is proven in the same workflow or through an explicit dependency.
- [x] 4.4 Upload a distinct `mutants.out` artifact for each full-scope shard whenever it exists.
- [x] 4.5 Start the full-scope lane in report-only mode if the initial configured scope has real survivor backlog, then document the ratchet path to blocking.

## 5. Survivor Calibration

- [x] 5.1 Run the mutation smoke command and fix command, config, timeout, or artifact problems.
- [x] 5.2 Run changed-code mutation against a representative local diff or disposable branch to prove the PR command path.
- [x] 5.3 Run the configured full-scope mutation pass or all shards and collect survivor output.
- [x] 5.4 For each high-confidence real survivor in the configured scope, add or tighten tests or extract deterministic logic before considering exclusions.
- [x] 5.5 For equivalent or intentionally out-of-scope survivors, add narrow documented exclusions and verify they do not hide unrelated mutants.

## 6. Validation

- [x] 6.1 Run `cargo fmt --all -- --check`.
- [x] 6.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 6.3 Run `cargo nextest run --workspace`.
- [x] 6.4 Run `scripts/run-widget-tests.sh --headless --retries 1` unless the final implementation only changes CI workflow files and mutation documentation.
- [x] 6.5 Run the mutation smoke command.
- [x] 6.6 Validate the mutation workflow syntax with `actionlint` when available.
- [x] 6.7 Run `openspec validate add-mutation-testing --strict`.
