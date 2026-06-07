## Why

The new `cargo-fuzz` lane covers hostile byte ingestion, but known corpus seeds
still need a stable, low-friction replay path that can run without nightly or
sanitizer setup. LushText also has deterministic service workflows where bugs
can emerge from operation ordering, so a small structured operation fuzzing
target is a better next step than introducing LibAFL.

## What Changes

- Add a stable corpus replay lane that runs committed fuzz corpus seeds through
  deterministic non-GTK helper surfaces on stable Rust.
- Add structured operation fuzzing that maps bounded arbitrary bytes into small
  deterministic editor/service operation scripts.
- Keep both lanes GTK-free and out of default test, property, widget, benchmark,
  and mutation runs unless explicitly invoked or scheduled.
- Document the relationship between corpus replay, `cargo-fuzz`, property
  tests, and mutation tests.
- Do not add LibAFL or custom fuzzing-framework infrastructure.

## Capabilities

### New Capabilities

- `stable-fuzz-corpus-replay`: deterministic replay of committed fuzz corpus
  seeds on stable Rust without requiring nightly, libFuzzer, sanitizer runtime,
  or C/C++ compiler setup.
- `structured-operation-fuzzing`: non-LibAFL structured operation fuzzing for
  bounded editor/service operation scripts generated from arbitrary bytes.

### Modified Capabilities

- None.

## Impact

- Affected code: fuzz helper boundaries, stable integration/unit test support,
  `Makefile`, fuzz corpus handling, and documentation.
- Affected systems: local validation, optional scheduled/manual validation, and
  agent build/testing rules.
- Dependencies: no LibAFL dependency; no new dependency should enter default
  workspace builds. Any operation fuzz target should reuse existing `cargo-fuzz`
  infrastructure or existing property-test infrastructure.
