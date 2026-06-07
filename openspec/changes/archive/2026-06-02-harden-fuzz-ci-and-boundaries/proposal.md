## Why

The new fuzz corpus replay harness is valuable only if CI exercises it; today a
feature-gated replay test can regress while ordinary workspace tests continue to
skip it. The fuzzing lane should also close the small remaining persistence
decode boundary and make its byte/text boundaries discoverable without adding
LibAFL or making PR validation slow.

## What Changes

- Add stable CI coverage for `make fuzz-corpus-replay` so committed fuzz seeds
  are replayed on push and pull-request validation.
- Add an optional scheduled/manual fuzz smoke lane that runs bounded
  `cargo-fuzz` exploration outside default PR CI.
- Extend structured operation fuzzing with arbitrary raw-byte JSON decode cases
  for session and draft persistence data.
- Document that Markdown fuzzing is a text-level preprocessing target and that
  invalid UTF-8 byte ingestion belongs to the editor byte-decode target.
- List fuzz replay and smoke commands in `make help` so the new lane is visible
  from the standard developer command index.

## Capabilities

### New Capabilities

- `fuzz-ci-and-boundary-hardening`: CI coverage, scheduled smoke policy,
  persistence-boundary fuzz inputs, Markdown byte/text boundary documentation,
  and fuzz-command discoverability for the fuzzing lane.

### Modified Capabilities

None.

## Impact

- Affected workflows: `.github/workflows/ci.yml` and a scheduled/manual fuzz
  smoke workflow or equivalent non-PR job.
- Affected tooling: `Makefile`, fuzz corpus seeds, and bounded fuzz commands.
- Affected code: feature-gated deterministic fuzz helper logic for structured
  operation fuzzing.
- Affected docs/rules: `docs/fuzzing.md`, build rules, and any test-skill
  guidance that lists validation lanes.
- Dependencies: no LibAFL dependency and no new default workspace dependency;
  scheduled `cargo-fuzz` smoke may install nightly Rust, `cargo-fuzz`, and a C++
  compiler only in the fuzz workflow.
