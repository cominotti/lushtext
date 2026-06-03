## 1. Property Test Generators

- [x] 1.1 Add bounded generated text helpers for whitespace, tabs, LF, CRLF, CR, empty strings, and mixed line-ending save-formatting cases.
- [x] 1.2 Add bounded generated session helpers for file-backed and untitled `SessionTab` values, pinned state, cursor/scroll positions, draft IDs, and active-tab indices.
- [x] 1.3 Add bounded generated draft helpers for `DraftEntry` and `DraftManifest` values with optional paths, optional mtimes, saved timestamps, and stable entry ordering.
- [x] 1.4 Add bounded generated Replace All fixture helpers for tiny tempdir-backed files, one or more lines, and non-overlapping replacement ranges.

## 2. Round-Trip Properties

- [x] 2.1 Add an EditorConfig save-formatting property proving `apply_save_formatting_overrides()` is idempotent for generated override combinations.
- [x] 2.2 Add a session serialization property proving generated `SessionData` and `SessionTab` values survive JSON serialize-deserialize round-trips.
- [x] 2.3 Add a draft serialization property proving generated `DraftManifest` and `DraftEntry` values survive JSON serialize-deserialize round-trips.
- [x] 2.4 Add a Replace All -> Undo property proving immediate undo restores non-diverged generated files to byte-identical original contents.
- [x] 2.5 Keep all new properties in the existing feature-gated `lushtext-core` property target and out of default nextest and mutation runs.

## 3. Documentation and Rules

- [x] 3.1 Update `docs/property-testing.md` with the new round-trip coverage areas.
- [x] 3.2 Update property-testing docs and `.agents/rules/build.md` to allow bounded deterministic tempdir-backed service properties while excluding GTK, watchers, file choosers, portals, and live sessions.
- [x] 3.3 Update `.agents/skills/gtk-testing/SKILL.md` if needed so agents choose property tests for deterministic broad-input invariants and widget tests for live UI behavior.

## 4. Validation

- [x] 4.1 Run `cargo fmt --all -- --check`.
- [x] 4.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 4.3 Run `cargo clippy -p lushtext-core --features property-tests --test properties -- -D warnings`.
- [x] 4.4 Run `cargo nextest run --workspace` and verify the property-test target is still excluded by default.
- [x] 4.5 Run `make test-prop`.
- [x] 4.6 Run `git diff --check`.
- [x] 4.7 Run `openspec validate expand-property-roundtrip-coverage --strict`.
