## Context

LushText currently has a layered test suite:

- Unit and service tests live mostly in `crates/lushtext-core` and run quickly under `cargo nextest`.
- Integration tests live in `crates/lushtext/tests/integration*.rs` and also run without a display server.
- Widget tests live in `crates/lushtext/tests/widget*.rs`, use a custom single-threaded harness, and must run through `scripts/run-widget-tests.sh` so Mutter, D-Bus, renderer settings, retries, and warning filtering stay consistent with CI.

Mutation testing should amplify the deterministic non-widget layers first. The GTK widget layer is already valuable, but it is intentionally shell-managed and warning-gated; using it as the first mutation target would make mutation results slower and noisier than the test-quality signal we want.

`cargo-mutants` is the best-fit tool because it is the established Rust mutation-testing runner, supports `cargo nextest`, supports pull-request diff mutation via `--in-diff`, supports checked-in file filters, emits `mutants.out`, and supports sharded full runs.

## Goals / Non-Goals

**Goals:**

- Add robust mutation testing for LushText's deterministic Rust logic.
- Provide fast changed-code mutation feedback on pull requests.
- Provide scheduled or manual sharded full mutation runs for broader coverage.
- Keep mutation output reviewable through uploaded `mutants.out` artifacts.
- Give developers local commands and triage guidance that match CI.
- Use mutation testing to encourage moving business rules out of GTK adapters and into model/service/helper seams.

**Non-Goals:**

- Do not add runtime application dependencies.
- Do not replace the existing unit, integration, widget, benchmark, lint, or dependency-policy gates.
- Do not make compositor-driven widget mutation a blocking gate in the first implementation.
- Do not treat mutation score as a vanity metric. The useful outcome is fewer real surviving mutants in important code.
- Do not blanket-exclude surviving mutants just to make CI green.

## Decisions

### Use `cargo-mutants` as repo-managed development tooling

LushText will use `cargo-mutants` from CI and local developer commands, but it will not be added as a runtime dependency to the application crates.

Alternatives considered:

- Manual mutation scripts: too much custom logic for source rewriting, timeouts, and reporting.
- Coverage-only tooling: useful, but it proves execution, not assertion strength.
- A wrapper tool with its own kill-rate policy: potentially useful later, but first-class cargo-mutants output is simpler and easier to audit.

### Make `cargo nextest` the mutation test runner

The mutation lanes should run the existing non-widget test surface with `--test-tool nextest`. This matches the current CI path for non-widget tests and avoids trying to route the custom widget harness through cargo-mutants.

Alternatives considered:

- `cargo test --workspace`: slower and less aligned with CI.
- `make test`: includes the widget harness and would make every mutant run create a headless compositor session.
- A custom widget mutation command: possible only after experimentation; not a first blocking gate.

### Start with deterministic mutation scope

The checked-in mutation config should examine `crates/lushtext-core/src/model/**`, `crates/lushtext-core/src/services/**`, and only carefully selected pure helper-heavy UI modules after calibration. Broad GTK adapter files such as `imp.rs`, window/sidebar/search-panel orchestration, the binary entry point, generated code, build scripts, benchmark harnesses, and packaging scripts should be excluded from the default scope.

This keeps the first mutation signal high-value: draft/session recovery, save safety, durable writes, encoding, search/replace safety, file indexing, sidecar identity, Markdown lowering helpers, minimap geometry helpers, and other logic where mutation survivors usually indicate missing assertions or architectural leakage.

### Add two CI lanes

Pull requests should get a changed-code mutation job:

- Fetch full history.
- Generate a diff against the PR base.
- Run normal non-widget tests first or depend on the existing non-widget test job.
- Run `cargo mutants --test-tool nextest --in-diff git.diff --baseline=skip` with explicit timeout settings.
- Upload `mutants.out` on success and failure.

Full mutation should be scheduled and manually dispatchable:

- Run after a non-widget baseline job passes.
- Shard with identical cargo-mutants arguments across all shards.
- Upload one artifact per shard.
- Start as a reporting gate if the initial backlog has survivors, then ratchet toward blocking once the configured scope is clean.

### Treat `--in-place` as CI-only unless guarded

`--in-place` improves CI performance on clean checkouts, especially for sharded runs, but it mutates the working tree while running. CI jobs can use it because their checkouts are disposable. Local Make targets or scripts must either avoid `--in-place` or refuse to use it unless the worktree is clean and the command restores a clean state afterward.

### Require explicit triage for survivors and exclusions

Missed mutants should be resolved by adding tests, tightening assertions, or extracting logic into lower-level seams whenever possible. Equivalent or uninteresting mutants can be excluded only with narrow config and a short reason close to the exclusion. Broad file-level exclusions require stronger justification than targeted regex exclusions.

## Risks / Trade-offs

- Mutation runs may be expensive -> Mitigate with PR diff checks, sharded full runs, cache reuse, and explicit timeouts.
- `--in-diff` can miss mutants outside changed lines -> Mitigate with scheduled/manual full-scope runs.
- Some surviving mutants may be equivalent -> Mitigate with narrow documented exclusions instead of hiding whole modules.
- GTK adapter mutation may be noisy -> Mitigate by keeping widget tests as their current gate and extracting deterministic helper logic into mutation scope.
- Baseline skipping can hide broken tests -> Mitigate by depending on a passing non-widget test job before any `--baseline=skip` mutation command.
- In-place mutation can disturb local worktrees -> Mitigate with clean-tree guards or non-in-place local defaults.

## Migration Plan

1. Add checked-in mutation configuration and local command wrappers.
2. Prove the configured non-widget baseline locally.
3. Run a limited mutation smoke check to validate installer, command, filters, timeouts, and artifact output.
4. Add PR changed-code mutation CI as the first blocking lane once smoke results are stable.
5. Add scheduled/manual sharded full mutation CI and upload per-shard artifacts.
6. Triage initial full-run survivors by adding tests, refactoring seams, or narrowly excluding equivalent mutants.
7. Ratchet the scheduled full run from report-only to blocking when the configured scope is clean enough to maintain.

Rollback is straightforward: disable the mutation workflow or Make targets without affecting application runtime behavior. The checked-in config can remain inert until the workflow is re-enabled.
