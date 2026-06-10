## 1. Vision and governance foundation

- [x] 1.1 Confirm `docs/next/gtk-lush.md` matches this change's specs (crate
      names, phases, gates) and fix any drift in the same commit
- [x] 1.2 Create `crates/gtk-lush/GOVERNANCE.md` with the constitution
      checklist, exception register (empty), treadmill SLAs, publishing gates,
      bus-factor/archiving policy, and the repo-graduation plan
- [x] 1.3 Add the reserved follow-up roadmap to `docs/next/gtk-lush.md`
      cross-checked against the governance spec's named follow-ups, including
      `extract-gtk-lush-signals-and-settle`

## 2. Workspace scaffolding

- [x] 2.1 Create `crates/gtk-lush/signals` and `crates/gtk-lush/settle` as
      workspace members (`gtk-lush-signals`, `gtk-lush-settle`) with SPDX
      headers, dual MIT/Apache license files, `rust-version`, README seeds,
      CHANGELOGs, and crate-level docs stating the constitution sentence
- [x] 2.2 Wire both crates into the root workspace, `[workspace.dependencies]`,
      cargo-hakari (`cargo hakari generate`), nextest defaults, the curated
      lint table, and cargo-deny; run `make cargo-sources` if the Flatpak
      manifest inputs change
- [x] 2.3 Add a family policy check (script under `scripts/`, wired into
      `make check-policy`) that fails on: inter-family dependencies, family
      dependencies on LushText crates, missing scaffolding files, or missing
      license metadata
- [x] 2.4 Add `examples/standalone.rs` to each crate proving single-crate
      adoption against stock gtk-rs (compiled in CI, runnable headless)
- [x] 2.5 Enforce `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` in
      both crates and verify `make check` passes with the empty-but-documented
      skeletons

## 3. CI integration

- [x] 3.1 Extend the existing container CI lanes to build, clippy, doc, and
      test the family crates (unit + doctests in the non-widget lane)
- [x] 3.2 Add an MSRV verification job building the family at the declared
      `rust-version`, with the toolchain pin recorded in workflow `env`
- [x] 3.3 Add a `cargo-semver-checks` + public-API snapshot job in advisory
      mode, version-pinned, with a documented flip-to-blocking condition tied
      to first publication
- [x] 3.4 Update `.agents/rules/build.md` with the new lanes and pins

## 4. Foundation documentation

- [x] 4.1 Update `README.md` (architecture overview: the gtk-lush family) and
      `AGENTS.md` (module layout) per the documentation rules
- [x] 4.2 Keep handler-lifetime and generation-counter rule rewrites out of
      this change except for links to the placeholder roadmap; the dedicated
      `extract-gtk-lush-signals-and-settle` follow-up rewrites those rules
      after the crates enforce them
- [x] 4.3 Run `make check-agent-docs`

## 5. Name reservation and program close-out

- [x] 5.1 Prepare `0.0.0` placeholder packages (metadata + README pointing at
      `docs/next/gtk-lush.md`, no public API) and a release checklist entry;
      publish reservations only with explicit maintainer approval recorded in
      GOVERNANCE.md
- [x] 5.2 Run the foundation gate set: `make check`, family crate doctests,
      standalone examples, policy checks, MSRV verification, and advisory
      semver/public-API jobs; run widget or visual-geometry lanes only if the
      final diff touches runtime or visual-sensitive files
- [x] 5.3 Audit the final diff against the constitution checklist and record
      the audit in GOVERNANCE.md as the program's first review entry
- [x] 5.4 Verify each reserved follow-up change name appears in both the
      vision document and the governance spec, so the next agent can propose
      Phase 0/2/3/4/5/6 changes without re-deriving scope
