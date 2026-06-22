## 1. Audit and Classification

- [x] 1.1 Re-run focused error-type searches for public, `pub(crate)`, re-exported, service-facing, UI-facing, and cross-crate error names.
- [x] 1.2 Classify each candidate error type as keep, rename, or local-only exception with a short rationale.
- [x] 1.3 Re-run focused numeric-literal Clippy probes for low-noise literal-format lints and noisy numeric advisory lints.
- [x] 1.4 Classify numeric-literal findings as behavior policy, protocol/format constant, UI geometry/timing, harmless inline literal, test fixture data, generated-code noise, or advisory-only lint noise.

## 2. Rust Implementation

- [x] 2.1 Rename only cross-boundary error types whose current names are ambiguous at call sites, updating imports, re-exports, matches, docs, and tests.
- [x] 2.2 Keep private helper errors unchanged when the local module/function context already makes the failing workflow clear.
- [x] 2.3 Extract behavioral numeric literals into named typed constants or policy values near their owning workflow, service, model, UI module, or proof-tool module.
- [x] 2.4 Leave harmless identity/index/count/test literals inline, avoiding low-value constants that only restate a number.
- [x] 2.5 Add or adjust targeted tests when any rename or constant extraction touches behavior, limits, UI geometry, persistence policy, or user-facing messages.

## 3. Lint Policy

- [x] 3.1 Promote only clean low-noise numeric literal Clippy lints into `[workspace.lints.clippy]`.
- [x] 3.2 Keep noisy numeric lints such as `default_numeric_fallback`, `float_arithmetic`, and `integer_division` advisory unless they are cleaned without broad suppressions.
- [x] 3.3 Update `scripts/lint-advisory.py` if numeric advisory discovery needs new probes.
- [x] 3.4 Update `scripts/lint-advisory-policy.toml` with classifications and rationales for every new or changed numeric lint category.
- [x] 3.5 Avoid adding `clippy.toml` unless a global path-insensitive ban is proven safe and includes reason/replacement metadata.

## 4. Rules, Skills, and Docs

- [x] 4.1 Update `.agents/rules/rust.md` with the cross-boundary error naming and numeric-literal ownership policy.
- [x] 4.2 Update `.agents/rules/build.md` if advisory lint commands, validation wording, or lint-policy workflow changes.
- [x] 4.3 Inspect relevant local skills for Rust architecture, comments, lint-sensitive review, performance review, data safety, and OpenSpec implementation guidance.
- [x] 4.4 Update relevant skill guidance, or record why an obvious candidate skill is intentionally unchanged.
- [x] 4.5 Update the root `AGENTS.md` rules index if a rule file is added or materially changed.

## 5. Validation

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 5.3 Run `make lint-advisory` and confirm there are no unclassified findings.
- [x] 5.4 Run `make check-agent-docs`.
- [x] 5.5 Run targeted tests for any behavior, limit, UI geometry, persistence, or message changes made during implementation.
- [x] 5.6 Run `openspec validate --all --strict`.

## Skill Guidance Notes

- Updated `rust-hex-arch`, `rust-comments`, and `gtk-perf-review` so architecture, comment, and performance reviews apply the same cross-boundary error naming and numeric-literal ownership policy.
- Updated stale performance/responsiveness reference examples to use `EditorLoadError`.
- Left `data-safety` unchanged because its existing scope is data-loss invariants and automatic-mode review, not general Rust naming or numeric-literal policy.
- Left `openspec-apply-change` unchanged because the generic implementation loop already requires task-file, rule, and skill updates when a change asks for them; adding this Rust-specific checklist there would be too broad.
