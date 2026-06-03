## Why

Mutation testing and bounded property tests do not replace fuzzing for hostile
byte-ingestion paths. LushText still needs a dedicated way to feed arbitrary
bytes through decoding, Markdown preprocessing, and related parsers so crashes
and panics are found before users open unusual files.

## What Changes

- Add a `cargo-fuzz` fuzz project with initial byte-ingestion targets for
  editor decoding/file-health logic and Markdown preprocessing.
- Add bounded local and optional scheduled/manual fuzz commands that keep fuzzing
  separate from default tests, property tests, widget tests, and mutation tests.
- Add seed corpus and artifact handling guidance so crashes can be reproduced,
  minimized, and reviewed.
- Document the fuzzing lane in developer docs, build rules, and CI policy.

## Capabilities

### New Capabilities

- `byte-ingestion-fuzzing`: defines LushText's fuzzing lane for arbitrary byte
  inputs, fuzz target scope, crash handling, and default-test separation.

### Modified Capabilities

None.

## Impact

- Affected tooling: new `fuzz/` cargo-fuzz project, `Makefile`, and possibly CI
  workflow updates for manual or scheduled fuzz smoke runs.
- Affected code may include narrow feature-gated or public pure helpers around
  byte decoding and Markdown preprocessing so fuzz targets avoid GTK widgets.
- Affected docs/rules: new fuzzing documentation plus `.agents/rules/build.md`.
- New development tooling dependency: `cargo-fuzz` for maintainers running fuzz
  targets locally or in scheduled/manual CI.
