# Final Verification

Date: 2026-06-11

Change: `extract-gtk-lush-signals-and-settle`

## Results

- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
- `cargo nextest run --workspace` passed: 754 tests.
- `make test-widget-headless` passed cleanly on the final full run: 756 tests.
- An earlier full widget run reported `window::test_bookmark_gutter_edit_dialog_validates_moves_and_persists` as flaky on retry. The test then passed 6/6 isolated headless reruns and the subsequent full suite passed with no flaky summary.
- `make visual-geometry-smoke` passed.
- `make check-visual-proof-policy` passed against `build/smoke/visual-geometry/summary.json`, including `native-minimap-highlight-anchors` and `native-minimap-animation-highlight-anchors`.
- `make check-policy` passed.
- `make check` passed.
- `make check-agent-docs` passed after agent-rule and AGENTS guidance updates.
- `make gtk-lush-doctests gtk-lush-examples gtk-lush-msrv gtk-lush-api-advisory` passed. The semver advisory subtarget reported the expected missing crates.io baselines for unpublished `0.0.0` crates and continued; public API snapshots were generated.
- `cargo hakari generate` and `cargo hakari verify` passed.
- `make cargo-sources` passed with no `build-aux/cargo-sources.json` diff.
- `cargo deny check advisories bans sources licenses` passed with existing workspace-duplicate warnings and final `advisories ok, bans ok, licenses ok, sources ok`.
- `openspec validate extract-gtk-lush-signals-and-settle --strict` passed.
- `openspec validate --changes --strict` passed.
- `openspec validate --specs --strict` passed.
- `openspec validate --all --strict` passed.
- `git diff --check` passed.

## Deferred Or Retained Explicit Classes

The first functional API intentionally does not absorb every signal, binding, timer, or generation site.

Retained signal/binding classes:

- Short widget/action lifetime handlers that never stored a handler ID.
- Event-controller handlers where the controller owns the lifecycle.
- Declarative property bindings that already live for the owning widget lifetime.
- Non-signal row data such as drag-and-drop suppression state.

Retained timer/generation classes:

- Recurring pollers and heartbeats.
- Chunked model population, idle repair, and allocation-repair loops.
- Async worker freshness tokens and pure service/domain generations.
- Lifecycle delays that deliberately own a `SourceId` or depend on a narrower GTK allocation contract.

No readiness fields, automation snapshots, or documented automation behavior changed. The automation documentation and client checks still passed through `make check-policy` and `make check`.
