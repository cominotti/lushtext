# cargo-gtk-proof

`cargo-gtk-proof` is LushText's workspace tool for reusable GTK proof
artifacts. It is a Rust cargo subcommand, invoked as `cargo gtk-proof`, but it
is not a GTK Lush family library crate and is not published.

## Current Phase

This phase makes the Rust tool authoritative for schema validation, bounded
artifact envelopes, pure PNG/corpus replay checks, and visual proof-policy
self-tests. The live same-session visual runner is still the existing Python
runner in `scripts/visual-geometry-smoke.py`; `cargo gtk-proof run` currently
returns a stable `unsupported-host` envelope instead of claiming rendered proof.
Wrappers must not default to Rust live execution until corpus, live-runner, and
animation parity are recorded.

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
  default corpus lives under `fixtures/proof-corpus`.
- `cargo gtk-proof policy --self-test`
  Runs Rust proof-policy negative and positive tests.
- `cargo gtk-proof policy [--artifact-dir DIR] [--base-ref REF]`
  Checks whether visual-sensitive local changes have current visual proof
  evidence. The default artifact directory is `build/smoke/visual-geometry`.
- `cargo gtk-proof run`
  Reserved for the Rust live visual runner. In this phase it exits with status
  `3` and a JSON envelope whose status is `unsupported-host`.

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
