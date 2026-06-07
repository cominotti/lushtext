## 1. Fuzz Project Setup

- [x] 1.1 Initialize a `cargo-fuzz` project under `fuzz/` with fuzz-only dependencies isolated from normal workspace builds.
- [x] 1.2 Add initial fuzz targets for editor byte ingestion and Markdown preprocessing.
- [x] 1.3 Add seed corpus directories and reviewable starter inputs for the initial fuzz targets.
- [x] 1.4 Ensure generated fuzz artifacts and crash outputs are ignored while intentional corpus seeds remain reviewable.

## 2. Fuzzable Helper Boundaries

- [x] 2.1 Add narrow fuzz-facing helper access to editor byte decoding, encoding-state, line-ending, and file-health classification logic without requiring disk I/O or GTK.
- [x] 2.2 Add narrow fuzz-facing helper access to Markdown preprocessing/parser setup without constructing `LushtextMarkdownPreview`, `GtkTextView`, GSettings, or other GTK objects.
- [x] 2.3 Keep fuzz helper APIs feature-gated or otherwise scoped so ordinary application and test builds do not expose unnecessary public surface.
- [x] 2.4 Ensure fuzz targets do not start GTK, create widgets, open file choosers, watch filesystems, use portals, or require a compositor.

## 3. Commands and CI Policy

- [x] 3.1 Add a local `make fuzz-list` or equivalent command that lists configured fuzz targets.
- [x] 3.2 Add a bounded `make fuzz-smoke` command that runs initial fuzz targets with explicit time and/or input-length limits.
- [x] 3.3 Keep fuzz commands out of default `make test`, default property tests, widget tests, benchmark compile, and mutation testing.
- [x] 3.4 Add optional scheduled/manual CI coverage for fuzz smoke, or document why CI enablement is deferred.

## 4. Documentation and Rules

- [x] 4.1 Add fuzzing documentation covering scope, commands, target list, longer manual runs, crash reproduction, minimization, and corpus handling.
- [x] 4.2 Update `.agents/rules/build.md` with fuzz command rules and the default-lane separation policy.
- [x] 4.3 Document how real fuzz crashes should become minimized corpus seeds, deterministic regression tests, or an explicit no-seed rationale.

## 5. Validation

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.3 Run `cargo nextest run --workspace` and verify fuzz targets are not part of default tests.
- [x] 5.4 Run `cargo fuzz list` or the new fuzz-list wrapper.
- [x] 5.5 Run `make fuzz-smoke`.
- [x] 5.6 Run `make test-prop` to verify the property lane remains separate and green.
- [x] 5.7 Run `make mutants-smoke` or an equivalent focused mutation-wrapper check to verify fuzzing did not enter mutation defaults.
- [x] 5.8 Run `actionlint` on any changed GitHub Actions workflows when available.
- [x] 5.9 Run `git diff --check`.
- [x] 5.10 Run `openspec validate add-byte-ingestion-fuzzing --strict`.
