## Why

Phase 4 turns LushText's proof infrastructure into reusable GTK Lush pieces:
the headless widget harness, the bounded readiness/snapshot spine, and the
Rust-side visual proof schema/policy/corpus boundary are now mature enough to
extract. The live visual runner remains too LushText-shaped to retire in the
same move, so this phase keeps Python as the authoritative live path while
making the Rust parity gate explicit. Extracting the reusable pieces now keeps
the GTK Lush program honest: every reusable crate must ship with real proof,
and the proof toolchain itself must be governed by the same
one-crate-in-an-afternoon rule.

This also removes a growing maintenance risk. The current Python visual runner,
widget harness, proof policy checker, Automation1 helper, and scenario schema
act as executable lore; this change promotes that lore into typed Rust crates,
a cargo subcommand, documented schemas, compatibility tests, and LushText
wrappers that preserve today's gates while making the next phase possible.

## What Changes

- Add `gtk-lush-proof-harness` as a GTK Lush family crate for headless Mutter
  and private D-Bus session bootstrap, per-test subprocess execution,
  bounded flake retry reporting, warning hygiene hooks, and GTK main-loop wait
  helpers such as the current `wait_until` drain semantics.
- Add `gtk-lush-proof-spine` as a GTK-free reusable runtime crate for
  versioned readiness predicates, readiness blockers, workflow events, bounded
  snapshots, privacy-safe artifact envelopes, and implementation traits that a
  consumer app can adapt to its own automation surface.
- Add `cargo-gtk-proof` as a Rust workspace tool, not a GTK Lush family leaf,
  for versioned proof schema descriptors, bounded result envelopes, PNG
  crop/anchor comparison primitives, compatibility corpus replay, and Rust
  proof-policy validation. The `run` command is reserved but non-authoritative
  until a later live-runner parity phase.
- Migrate LushText's widget test harness onto the extracted harness and map
  Automation1 readiness/workflow/snapshot value objects through the proof
  spine without changing the user-facing Automation1 D-Bus contract or
  weakening any existing visual invariant.
- Preserve the current Python runner and policy checker as the compatibility
  oracle until the Rust tool proves identical pass/fail decisions, required
  summary fields, and bounded artifact paths on a frozen corpus. After parity,
  Makefile/script entry points become thin compatibility wrappers around the
  Rust tool in a later phase; the proof-policy checker moves last.
- Publish scenario, summary, policy, and artifact schemas as checked
  documentation and machine-readable fixtures so future agents can validate
  proof evidence without reverse-engineering scripts.
- Update GTK Lush governance, workspace policy, README, vision, automation
  docs, and proof docs to describe which pieces are family crates, which piece
  is a workspace cargo tool, and which publishing/second-consumer work remains
  deferred to Phase 5.
- Add broad verification: crate unit tests, doctests, examples, widget harness
  tests, compatibility corpus tests, schema tests, CLI tests, docs drift
  checks, visual proof policy negative tests, full LushText gates, and
  delegated reviews for GTK testing, live GTK behavior, architecture,
  responsiveness, data safety/privacy, comments, and proof evidence quality.

## Capabilities

### New Capabilities

- `gtk-lush-proof-harness`: reusable headless GTK test-session and widget
  harness behavior for stock gtk-rs applications.
- `gtk-lush-proof-spine`: reusable bounded readiness, snapshot, workflow-event,
  and artifact-envelope protocol primitives for app-owned automation surfaces.
- `cargo-gtk-proof`: Rust cargo subcommand for GTK proof schemas, bounded
  artifact envelopes, compatibility corpus validation, PNG proof primitives,
  and proof-policy enforcement.

### Modified Capabilities

- `gtk-lush-workspace`: integrate the new proof family crates while explicitly
  hosting `cargo-gtk-proof` as a workspace tool outside the family leaf-crate
  policy.
- `gtk-lush-program-governance`: record Phase 4 conformance, non-framework
  boundaries, review gates, and the deferred Phase 5 publishing/second-consumer
  boundary.
- `dbus-automation-spine`: require LushText Automation1 to implement the
  reusable proof-spine traits with zero D-Bus surface drift and the same privacy
  guarantees.
- `visual-geometry-invariants`: allow the Rust proof tool to become the
  authoritative runner only after corpus parity proves the same scenario,
  rendering, animation, and artifact decisions as the existing Python runner.
- `automation-client-tools`: keep the LushText automation client stable while
  documenting which schema, corpus, and policy responsibilities move into
  `cargo-gtk-proof`.
- `desktop-visual-smoke-coverage`: preserve the end-user smoke matrix while
  keeping the Python live runner stable and maintaining bounded, reviewable
  artifacts that the Rust proof tool can validate.

## Impact

- Adds new workspace crates under `crates/gtk-lush/proof-harness` and
  `crates/gtk-lush/proof-spine`.
- Adds a Rust workspace cargo tool, recommended as `crates/cargo-gtk-proof`,
  with CLI integration for schema validation, corpus replay, and Rust
  proof-policy self-tests.
- Updates root workspace membership, workspace dependencies, cargo-hakari,
  nextest/cargo-deny policy, MSRV/API advisory lanes, GTK Lush policy scripts,
  and documentation checks.
- Migrates `crates/lushtext/tests/widget.rs`,
  `crates/lushtext/tests/widget/common.rs`, and `scripts/run-widget-tests.sh`
  toward the extracted API/tool surface. The Python visual runner, Python proof
  policy wrapper, and automation artifact-summary path remain stable reference
  paths until parity gates pass.
- Touches LushText Automation1 adapters in `crates/lushtext-core/src/ui` and
  `crates/lushtext-core/src/model` only through adapter code that preserves the
  documented D-Bus interface, action catalog, readiness predicates, snapshot
  fields, and privacy boundaries.
- Requires updates to `docs/next/gtk-lush.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `crates/gtk-lush/README.md`,
  `crates/gtk-lush/GOVERNANCE.md`, Makefile targets, CI, and OpenSpec specs.
- Does not include Phase 5 publishing, crates.io functional release, a second
  non-LushText consumer, upstreaming, an Automation1 redesign, a custom view
  DSL, a state/message framework, or Libadwaita replacement behavior.
