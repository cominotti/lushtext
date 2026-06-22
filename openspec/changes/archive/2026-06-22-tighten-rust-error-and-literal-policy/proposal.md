## Why

The codebase already has strong Rust lint governance, but the recent audit found two areas where future changes can drift: typed error names can become too generic across module boundaries, and behavioral numeric literals can hide policy when they remain inline. This change tightens those conventions without turning broad Clippy groups into noisy blocking gates.

## What Changes

- Define a repository policy for Rust error type names that favors workflow/domain-specific names when errors cross module, service, UI, or crate boundaries.
- Clarify when numeric literals must become named, typed constants or small policy values, and when inline literals such as indexes, counts, and obvious identities are acceptable.
- Promote only proven, low-noise numeric literal Clippy lints after cleanup, while keeping broad or noisy lints advisory.
- Update `.agents/rules` and relevant local skills so future Rust, architecture, lint-policy, and comment reviews apply the same error-naming and numeric-literal policy.
- Keep `clippy.toml` optional and avoid creating one unless a global, path-insensitive Clippy ban is genuinely safe and includes replacement guidance.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rust-linting-policy`: add requirements for error-type naming, numeric-literal policy, curated numeric lint promotion, and synchronized agent guidance.

## Impact

- Affected specs: `openspec/specs/rust-linting-policy/spec.md`.
- Affected guidance: `.agents/rules/rust.md`, `.agents/rules/build.md` if advisory lint commands change, root `AGENTS.md` rules index only if a rule file is materially changed, and relevant `.agents/skills/*/SKILL.md` files for Rust architecture/comment/lint-sensitive work.
- Affected code/config: selected Rust error type names, selected numeric policy constants, root `Cargo.toml` workspace lint table, `scripts/lint-advisory.py`, and `scripts/lint-advisory-policy.toml` if new advisory or blocking numeric lints are added.
- Validation: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `make lint-advisory`, `make check-agent-docs`, and `openspec validate --all --strict`.
