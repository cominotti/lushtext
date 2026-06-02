## 1. Property Test Infrastructure

- [x] 1.1 Add `proptest` as a workspace test/development dependency without adding any runtime application dependency.
- [x] 1.2 Add a `property-tests` feature and a dedicated `lushtext-core` property-test target guarded by `required-features = ["property-tests"]`.
- [x] 1.3 Add bounded shared property-test helpers for case counts, shrink limits, generated string/path/vector sizes, and regression persistence.
- [x] 1.4 Run `cargo hakari generate` after the dependency change.
- [x] 1.5 Run `make cargo-sources` so Flatpak vendoring stays in sync with the updated dependency graph.

## 2. Developer and CI Commands

- [x] 2.1 Add a local `make test-prop` target that runs the dedicated property-test target with the `property-tests` feature enabled.
- [x] 2.2 Add a documented way to run a deeper manual or scheduled property pass with higher case counts.
- [x] 2.3 Add CI coverage for the bounded property-test lane without folding it into the default mutation workflow.
- [x] 2.4 Ensure `cargo nextest run --workspace` continues to skip the property-test target unless `property-tests` is explicitly enabled.
- [x] 2.5 Ensure `scripts/run-mutants.sh` and the mutation CI workflow continue to omit the property-test target by default.

## 3. Initial Property Coverage

- [x] 3.1 Add bounded inline-footnote property tests for protected-region preservation and generated-label collision avoidance.
- [x] 3.2 Add bounded search-replacement property tests for range/order/clipping invariants in deterministic helper logic.
- [x] 3.3 Add bounded path-rebasing property tests for workspace, document-note, workspace-note, bookmark, or related sidecar helpers.
- [x] 3.4 Add bounded palette property tests for merge ordering, max truncation, and stable tie handling.
- [x] 3.5 Add bounded encoding, line-ending, and sidecar-identity property tests for deterministic identifiers, separators, and hash behavior.
- [x] 3.6 Keep GTK widget behavior, watcher timing, file chooser flows, and live session behavior out of the property-test target.

## 4. Documentation

- [x] 4.1 Document property-testing scope, commands, case-count policy, failure replay, and regression-file handling.
- [x] 4.2 Update `.agents/rules/build.md` with the property-test command and the rule that mutation and property lanes stay separate by default.
- [x] 4.3 Update mutation-testing documentation to describe how property tests complement, but do not replace, mutation tests.
- [x] 4.4 Document how to add a future explicit mutation/property opt-in if a tiny property test is intentionally useful under mutation.

## 5. Validation

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.3 Run `cargo nextest run --workspace` and verify the property-test target is not included by default.
- [x] 5.4 Run `make test-prop`.
- [x] 5.5 Run `scripts/run-widget-tests.sh --headless --retries 1` unless the final implementation only changes non-UI test infrastructure.
- [x] 5.6 Run `cargo bench -p lushtext-core --no-run`.
- [x] 5.7 Run `make mutants-smoke` or an equivalent focused mutation check to prove the mutation wrapper still works.
- [x] 5.8 Validate any changed GitHub Actions workflow with `actionlint` when available.
- [x] 5.9 Run `openspec validate add-property-based-testing --strict`.
