## 1. Vision and governance foundation

- [ ] 1.1 Confirm `docs/next/gtk-lush.md` matches this change's specs (crate
      names, phases, gates) and fix any drift in the same commit
- [ ] 1.2 Create `crates/gtk-lush/GOVERNANCE.md` with the constitution
      checklist, exception register (empty), treadmill SLAs, publishing gates,
      bus-factor/archiving policy, and the repo-graduation plan
- [ ] 1.3 Add the reserved follow-up roadmap to `docs/next/gtk-lush.md`
      cross-checked against the governance spec's named follow-ups

## 2. Workspace scaffolding

- [ ] 2.1 Create `crates/gtk-lush/signals` and `crates/gtk-lush/settle` as
      workspace members (`gtk-lush-signals`, `gtk-lush-settle`) with SPDX
      headers, dual MIT/Apache license files, `rust-version`, README seeds,
      CHANGELOGs, and crate-level docs stating the constitution sentence
- [ ] 2.2 Wire both crates into the root workspace, `[workspace.dependencies]`,
      cargo-hakari (`cargo hakari generate`), nextest defaults, the curated
      lint table, and cargo-deny; run `make cargo-sources` if the Flatpak
      manifest inputs change
- [ ] 2.3 Add a family policy check (script under `scripts/`, wired into
      `make check-policy`) that fails on: inter-family dependencies, family
      dependencies on LushText crates, missing scaffolding files, or missing
      license metadata
- [ ] 2.4 Add `examples/standalone.rs` to each crate proving single-crate
      adoption against stock gtk-rs (compiled in CI, runnable headless)
- [ ] 2.5 Enforce `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` in
      both crates and verify `make check` passes with the empty-but-documented
      skeletons

## 3. CI integration

- [ ] 3.1 Extend the existing container CI lanes to build, clippy, doc, and
      test the family crates (unit + doctests in the non-widget lane)
- [ ] 3.2 Add an MSRV verification job building the family at the declared
      `rust-version`, with the toolchain pin recorded in workflow `env`
- [ ] 3.3 Add a `cargo-semver-checks` + public-API snapshot job in advisory
      mode, version-pinned, with a documented flip-to-blocking condition tied
      to first publication
- [ ] 3.4 Update `.agents/rules/build.md` with the new lanes and pins

## 4. gtk-lush-signals implementation

- [ ] 4.1 Write the README from the existing handler-lifetime rule prose,
      then design the public API against it (`SignalBag`, binding support,
      controller support, weak-source tolerance)
- [ ] 4.2 Implement registration/clear/drop semantics with doctests for each
      public item, including the finalized-source tolerance scenario
- [ ] 4.3 Add unit tests for ordering, double-clear idempotence, and
      weak-source upgrade failure paths; add pure logic to the mutation scope
- [ ] 4.4 Add headless widget tests (via the existing harness) proving
      disconnect-on-dispose and no-leak for shared-object handlers
- [ ] 4.5 Run the GTK warning gate over the crate's widget tests and fix any
      GLib criticals before migration starts

## 5. LushText migration to gtk-lush-signals

- [ ] 5.1 Migrate preference-binding handler bookkeeping
      (`PreferenceBindingState` and the `Drop` disconnect block) to bags;
      delete the per-field ids; full widget suite green
- [ ] 5.2 Migrate `sidebar/` and `window/` handler-id fields and their
      dispose blocks; full widget suite green
- [ ] 5.3 Migrate `editor_page/` (including minimap buffer handlers) last,
      rebasing onto current main; full widget suite plus, because these files
      are visual-sensitive, a passing `make visual-geometry-smoke`
- [ ] 5.4 Verify no `RefCell<Option<SignalHandlerId>>` bookkeeping remains in
      `ui/` outside intentional exceptions; document any exception in
      GOVERNANCE.md

## 6. gtk-lush-settle implementation

- [ ] 6.1 Write the README from the auto-dismiss-timer and reflow-burst rule
      prose; design `Debounce`, `SettleBurst` (with `pending()` and
      single-dispatch settle), and `SupersedingTimer`
- [ ] 6.2 Implement with the pure generation logic in a GLib-free module;
      doctests for every public item
- [ ] 6.3 Add unit tests plus bounded property tests for the pure logic under
      `make test-prop`; include it in the mutation scope
- [ ] 6.4 Add headless tests for weak-target cancellation and
      pending-state bracketing under the widget harness

## 7. LushText migration to gtk-lush-settle

- [ ] 7.1 Migrate status-bar auto-dismiss and notification pulse timers;
      widget tests green
- [ ] 7.2 Migrate sidebar tree loading, drafts, search scheduling, focus
      indexing, and notes timers; widget plus integration suites green
- [ ] 7.3 Migrate `editor_page` overscroll/refresh debounces and, last, the
      minimap reflow settle onto `SettleBurst`, preserving the existing
      window constants and wiring readiness predicates to `pending()`
- [ ] 7.4 Run the automation contract checks (`make check-automation-docs`,
      automation widget tests) proving readiness blocker behavior is
      unchanged, then run the full visual-geometry lane including the minimap
      pixel-anchor and animation-stream scenarios
- [ ] 7.5 Verify no hand-rolled generation-counter scheduling remains in
      `ui/`; document any exception in GOVERNANCE.md

## 8. Rules and documentation rewrite

- [ ] 8.1 Rewrite the handler-lifetime guidance in `.agents/rules/rust.md`
      and `widget-wiring.md` to require the crates and link their docs,
      keeping only LushText-specific judgment
- [ ] 8.2 Rewrite the auto-dismiss/generation-counter and reflow-burst rule
      sections the same way (`widget-wiring.md`, `ui.md`)
- [ ] 8.3 Update `README.md` (architecture overview: the gtk-lush family) and
      `AGENTS.md` (module layout) per the documentation rules
- [ ] 8.4 Run `make check-agent-docs` and `make check-automation-docs`

## 9. Name reservation and program close-out

- [ ] 9.1 Prepare `0.0.x` placeholder packages (metadata + README pointing at
      `docs/next/gtk-lush.md`, no public API) and a release checklist entry;
      publish reservations only with explicit maintainer approval recorded in
      GOVERNANCE.md
- [ ] 9.2 Run the complete repository gate set: `make check`, full widget
      suite, non-widget suites, `make test-prop`, mutation smoke over the new
      pure logic, and `make visual-geometry-smoke` matching the final diff
- [ ] 9.3 Audit the final diff against the constitution checklist and record
      the audit in GOVERNANCE.md as the program's first review entry
- [ ] 9.4 Verify each reserved follow-up change name appears in both the
      vision document and the governance spec, so the next agent can propose
      Phase 0/3/4/5/6 changes without re-deriving scope
