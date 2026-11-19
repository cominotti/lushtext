# Design

## Context

See `proposal.md`. Only `model.rs` derives `Deserialize`, for scenario manifests. Summaries are read back as `serde_json::Value` and already fail closed on an unexpected status (`policy.rs:549-555`).

## Goals / Non-Goals

**Goals:** typed failures; closed, wire-stable statuses. **Non-Goals:** `proposal.md`.

## Decisions

### D1. One error enum per module, `anyhow` only at the binary edge

- **Shape.** `PngError`, `PolicyError`, `ProcessError`, `CaptureError`, `AutomationError`, `RunnerError`, `LiveError`, named for their workflow per `rust.md` "Error and Literal Ownership". Variants carry `#[source]` for I/O, JSON and child-process causes.
- **Messages.** A message does not repeat its source (`#[error("failed to read PNG {path}")]` with `#[source] io::Error`), so a chain printed with `{:#}` shows each cause once.
- **The edge.** `main` and `lib.rs`'s command dispatch return `anyhow::Result`, add `.context` at each subcommand, and print the chain. Exit codes are unchanged, because they come from the result envelope, not the message.
- **Order.** `png`, `policy`, `geometry`, `warnings`, `reveal_fixture` (pure), then `process`, `capture`, `automation`, `host`, `artifacts`, then `runner` and `live`. Each step builds and its tests pass, so the work can stop between steps.
- **Rejected:**
  - One crate-wide `ProofError`. It is a catch-all whose variants no caller matches.
  - `anyhow` everywhere. Library callers inside the crate match on PNG and policy failures (the corpus counts `failed`).

### D2. Closed enums with explicit serde names, for tool-originated values only

- **Names.** Each tool-originated field gets its own `enum` (for example `DetectionStatus`, `CaptureStatus`, `CorpusStatus`, `ExitClass`), following the existing `ProofStatus` (`lib.rs:258`). An explicit `#[serde(rename = "...")]` sits on every variant, never `rename_all`, so a rename of a Rust variant cannot change the wire.
- **Out of scope.** `ReadinessWait.status` (the application's D-Bus vocabulary, `automation.rs:296-307`) and `AnimationAnchorSpec.detector` (manifest input, `live.rs:1378`) stay `String`, because closing them would couple the tool to another program's vocabulary.
- **Inventory first.** It covers every literal written to these fields, including the `json!` sites in `lib.rs`, `live.rs` (for example `live.rs:1311,1352,1679,1691,1715`) and `policy.rs`, and every value found in the Python oracle fixtures. Dynamic `json!` sites write `Enum::as_str()`, which a test ties to the serde name.
- **No read path is added.** Only `model.rs` derives `Deserialize`, for scenario manifests. Summaries are read as `serde_json::Value` and already fail closed on an unexpected status (`policy.rs:549-555`), and that behaviour stays.
- **Wire proof.** A golden-file test serializes every variant and compares it with the pre-change strings. The existing artifact tests (`artifacts.rs:486-571`, `lib.rs:764-1082`) must pass unchanged, except where they assert on a failure's `detail` text, which D1 changes on purpose; each such assertion is updated and listed in the handoff.
- **Schemas.** If a published schema descriptor lists these fields as free strings, it stays as it is unless `cargo-gtk-proof`'s schema-versioning rule allows an `enum` constraint without a version bump.


## Risks / Trade-offs

- **The wire form drifts while it is being typed.** → D2's golden test, plus the unchanged artifact tests and the PNG corpus.
- **Large mechanical diff in `live.rs` and `runner.rs`.** → They come last. Each tier's commit has a current `make visual-geometry-smoke` summary, or the tiers land as one commit under one proof.

## Verification scope

- **Kani:** none (no proven module touched).
- **Timing:** none (no timed path touched).
- **Visual proof:** `make visual-geometry-smoke` is current before every commit touching `crates/cargo-gtk-proof/src/`.
