## Why

The initial property-test lane covers useful deterministic helpers, but several
cheap high-value round-trip invariants still live only in example tests. Adding
these properties now strengthens save-formatting, session/draft persistence, and
Replace All undo safety without broadening into GUI or live-session behavior.

## What Changes

- Add bounded property tests for EditorConfig save-formatting idempotence,
  including `trim_trailing_whitespace` and `insert_final_newline` combinations.
- Add bounded property tests for session and draft model serialization
  round-trips over generated but reviewable tab and manifest shapes.
- Add a bounded tempdir-backed service property proving Replace All followed by
  Undo restores byte-identical file contents when files have not diverged.
- Update property-testing docs and rules so tiny deterministic tempdir-backed
  service properties are allowed while GTK, watcher, file chooser, portal, and
  live-session flows remain out of scope.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `property-based-testing`: expand the property-test contract with additional
  round-trip coverage and clarify the boundary for deterministic tempdir-backed
  service properties.

## Impact

- Affected tests: `crates/lushtext-core/tests/properties.rs` and
  `crates/lushtext-core/tests/properties/*.rs`
- Affected code may include feature-gated pure or deterministic service hooks in
  `crates/lushtext-core/src/services/**` or `src/model/**` if needed for
  property access.
- Affected docs/rules: `docs/property-testing.md`, `.agents/rules/build.md`,
  and `.agents/skills/gtk-testing/SKILL.md`
- No runtime dependency changes are expected.
