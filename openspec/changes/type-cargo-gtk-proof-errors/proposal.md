# Proposal

## Why

`cargo-gtk-proof`, the workspace's visual proof tool, reports failures as strings. Measured on `origin/main` `761866ff`; task 0.1 re-measures:

- **Errors.** 172 `Result<_, String>` signatures: `live.rs` 52, `runner.rs` 36, `automation.rs` 16, `capture.rs` 16, `policy.rs` 14, `process.rs` 12, others 26. `png.rs` declares `type PngResult<T> = Result<T, String>`, so a failure's cause becomes text and cannot be inspected.
- **Statuses.** The statuses the tool originates are free strings (`status: String` in `png.rs:73` and `automation.rs:41,52`, and many `json!` sites), although they come from a closed set (`passed`, `failed`, `skipped`, `captured`, …). The tool already has one typed vocabulary, `ProofStatus` (`lib.rs:258`).

## What Changes

- **Typed errors.** Each module gets an error `enum` (`thiserror`) with its cause in `source()`, and the CLI edge uses `anyhow` with context. The pure modules convert first, `runner` and `live` last.
- **Closed statuses.** The statuses the tool itself originates become closed enums whose serde form is today's string.
  - `status`, `exit_class`, paths and schemas stay byte-identical.
  - Values carried from the application (`ReadinessWait.status`) or from manifests (`AnimationAnchorSpec.detector`) stay strings.
  - Failure `detail` text changes, because it now renders the cause chain.

## Non-goals

- **Moving `run_self_tests()` / `run_embedded_png_corpus()` behind `#[cfg(test)]` (declined).** They back `cargo gtk-proof policy --self-test` and `corpus` (`lib.rs:545,351`), and the `cargo-gtk-proof` spec requires the first ("Proof policy target has Rust parity evidence").
- **Removing the Python visual-policy implementation.** Its parity is deliberate (`Makefile:936-940`).
- **A flat PNG buffer (declined).** No measurement shows a cost (disposition record).

## User-visible impact

None for LushText users. `cargo-gtk-proof` failure messages gain the cause chain, so their `detail` text changes. Statuses, exit classes, paths and schemas are unchanged byte for byte.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cargo-gtk-proof`: failures are typed with a cause chain, and the statuses the tool originates are a closed set with an unchanged wire form.

## Impact

- **Code:** `crates/cargo-gtk-proof/src/**` and its `Cargo.toml` (`thiserror` and `anyhow` are workspace dependencies; `cargo hakari generate` afterwards).
- **Proof:** `crates/cargo-gtk-proof/src/` is a visual-sensitive prefix (`policy.rs:21`), so every commit owes a current `make visual-geometry-smoke` summary.
- **Order:** the Start line in `tasks.md`.
