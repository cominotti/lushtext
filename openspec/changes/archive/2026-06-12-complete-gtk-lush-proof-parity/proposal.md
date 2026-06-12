## Why

GTK Lush cannot honestly enter the Phase 5 publishing gate while its visual
proof system still says Python is the authoritative live runner and
`cargo gtk-proof run` is non-coverage. This change completes the proof parity
phase so the Rust tool owns live visual proof only after it preserves the
current same-session, pixel-anchor, animation-stream, artifact, policy, and
privacy contracts.

## What Changes

- Implement `cargo gtk-proof run` as the authoritative Rust live visual runner
  for LushText visual-geometry scenarios once parity is proven.
- Port same-session before/after capture, Automation1 readiness waits, scenario
  matrix expansion, protected-region comparison, screenshot-derived pixel
  anchors, mask handling, warning scans, and bounded artifact writing from the
  Python visual runner into Rust.
- Port animation-frame stream proof into Rust, including timestamp-correlated
  frame/sample pairing, intermediate-frame requirements, skew limits, and
  bounded failure artifacts.
- Expand the compatibility corpus so Python and Rust agree on representative
  pass, fail, skip, unsupported-host, malformed-artifact, rendered-drift,
  warning-scan, stale-frame, and final-settle-only cases before wrappers flip.
- Move default visual proof wrappers and scheduled smoke coverage to Rust only
  after parity is recorded, while keeping compatibility aliases and diagnostics
  for the old Python path during the transition.
- Preserve `scripts/lushtext-automation.py` result envelopes and exit classes,
  delegating generic proof summaries to Rust only when shapes are parity-tested.
- Update schema descriptors, proof documentation, GTK Lush governance, and the
  umbrella roadmap so Phase 4 proof extraction is genuinely closed and Phase 5
  publishing remains a separate later change.
- No publishing, second-consumer adoption, repository split, crates.io release,
  or Phase 6 upstreaming work is included.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cargo-gtk-proof`: promote the Rust proof tool from staged schema/corpus/policy
  support to authoritative live visual execution with parity gates.
- `visual-geometry-invariants`: require Rust to preserve same-session pixel
  oracles, animation-stream proof, masks, artifact metadata, and skip/failure
  semantics before Python retirement.
- `desktop-visual-smoke-coverage`: switch local and scheduled visual-geometry
  smoke defaults to Rust after parity while preserving evidence layout and
  unsupported-host behavior.
- `automation-client-tools`: preserve automation-client command/result
  compatibility while delegating generic visual proof summaries to Rust.
- `gtk-lush-program-governance`: close the Phase 4 proof audit, record parity
  evidence, and keep Phase 5 publishing gates deferred.

## Impact

- Affects `crates/cargo-gtk-proof`, especially live runner orchestration,
  schema/model validation, PNG comparison, policy checks, corpus replay, and
  artifact envelope generation.
- Affects visual proof scripts and wrappers:
  `scripts/visual-geometry-smoke.py`, `scripts/test-visual-geometry.py`,
  `scripts/visual_geometry_png.py`, `scripts/check-visual-proof-policy.py`,
  `scripts/lushtext-automation.py`, and their Makefile/CI entry points.
- Affects visual scenario manifests, proof corpus fixtures, ignored/generated
  artifact roots, and smoke workflow retention/summary expectations.
- Affects documentation and governance: `docs/gtk-proof-schemas.md`,
  `docs/end-user-coverage.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `docs/next/gtk-lush.md`, and
  `crates/gtk-lush/GOVERNANCE.md`.
- Requires extensive verification: Rust unit/property/corpus tests, Python/Rust
  parity replay, full widget headless suite, visual-geometry smoke,
  proof-policy checks, automation docs/self-tests, specialist delegated reviews,
  and the strict OpenSpec validation ladder.
