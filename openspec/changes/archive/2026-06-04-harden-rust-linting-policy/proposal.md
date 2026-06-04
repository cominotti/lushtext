## Why

LushText already has a strong curated Rust lint table, but recent exploration showed that the current gate leaves valuable Clippy, Cargo, rustc, dependency-policy, and tool-pinning signal outside the blocking validation path. This change raises the repository from "good lint hygiene" to a comprehensive, evidence-backed Rust quality policy that catches non-idiomatic code, maintainability drift, dependency-license risk, and validation-tool drift before they become normal.

## What Changes

- Expand the normal Clippy gate to cover all workspace targets and all enabled workspace features.
- Clean up all currently identified high-signal lint findings, then add those lints to the blocking workspace lint policy.
- Add a checked-in advisory lint lane for broad `pedantic`, `nursery`, `cargo`, maintainability, and selected rustc lints so future lint discovery is repeatable without turning noisy groups into hard gates.
- Keep broad `clippy::restriction`, `clippy::pedantic`, `clippy::nursery`, and `clippy::cargo` groups out of the blocking gate unless individual lints are explicitly curated.
- Evaluate and apply `clippy.toml` only for globally safe project policy; path-sensitive policies such as the filesystem boundary remain enforced by the existing path-aware audit script.
- Clean up rustc lint candidates that currently produce noise, including unreachable public items, unnecessary qualifications, and unused dependency declarations where they can be made meaningful without breaking cargo-hakari.
- Harden cargo-deny from advisories/bans/sources only into a complete dependency policy that includes license allowlisting, workspace-hack metadata, and justified duplicate-version policy.
- Pin lint and validation tools installed in CI, including cargo-deny and other Rust validation helpers currently fetched without a stable version.
- Update Makefile, GitHub Actions, repository rules, agent guidance, and user-facing documentation so local, CI, and release validation all describe the same lint policy.
- Preserve existing specialized gates such as `scripts/check-filesystem-boundary.sh`, rustdoc linting, mutation testing, fuzz replay, and widget warning checks while integrating the new lint-hardening workflow around them.

## Capabilities

### New Capabilities
- `rust-linting-policy`: Defines the comprehensive Rust linting, advisory lint discovery, dependency policy, tool pinning, cleanup, and documentation contract for LushText.

### Modified Capabilities
- None. Existing capabilities such as `rust-toolchain-adoption`, `internal-filesystem-abstractions`, `mutation-testing`, and `fuzz-ci-and-boundary-hardening` remain valid; this change introduces the missing repository-wide lint policy contract instead of changing their feature-specific requirements.

## Impact

- Affected code and configuration: `Cargo.toml`, crate manifests, optional `clippy.toml`, `deny.toml`, `Makefile`, `.github/workflows/*.yml`, `scripts/check-filesystem-boundary.sh`, Rust source and tests needed to satisfy newly curated lints, `.agents/rules/*.md`, root and nested `AGENTS.md` files, README or other contributor docs, and release/pre-commit validation guidance.
- Affected validation: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, rustdoc lint gate, cargo-deny with advisories/bans/sources/licenses, advisory lint report command, filesystem-boundary audit, relevant tests for cleaned code, `actionlint` for workflow edits, and `openspec validate --all --strict`.
- Affected contributors and agents: new code must satisfy a stricter blocking lint baseline, use documented exceptions with `#[expect]` rather than broad `#[allow]`, treat broad lint groups as advisory discovery inputs, and pin validation tools instead of relying on latest downloads.
