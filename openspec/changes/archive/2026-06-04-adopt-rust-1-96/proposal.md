## Why

Rust 1.96.0 was released on 2026-05-28 with a small but relevant set of stable language/library/tooling changes: copyable `core::range` range types, `assert_matches!`, new Clippy lints, rustdoc changes, and Cargo fixes for two third-party registry vulnerabilities. LushText is already pinned to Rust 1.95.0 across local, CI, and Snap build surfaces, so adopting 1.96 should be an intentional cross-repo alignment rather than an ad hoc version bump.

## What Changes

- Bump the LushText Rust floor from 1.95.0 to 1.96.0 across the workspace `rust-version`, local `rust-toolchain.toml`, GitHub Actions toolchain installs, Snap rustup bootstrap, README/AGENTS tech-stack text, and agent build guidance.
- Preserve Edition 2024 and the existing gtk-rs/GNOME 50 dependency floor; no GTK, Libadwaita, GtkSourceView, or Cargo dependency upgrade is implied by this change.
- Add a targeted Rust 1.96 audit pass that adopts only features that improve correctness or diagnostics:
  - Replace `assert!(matches!(...))` / `debug_assert!(matches!(...))` in tests with explicitly imported `std::assert_matches` / `std::debug_assert_matches` where the failure output becomes more useful.
  - Consider `core::range::Range` only for named byte-span value objects where `Copy` removes real clone/move friction; keep accepting legacy range syntax and `RangeBounds` at API boundaries where broad compatibility matters.
  - Add the new Clippy 1.96 lints that are safe for this codebase (`manual_option_zip`, `manual_pop_if`, `manual_noop_waker`) to the workspace lint policy if the explicit 1.96 lint probe stays clean.
- Update `.agents/rules/rust.md`, `.agents/rules/build.md`, and the Rust-quality skills so future agents know the current MSRV, the accepted 1.96 idioms, and the cases where novelty should be rejected.
- Run the 1.96 validation stack before implementation is considered complete: rustfmt, Clippy, rustdoc lint gate, core tests, and the repo documentation/rules check. Broader GTK, packaging, and release checks remain scoped by the touched files and existing build rules.

## Capabilities

### New Capabilities

- `rust-toolchain-adoption`: Defines how LushText adopts a new stable Rust toolchain, keeps version surfaces aligned, validates compatibility, adopts only useful new idioms, and updates agent-facing guidance.

### Modified Capabilities

- None.

## Impact

- Affected code/config: `Cargo.toml`, `rust-toolchain.toml`, `.github/workflows/*.yml` Rust setup steps, `snap/snapcraft.yaml`, README/AGENTS tech-stack references, `.agents/rules/rust.md`, `.agents/rules/build.md`, and `.agents/skills/gtk-perf-rust-optimize/SKILL.md` / `.agents/skills/gtk-perf-review/SKILL.md` where they mention MSRV or latest-stable idioms.
- Affected Rust code: primarily tests that currently use `assert!(matches!(...))`, plus any small, explicitly justified span/value-object sites that benefit from `core::range::Range` being `Copy`.
- Dependencies: no Cargo dependency change is planned; `Cargo.lock`, `workspace-hack`, and Flatpak `cargo-sources.json` should remain unchanged unless validation proves otherwise.
- Packaging: Flatpak continues to use the GNOME SDK `rust-stable` extension; Snap's rustup bootstrap must track the new pinned toolchain. CI must install 1.96.0 consistently in every stable lane.
