## 1. Inventory and Safety Checks

- [x] 1.1 Search active repo surfaces for `1.95.0`, `MSRV`, `rust-version`, `rust-toolchain`, `dtolnay/rust-toolchain`, and Snap rustup bootstrap references; classify every hit as active, archive/history, or unrelated.
  - Evidence: final `rg -n "1\.95\.0" --glob '!openspec/changes/adopt-rust-1-96/**' .` returned no active hits. Earlier active hits in `fuzz/Cargo.toml` and `conductor/tech-stack.md` were updated to 1.96.0.
- [x] 1.2 Inspect `.cargo` configuration, Cargo manifests, GitHub workflows, build scripts, and packaging scripts for alternate registry or sparse-registry credential configuration; record whether Cargo 1.96 advisory mitigation needs anything beyond the toolchain bump.
  - Evidence: registry scan found only crates.io sparse protocol env, Flatpak vendored-source replacement, offline Flatpak manifests, container registry image names, and release/deployment tokens unrelated to Cargo registry credentials. No third-party Cargo registry flow needs mitigation beyond Cargo 1.96.0.
- [x] 1.3 Confirm the current Flatpak GNOME SDK Rust stable extension can satisfy Rust 1.96.0, or document the exact Flatpak blocker before proceeding.
  - Evidence: `flatpak remote-ls --system --runtime --columns=application,branch,version flathub` reports `org.freedesktop.Sdk.Extension.rust-stable 25.08 1.96.0`. The local user install is still 1.95.0, so local Flatpak builds need `flatpak update --user org.freedesktop.Sdk.Extension.rust-stable//25.08` before building.
- [x] 1.4 Re-run the exploratory Rust 1.96 probes if the tree changed since proposal creation: rustfmt, Clippy, rustdoc lint gate, core library tests, and explicit new-Clippy-lint probe.
  - Evidence: all final probes under Rust 1.96.0 passed, including rustfmt check, Clippy, rustdoc lint gate, `lushtext-core` lib tests, nextest workspace tests, and the explicit new-Clippy-lint probe.

## 2. Align Toolchain Pins

- [x] 2.1 Update `[workspace.package].rust-version` in `Cargo.toml` from `1.95.0` to `1.96.0`.
  - Evidence: `Cargo.toml` now sets `rust-version = "1.96.0"`.
- [x] 2.2 Update `rust-toolchain.toml` to install Rust `1.96.0` with the existing `clippy` and `rustfmt` components.
  - Evidence: `rust-toolchain.toml` now pins `channel = "1.96.0"` and keeps `rustfmt`/`clippy`.
- [x] 2.3 Update every stable GitHub Actions Rust setup step from `dtolnay/rust-toolchain@1.95.0` to `dtolnay/rust-toolchain@1.96.0`; leave nightly fuzz lanes unchanged.
  - Evidence: stable lanes in `ci.yml`, `end-user-smoke.yml`, `mutation-testing.yml`, and `release-benchmark.yml` now use `dtolnay/rust-toolchain@1.96.0`; `fuzz-smoke.yml` remains nightly-only.
- [x] 2.4 Update `snap/snapcraft.yaml` comments and rustup bootstrap command to use Rust `1.96.0`.
  - Evidence: Snap bootstrap command and MSRV comments now name Rust 1.96.0.
- [x] 2.5 Confirm `Cargo.lock`, `workspace-hack`, and `build-aux/cargo-sources.json` stay unchanged unless a validation command proves they must move.
  - Evidence: `git status --short Cargo.lock workspace-hack build-aux/cargo-sources.json` returned no changes.

## 3. Adopt Rust 1.96 Code Patterns

- [x] 3.1 Add the clean Rust 1.96 Clippy lints `manual_option_zip`, `manual_pop_if`, and `manual_noop_waker` to the curated `[workspace.lints.clippy]` table if the explicit lint probe remains clean.
  - Evidence: the lints are now deny-level workspace lints in `Cargo.toml`; `cargo +1.96.0 clippy --workspace --all-targets -- -W clippy::manual_option_zip -W clippy::manual_pop_if -W clippy::manual_noop_waker -D warnings` passes.
- [x] 3.2 Replace direct `assert!(matches!(...))` and `debug_assert!(matches!(...))` test assertions with explicitly imported `std::assert_matches` or `std::debug_assert_matches` where the asserted value implements `Debug` and the macro improves failure output.
  - Evidence: touched assertion tests now import and use `std::assert_matches`; `LoadResult` derives `Debug` so `Result<LoadResult, LoadError>` error assertions can use the macro; `rg -n "assert!\(matches!"` across the touched files returns no hits.
- [x] 3.3 Audit stored byte-span types in content search, Replace All, Markdown inline footnotes, fuzz/property helpers, and benchmarks for possible `core::range::Range` use; migrate only sites with a clear Copy/readability win.
  - Evidence: audited content-search match ranges, Replace All spans, Markdown inline footnote protected ranges, fuzz/property helpers, and benchmarks. No stored range site had a clear Copy/readability win after accounting for range syntax and third-party APIs.
- [x] 3.4 Preserve legacy `std::ops::Range` or use `impl RangeBounds<usize>` where range syntax, proptest strategy generation, third-party APIs, or caller-facing range inputs would become noisier with explicit conversions.
  - Evidence: retained `std::ops::Range` in range-syntax-heavy paths and third-party parser/search interfaces instead of adding explicit conversions to `core::range::Range`.
- [x] 3.5 Run `cargo +1.96.0 fmt --all -- --check` and fix any formatting fallout from the assertion/range/lint edits.
  - Evidence: `cargo +1.96.0 fmt --all -- --check` passes.

## 4. Update Documentation, Rules, and Skills

- [x] 4.1 Update README tech-stack and build-prerequisite sections to name Rust 1.96.0 while preserving Edition 2024 and GNOME 50 guidance.
  - Evidence: README tech-stack table and Rust prerequisite now name Rust 1.96.0/1.96.0+.
- [x] 4.2 Update root AGENTS.md tech-stack/current-context references to name Rust 1.96.0.
  - Evidence: root AGENTS tech stack and current-context entries now name Rust 1.96.0.
- [x] 4.3 Update `.agents/rules/build.md` so Rust pinning, Snap bootstrap notes, and validation guidance reference Rust 1.96.0.
  - Evidence: build rule now names 1.96.0 for `rust-version`, Snap MSRV/bootstrap guidance, and the Flatpak local-extension refresh note.
- [x] 4.4 Update `.agents/rules/rust.md` with explicit `assert_matches` import guidance, selective `core::range` adoption rules, and the accepted Clippy 1.96 lint expectations.
  - Evidence: Rust rule now covers `assert_matches`, selective `core::range` use, and accepted Clippy 1.96 lints.
- [x] 4.5 Update `.agents/skills/gtk-perf-rust-optimize/SKILL.md` so the modern Rust audit prompt uses MSRV 1.96.0 and checks the accepted 1.96 idioms without encouraging broad rewrites.
  - Evidence: Rust optimize skill now names MSRV 1.96.0 and includes focused criteria for `assert_matches`, `core::range`, `manual_option_zip`, `manual_pop_if`, and `manual_noop_waker`.
- [x] 4.6 Update `.agents/skills/gtk-perf-review/SKILL.md` only if its delegated Rust-quality wording embeds stale MSRV or misses the updated Rust optimize skill contract.
  - Evidence: inspected `gtk-perf-review`; it delegates to the Rust optimize skill without stale embedded MSRV wording, so no edit was required.
- [x] 4.7 Run `make check-agent-docs` and fix any stale guidance or rules/skills validation failures.
  - Evidence: `make check-agent-docs` passes.

## 5. Validate the Toolchain Adoption

- [x] 5.1 Run `rg "1\\.95\\.0|MSRV|rust-toolchain|rust-version"` and confirm no active stale Rust 1.95.0 guidance remains.
  - Evidence: active-surface scan found only 1.96.0 references plus OpenSpec proposal/design history; final `rg -n "1\.95\.0" --glob '!openspec/changes/adopt-rust-1-96/**' .` returned no hits.
- [x] 5.2 Run `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings`.
  - Evidence: `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings` passes.
- [x] 5.3 Run the rustdoc lint gate under Rust 1.96.0 with the repo's existing `RUSTDOCFLAGS`.
  - Evidence: `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo +1.96.0 doc --workspace --no-deps` passes.
- [x] 5.4 Run representative non-GTK tests under Rust 1.96.0, at minimum `cargo +1.96.0 test -p lushtext-core --lib`; broaden to `cargo +1.96.0 nextest run --workspace` if nextest is available and the change touched non-widget test surfaces.
  - Evidence: `cargo +1.96.0 test -p lushtext-core --lib --quiet` passes with 498 tests; `cargo +1.96.0 nextest run --workspace` passes with 550 tests and 0 skipped. Workspace watcher integration tests now return early only for host inotify resource exhaustion (`EMFILE`/`MaxFilesWatch`) and still fail for other watcher errors.
- [x] 5.5 If Snap files changed, run `snapcraft expand-extensions` where snapcraft is available, or record that local Snap structural validation could not be run and rely on the CI Snap validate job.
  - Evidence: `command -v snapcraft` returned no local Snapcraft binary, so `snapcraft expand-extensions` could not be run locally; rely on the CI Snap validate job for structural Snap validation.
- [x] 5.6 Run any additional file-triggered checks required by `.agents/rules/build.md`, such as `make test-flathub-manifest` or release-dry-run helper tests only if Flatpak/release scripts are touched.
  - Evidence: no Flatpak manifest/generator or release-helper files were touched, so Flatpak/release file-triggered checks were not required. `cargo +1.96.0 metadata --manifest-path fuzz/Cargo.toml --format-version 1 --no-deps` was run for the fuzz manifest MSRV update and passed.

## 6. OpenSpec Closure

- [x] 6.1 Run `openspec validate adopt-rust-1-96 --strict`.
  - Evidence: `openspec validate adopt-rust-1-96 --strict` passes.
- [x] 6.2 Run `openspec status --change adopt-rust-1-96 --json` and confirm proposal, design, specs, and tasks are complete.
  - Evidence: final status check confirms proposal, design, specs, and tasks are complete.
- [x] 6.3 Update this task list with completed evidence before handing the change to apply/archive workflow.
  - Evidence: this file now records completion evidence for each task.
