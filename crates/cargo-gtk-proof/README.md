# cargo-gtk-proof

`cargo-gtk-proof` is LushText's workspace tool for reusable GTK proof
artifacts. It is a Rust cargo subcommand, invoked as `cargo gtk-proof`, but it
is not a GTK Lush family library crate and is not published.

## Current Phase

This phase makes the Rust tool authoritative for schema validation, bounded
artifact envelopes, pure PNG/corpus replay checks, and visual proof-policy
checks. The live same-session visual runner is now Rust-backed by default:
`cargo gtk-proof run` materializes scenario cases, launches isolated headless
Mutter sessions, captures before/after screenshots and Automation1 geometry,
aggregates protected-region, rendered-anchor, animation-stream, warning-scan,
and workflow evidence, and writes an authoritative root summary. `cargo
gtk-proof run --oracle python` remains an explicit Rust-supervised
diagnostic/oracle path and is not default Rust proof.

## Commands

- `cargo gtk-proof --help`
  Prints command names, default artifact root, default scenario root, and tool
  version.
- `cargo gtk-proof schema list`
  Emits a JSON `artifact-envelope` listing supported schema identifiers.
- `cargo gtk-proof schema validate PATH`
  Validates a versioned proof JSON file and returns `unsupported-schema-version`
  or `malformed-field` for schema failures.
- `cargo gtk-proof summarize [DIR]`
  Validates `summary.json` in the artifact directory. The default is
  `build/smoke/visual-geometry`.
- `cargo gtk-proof corpus [DIR]`
  Replays the frozen compatibility corpus plus embedded pure PNG cases. The
  default corpus lives under `fixtures/proof-corpus`; the embedded PNG cases
  cover exact crops, allowed-changing regions, minimap detectors, crop
  artifacts, and rendered-anchor drift diagnostics.
- `cargo gtk-proof corpus --parity [DIR]`
  Replays the Python-oracle and Rust fixture fields for status, exit class,
  invariant IDs, warning-scan status, artifact path shape, engine metadata, and
  bounded details. Any mismatch exits nonzero and reports compatibility drift.
- `cargo gtk-proof policy --self-test`
  Runs Rust proof-policy negative and positive tests.
- `cargo gtk-proof policy [--artifact-dir DIR] [--base-ref REF] [--repo-root DIR]`
  Checks whether visual-sensitive local changes have current visual proof
  evidence. The default artifact directory is `build/smoke/visual-geometry`,
  and `--repo-root` resolves git state and file digests from another checkout
  (hermetic tests use it to point at a scratch repository).
- `cargo gtk-proof policy --require-rust-engine`
  Adds the post-migration guard that passing summaries must identify
  authoritative `cargo-gtk-proof` engine metadata, schema version, and scenario
  source. This rejects Python-only or diagnostic oracle summaries after the
  Rust live runner becomes the default.
- `cargo gtk-proof run`
  Runs the authoritative Rust live visual runner. It exits with status `3` and
  `unsupported-host` when host tooling is missing, otherwise writes
  schema-valid per-case manifests, comparison reports, optional animation
  reports, warning scans, and a root `summary.json` with
  `engine.authoritative=true`.
- `cargo gtk-proof run --oracle python`
  Runs the legacy Python visual runner under Rust process supervision with a
  bounded log and explicit `python-visual-oracle` engine metadata. Skipped or
  failed oracle output remains non-proof and does not make the Rust live runner
  authoritative.

## Result Envelope

Every non-help command writes one JSON `artifact-envelope` to stdout:

```json
{
  "ok": true,
  "status": "passed",
  "command": "schema",
  "detail": "schema validation passed",
  "version": {
    "schema_version": 1,
    "tool_version": "0.0.0"
  },
  "data": {}
}
```

Stable statuses include `passed`, `failed`, `usage-error`, `artifact-error`,
`unsupported-host`, `unsupported-schema-version`, `malformed-field`, and
`policy-failure`.

## Privacy Boundary

Proof artifacts may be uploaded to CI and shared with agents. The Rust tool
records paths, schema metadata, bounded diagnostics, invariant IDs, counts, and
relative artifact names. It must not print document text, note bodies, complete
search result text, raw image bytes, private persistence identifiers, or
unbounded logs in command output.
