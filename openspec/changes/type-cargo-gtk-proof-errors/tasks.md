# Tasks

Tests: no behaviour change except the failure `detail` text that cause chains produce; the golden, artifact and corpus tests are the oracle.
Kani: none (no proven module touched).
Timing: none (no timed path touched).
Visual proof: `make visual-geometry-smoke` current before every commit touching `crates/cargo-gtk-proof/src/`.
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first.

## 0. Re-measure

- [ ] 0.1 Re-count the `Result<_, String>` signatures per file and the status literals on the then-current `origin/main`, and update `proposal.md`. Verify with `openspec validate type-cargo-gtk-proof-errors --strict`.

## 1. Typed results in `cargo-gtk-proof`

- [ ] 1.1 Inventory every tool-originated status and exit-class value (design D2), including the `json!` sites and the Python oracle fixtures, into the change's handoff. Add the golden-file test of today's strings. Verify that it passes on `main`.
- [ ] 1.2 Introduce the closed enums with explicit per-variant serde names and an `as_str()` tied to them by a test. Leave `ReadinessWait.status` and `AnimationAnchorSpec.detector` as strings. Verify that the golden test, the `artifacts.rs` and `lib.rs` tests and `cargo run -q -p cargo-gtk-proof -- policy --self-test` pass unchanged.
- [ ] 1.4 Add `thiserror` and `anyhow` to the crate, run `cargo hakari generate`, and convert the pure modules' errors (design D1, first tier). Add one test that a missing PNG's error has an `io::Error` source. Verify with `cargo test -p cargo-gtk-proof` and `make check-clippy`.
- [ ] 1.5 Convert the second and third tiers in separate commits. Verify after each that `cargo test -p cargo-gtk-proof` and `make check-visual-proof-policy` pass. Then `git grep -n -E 'Result<[^>]*, *String>' -- crates/cargo-gtk-proof/src` prints nothing.
- [ ] 1.6 Update the `cargo-gtk-proof` README or docs if they describe error output, and list every updated `detail` assertion in the handoff. Before each commit of this group, run `make visual-geometry-smoke` so its summary is current, then commit through `heavy.sh`, signed, and push.

## Workflow follow-up

- Archive after review and merge under the programme's merge rules.
