## 1. Stable Replay CI

- [x] 1.1 Add a GitHub Actions CI job or step that runs `make fuzz-corpus-replay`
      on push and pull-request validation.
- [x] 1.2 Keep the replay job on stable Rust and avoid nightly, `cargo-fuzz`,
      libFuzzer, and C/C++ compiler requirements for stable corpus replay.
- [x] 1.3 Ensure the job fails with visible diagnostics when a committed corpus
      seed fails replay.

## 2. Scheduled Fuzz Smoke

- [x] 2.1 Add a scheduled/manual fuzz smoke workflow or non-PR job with
      `schedule` and `workflow_dispatch` triggers.
- [x] 2.2 Install the required fuzz-only tooling in that lane: nightly Rust,
      `cargo-fuzz`, and the compiler/runtime packages needed by libFuzzer.
- [x] 2.3 Run bounded fuzz smoke with explicit target, time, input-size, and
      operation-count limits.

## 3. Persistence Decode Boundary

- [x] 3.1 Extend structured operation fuzzing with raw-byte
      `SessionData` decode operations.
- [x] 3.2 Extend structured operation fuzzing with raw-byte `DraftManifest`
      decode operations.
- [x] 3.3 Add durable operation corpus seeds for invalid, truncated, and
      minimally valid persistence JSON bytes.

## 4. Documentation and Discoverability

- [x] 4.1 Document the Markdown text-level fuzzing boundary and the editor
      invalid-UTF-8 byte-ingestion boundary in `docs/fuzzing.md`.
- [x] 4.2 Add `fuzz-corpus-replay`, `fuzz-smoke`, and
      `fuzz-operation-smoke` to `make help`.
- [x] 4.3 Update build/test rules or testing skill guidance if they enumerate
      validation lanes.

## 5. Validation

- [x] 5.1 Run `openspec validate harden-fuzz-ci-and-boundaries --strict`.
- [x] 5.2 Run `make fuzz-corpus-replay`.
- [x] 5.3 Run `make fuzz-list`.
- [x] 5.4 Run `make fuzz-operation-smoke`.
- [x] 5.5 Run the bounded fuzz smoke command used by the scheduled workflow.
- [x] 5.6 Run `make test-prop`.
- [x] 5.7 Run formatting and lint checks for touched workspace and fuzz project
      files.
- [x] 5.8 Validate GitHub Actions syntax when the local tooling is available.
