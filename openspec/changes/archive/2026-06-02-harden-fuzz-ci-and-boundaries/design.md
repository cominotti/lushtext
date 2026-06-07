## Overview

This change hardens the fuzzing work that already exists or is being introduced
by the active `add-byte-ingestion-fuzzing` and
`add-corpus-replay-and-operation-fuzzing` changes. It keeps default validation
fast while ensuring the stable replay harness, committed corpus, and key
decode-boundary assumptions cannot silently rot.

## Goals

- Run stable corpus replay in ordinary CI without requiring nightly Rust,
  `cargo-fuzz`, sanitizer runtime, or a C/C++ compiler.
- Add bounded scheduled/manual fuzz exploration for maintainers who want
  coverage-guided discovery without affecting pull-request latency.
- Exercise corrupt `session.json` and draft manifest bytes through deserialize
  boundaries without requiring any stateful fuzzer or custom framework.
- Make the Markdown invalid-UTF-8 boundary explicit in docs.
- Make fuzz commands visible through `make help`.

## Non-Goals

- Do not add LibAFL.
- Do not add custom schedulers, custom feedback engines, distributed fuzz
  orchestration, or persistent fuzzer state beyond ordinary `cargo-fuzz` corpus
  files.
- Do not run coverage-guided fuzzing in every pull request.
- Do not start GTK, create widgets, use portals, watch the filesystem, or depend
  on a compositor in fuzz replay or fuzz targets.

## Decisions

### Stable Replay CI

Add a dedicated `fuzz-corpus-replay` CI job in the existing Fedora container
style. A separate job gives the harness and corpus their own check name and
makes failures easier to triage than hiding them inside `property-tests`.

The job should use stable Rust and run `make fuzz-corpus-replay`. It should
install only the normal GTK/build packages needed to compile the workspace and
must not install nightly Rust, `cargo-fuzz`, libFuzzer tooling, or a C++
compiler solely for replay. Because the replay test is feature-gated with
`required-features = ["fuzzing"]`, this explicit command is the CI contract.

### Scheduled Fuzz Smoke

Add a scheduled/manual fuzz smoke workflow or job that mirrors the weekly
mutation lane's shape: no pull-request trigger, explicit schedule, and
`workflow_dispatch`. This lane can install nightly Rust, `cargo-fuzz`, and a C++
compiler because it runs bounded coverage-guided exploration rather than stable
replay.

The smoke command should remain intentionally small. It is a liveness and crash
discovery signal, not an exhaustive fuzz campaign.

### Raw Persistence JSON Bytes

Extend the existing structured operation fuzz target with operations that feed
arbitrary bounded byte slices into:

- `serde_json::from_slice::<SessionData>(raw)`
- `serde_json::from_slice::<DraftManifest>(raw)`

The operation should discard the `Result` and assert only panic safety. This
models corrupt or truncated persistence files on disk without confusing the
contract with round-trip generation of well-formed models.

### Markdown Byte/Text Boundary

Keep Markdown preprocessing fuzzing as text-level coverage. The helper converts
bytes with lossy UTF-8 before exercising production Markdown preprocessing, so
invalid-byte behavior is deliberately covered by the editor byte-ingestion
target instead. Documenting this avoids double-counting the Markdown target as a
raw byte decoder.

### Discoverability

Update `make help` so the fuzz commands appear with the other test targets.
This is small but important because the Makefile command header already lists
them and developers use `make help` as the source of truth.

## Validation Strategy

- `openspec validate harden-fuzz-ci-and-boundaries --strict`
- `make fuzz-corpus-replay`
- `make fuzz-list`
- `make fuzz-operation-smoke`
- `make fuzz-smoke` or the same bounded command used by the scheduled workflow
- `make test-prop`
- `cargo fmt --all`
- `cargo +nightly fmt --manifest-path fuzz/Cargo.toml` if fuzz project files
  change
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Focused clippy/build command for the fuzz target if required by the fuzz
  project
- CI workflow syntax validation when available
