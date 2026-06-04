## Context

LushText currently pins Rust 1.95.0 in the workspace `rust-version`, `rust-toolchain.toml`, GitHub Actions stable lanes, Snap's rustup bootstrap, README, AGENTS, and agent rules/skills. The codebase already uses Edition 2024 and several Rust 1.95 idioms, including if-let match guards, atomic `try_update`, `array_windows`, and `cfg_select!` guidance.

Rust 1.96.0 was released on 2026-05-28. Official release sources identify these relevant changes:

- New `core::range` types, including copyable `Range`, with range syntax still producing the legacy `core::ops`/`std::ops` range types until a future edition: <https://blog.rust-lang.org/2026/05/28/Rust-1.96.0/> and <https://doc.rust-lang.org/stable/core/range/struct.Range.html>
- New `assert_matches!` and `debug_assert_matches!` macros that are not in the prelude and must be explicitly imported: <https://doc.rust-lang.org/stable/std/macro.assert_matches.html>
- Cargo fixes for CVE-2026-5222 and CVE-2026-5223, both relevant to third-party registries rather than crates.io-only workflows: <https://blog.rust-lang.org/2026/05/25/cve-2026-5222/> and <https://blog.rust-lang.org/2026/05/25/cve-2026-5223/>
- Cargo support for git dependencies that also declare an alternate registry, target cfg rustdoc flags, rustdoc rendering changes, and compatibility notes around wasm linker strictness and other edge cases: <https://doc.rust-lang.org/nightly/releases.html#version-1960-2026-05-28>
- Clippy 1.96 adds `manual_noop_waker`, `manual_option_zip`, and `manual_pop_if`, along with false-positive fixes and improvements: <https://raw.githubusercontent.com/rust-lang/rust-clippy/master/CHANGELOG.md>

Local exploration already installed the Rust 1.96.0 toolchain and proved the current tree against the most relevant stable gates:

```text
cargo +1.96.0 fmt --all -- --check
cargo +1.96.0 clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo +1.96.0 doc --workspace --no-deps
cargo +1.96.0 test -p lushtext-core --lib --quiet
cargo +1.96.0 clippy --workspace --all-targets -- -W clippy::manual_option_zip -W clippy::manual_pop_if -W clippy::manual_noop_waker -D warnings
```

All of those probes passed. The core test probe reported 498 tests passed.

The adoption surface is cross-cutting:

```text
Rust 1.96.0
     |
     +-- workspace Cargo.toml rust-version
     +-- rust-toolchain.toml
     +-- GitHub Actions stable lanes
     +-- Snap rustup bootstrap
     +-- README / AGENTS tech-stack text
     +-- .agents/rules/build.md and rust.md
     +-- Rust-quality skills and their subagent prompts
     +-- targeted code idioms and Clippy lints
```

## Goals / Non-Goals

**Goals:**

- Move the stable Rust floor from 1.95.0 to 1.96.0 everywhere the repo pins or documents that floor.
- Keep Edition 2024 and the existing GNOME 50 / gtk-rs dependency floor unchanged.
- Use the Cargo security fixes as part of the rationale while confirming the repo does not rely on third-party registries today.
- Add Rust 1.96 idioms only where they improve diagnostics, correctness, or readability.
- Update agent rules and skills so future reviews know the new MSRV and selected 1.96 patterns.
- Validate with the 1.96 stable toolchain before marking the change complete.

**Non-Goals:**

- No dependency update sweep, `cargo update`, gtk-rs bump, GNOME runtime bump, or Flatpak vendoring change unless validation reveals a strict need.
- No broad mechanical rewrite of every `std::ops::Range` or range expression.
- No nightly-only Rust feature adoption.
- No user-visible editor behavior change.
- No WASM target support work; the release's wasm linker compatibility note is not relevant to LushText's GTK desktop targets.

## Decisions

### Decision: Bump the floor consistently instead of using `stable`

Keep explicit `1.96.0` pins in the same surfaces that currently pin `1.95.0`: workspace package metadata, rustup toolchain file, GitHub Actions stable jobs, and Snap's bootstrap command. This preserves reproducibility and keeps Clippy/rustfmt behavior deterministic across local and CI runs.

Alternative considered: switch CI to `stable`. Rejected because this repo treats Clippy and rustfmt as hard gates. A floating channel would make formatting and lint failures arrive as surprise maintenance instead of planned toolchain adoption.

### Decision: Treat Cargo CVEs as a supply-chain hygiene reason, not as an app behavior change

LushText appears to use crates.io and vendored Flatpak sources, not a custom third-party registry. The CVE fixes therefore do not require application code changes, but they are still a good reason to avoid staying on an affected Cargo release. The implementation should confirm there are no alternate registries in `.cargo`, manifests, or workflow configuration, then record that the 1.96 Cargo fixes are inherited by the pinned toolchain.

Alternative considered: add new dependency-policy tooling just for the CVEs. Rejected because `cargo deny check advisories bans sources` already owns dependency policy, and the vulnerability is in Cargo's registry handling rather than in a dependency crate.

### Decision: Adopt `assert_matches!` in tests first

The new macros improve failure output over `assert!(matches!(...))`. The codebase has a small number of direct `assert!(matches!(...))` sites, mostly tests. These are good low-risk adoption targets when the asserted value implements `Debug`.

The macros are intentionally not in the prelude, so every edited test module must import `std::assert_matches` or `std::debug_assert_matches` explicitly. Do not introduce an app-local wrapper macro.

### Decision: Use new `core::range` types only for named span values with real Copy benefit

`core::range::Range` is useful when LushText stores byte spans as named values and later needs start/end access, slicing, comparison, or cheap reuse. Search and Markdown preview code contains such span-like values, but range syntax still creates the legacy range type today, and property-test strategies naturally generate legacy ranges. The implementation should therefore audit before migrating:

- Good candidates: internal span structs or fields where `Copy` removes clones or avoids moving a range that is still needed.
- Keep legacy ranges: generated strategies, APIs that accept range syntax directly, or places where `.into()`/`Range::from(...)` noise is worse than the clone it removes.
- Public-ish boundaries should prefer `impl RangeBounds<usize>` when accepting caller-provided ranges, if such a boundary is introduced or touched.

Alternative considered: rewrite all `std::ops::Range<usize>` imports to `core::range::Range`. Rejected because it would force explicit conversions while range syntax remains legacy, creating noise without improving behavior.

### Decision: Add only clean, relevant Clippy 1.96 lints to the workspace policy

The explicit 1.96 pass with `manual_option_zip`, `manual_pop_if`, and `manual_noop_waker` produced no warnings. These lints can therefore be added as deny-level workspace lints if their MSRV behavior is compatible with 1.96.0 and their suggestions are available on stable. This makes future code inherit the new quality floor without creating a migration backlog.

Alternative considered: enable a broad Clippy group such as `complexity` as deny. Rejected because the repo already maintains a carefully curated lint table, and broad groups can change meaning across Clippy releases.

### Decision: Update agent guidance in the same change

The repo's rules and skills currently teach Rust 1.95.0-specific guidance. This change must update those references alongside code/config so future automated reviews do not regress to stale MSRV assumptions. The minimum set is:

- `.agents/rules/build.md` for pinning and validation surfaces.
- `.agents/rules/rust.md` for 1.96 idioms, range cautions, and new lints.
- `.agents/skills/gtk-perf-rust-optimize/SKILL.md` for modern Rust audit prompts.
- `.agents/skills/gtk-perf-review/SKILL.md` only if it embeds MSRV-sensitive prompts or delegates to the Rust optimize skill in a way that needs wording updates.
- README and AGENTS tech-stack/version references.
- Snap comments that name the old MSRV.

## Risks / Trade-offs

- [Risk] A hidden stable lane remains pinned to 1.95.0. Mitigation: use `rg "1\\.95\\.0|MSRV"` after edits and require only historical/archive references or intentionally unrelated text to remain.
- [Risk] `core::range::Range` migration introduces conversion noise or conflicts with property-test generation. Mitigation: require a small before/after audit and allow "no migration" as a valid result if every candidate becomes noisier.
- [Risk] New Clippy lints become annoying if suggestions do not fit GTK or async patterns. Mitigation: add only the explicit clean lints, keep the lint table curated, and require `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings`.
- [Risk] Flatpak's `rust-stable` extension might lag the repo's MSRV. Mitigation: keep the manifest on the stable extension but run/require Flatpak manifest validation and document a blocker if the GNOME SDK extension cannot satisfy Rust 1.96.0.
- [Risk] Toolchain bump is mistaken for a user-facing feature. Mitigation: no README feature-list changes except version/build guidance, and no app behavior changes unless validation forces a compatibility fix.

## Migration Plan

1. Update all Rust 1.95.0 pins and documentation references to 1.96.0, leaving nightly fuzz tooling unchanged.
2. Confirm no third-party registry configuration requires additional Cargo advisory mitigation.
3. Add clean Clippy 1.96 lints to the workspace lint table.
4. Replace direct `assert!(matches!(...))` / `debug_assert!(matches!(...))` sites where the macro improves failure output.
5. Audit named byte-span range types and migrate only the sites where `core::range::Range` gives a clear Copy/readability benefit.
6. Update agent rules and skills to teach the new floor and the selective adoption policy.
7. Run the 1.96 validation stack and file-specific packaging/docs checks required by touched files.

Rollback is a normal revert of the implementation commit. No user data or persisted file format changes are involved.

## Open Questions

- None. The exploratory 1.96 probes passed, and any range migration can validly conclude with "no code change" if the audit finds no net readability win.
