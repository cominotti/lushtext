# Tasks: Close Consistency And Decomposition Gaps

## 1. Hygiene one-liners and dead code

- [x] 1.1 Add a `tracing::warn!` (path + error) to the silent cancel-path
      temp-item cleanup in `ui/sidebar/workspace_section/actions.rs:364-366`,
      matching the diagnostic level of the sibling rename-failure path.
- [x] 1.2 Replace the poisoning `.expect()` on the session save-ordering lock
      (`services/session_service.rs:87`) with the existing `lock_unpoisoned`
      recovery helper; update the `# Panics` doc section; add a unit test that
      poisons the lock and proves a later save still completes with correct
      ordering state.
- [x] 1.3 For each of the 10 unreferenced pub fns (`ui/window/drafts.rs:1240`,
      `services/local_history_service.rs:475`, `ui/editor_page/mod.rs:276`,
      `services/recovery_metadata.rs:360,366,478`,
      `ui/command_palette/mod.rs:386`,
      `ui/sidebar/workspace_section/refresh.rs:78`,
      `ui/markdown_preview/mod.rs:253`, `model/automation.rs:1241`): check the
      history of the test that referenced it; either resurrect the missing
      test or delete the hook. Record the per-hook decision in the commit
      message.
- [x] 1.4 Convert the boolean-flag one-shot timer in
      `ui/sidebar/workspaces.rs:179` to `gtk_lush_settle::SupersedingTimer`,
      preserving the 300ms window and re-arm semantics.
- [x] 1.5 Extract the two byte-similar fire-and-forget cleanup spawn blocks in
      `ui/sidebar/workspace_section/actions.rs:325/:362` into one local
      helper; keep the documented rationale for bypassing
      `spawn_blocking_then`.
- [x] 1.6 Group the ~12 loose `*_generation` fields on
      `ui/editor_page/imp.rs` into small named structs by workflow ownership
      (pure field moves; no semantic change); run editor_page widget tests.

## 2. Typed ownership: buffer replacement pairing

- [x] 2.1 Restructure `ui/editor_page/buffer_replacement.rs` so a guarded
      cancellation callback cannot pair with a plain body (generic body-kind
      parameter preferred; two concrete request types over a private core as
      the pre-approved fallback); delete the `unreachable!` arm at line 68.
- [x] 2.2 Update the five workflow call sites (memory eviction, draft
      recovery, local-history restore/undo, save formatting) to the new
      construction surface; preserve `Default`/`mem::take` placeholder
      behavior on the plain side.
- [x] 2.3 Add a compile-fail-shaped proof (doc comment + type-level test or
      trybuild-free static assertion pattern already used in the repo) or unit
      coverage demonstrating the mismatch is unconstructible; run replacement
      session unit + widget tests including synchronous reentrant
      supersession.

## 3. Typed ownership: scope-owned admission charges

- [x] 3.1 Promote the `with_construction_charge` scope-guard shape from
      `services/palette/notes.rs` to a shared palette-internal helper
      parameterized over ledger charge/release/consume operations.
- [x] 3.2 Convert `FileIndexBuildLedger` scratch/installed handling in
      `services/palette/index.rs` traversal to the scope-owned guard;
      eliminate the 8+ scattered manual `release_scratch`/`release_installed`
      calls; keep truncation vocabulary, byte ceilings, and
      peak/high-water evidence unchanged.
- [x] 3.3 Convert the residual manual paired releases in `notes.rs`
      (canonical-folder bytes at :616-618, live-identity bytes at :756) to
      scope ownership; keep `admit_parsed_sidecar`'s direct settlement as a
      documented consume path through the guard.
- [x] 3.4 Extend ledger unit tests with new-early-exit leak proofs for the
      file-index ledger (filtered batch, budget rejection, supersession,
      error propagation), asserting exactly-once release and restored
      pre-item accounting.

## 4. Shared single-flight coordination

- [x] 4.1 Move the generic coordinator + cancellation token from
      `services/palette/runtime.rs` to a workflow-neutral home
      (e.g. `services/single_flight.rs`); rename
      `PaletteSearch{Coordinator,Cancellation,...}` to workflow-neutral names
      and update all in-tree consumers (command palette, notes browser,
      bookmark excerpts).
- [x] 4.2 Converge `model/search_flight.rs` on the shared coordinator: thin
      wrapper preserving `Supersede { active_generation }` evidence, or
      direct consumer migration if the evidence maps 1:1; existing
      search-flight unit tests must pass unmodified (wrapper) or with
      mechanical-only updates (migration), with the decision recorded.
- [x] 4.3 Replace `LocalHistoryPreviewCancellation`
      (`services/local_history_service.rs:104`) with an alias of the shared
      token, following the `bookmark_excerpt.rs:174` precedent.
- [x] 4.4 Extract one UI-side helper for the guarded worker-outcome
      weight→shrink→own sequence and adopt it in
      `ui/window/focus_indexing.rs:43`, `ui/window/notes/browser.rs:166`, and
      `ui/window/local_history.rs:82` — each workflow keeps its own outcome
      enum and explicit freshness checks; document any site that cannot
      adopt it.
- [x] 4.5 Run the full supersession/cancellation-observability test set for
      palette, notes browser, bookmark excerpts, local-history preview, and
      workspace search; assertions must not weaken.

## 5. Keyed fault seam

- [x] 5.1 Convert `FAIL_REPLACE_BEFORE_RENAME_PATH`
      (`services/content_search/replace.rs:56-85`) to the per-target keyed
      `BTreeMap` registry with `#[must_use]` cleanup ownership, matching the
      sibling after-metadata hook; keep path-matched consumption semantics.
- [x] 5.2 Add a parallel-registration test proving two targets can arm
      before-rename failures concurrently without clobbering, and that the
      registry is empty after cleanup.

## 6. Widget-test presentation helper

- [x] 6.1 Add the unified `present_window` (present + ≥5s
      allocation/realization wait + drain) to
      `crates/lushtext/tests/widget/common.rs`; delete the three local copies
      in `sidebar.rs:404`, `command_palette.rs:77`, `window.rs:573` and update
      call sites (window.rs keeps its `flush_after_delay` as a caller-side
      addition where needed).
- [x] 6.2 Rerun sidebar widget tests in isolation and under load per flake
      discipline — sidebar gains a realization wait it never had; any newly
      exposed timing assumption is fixed, not bypassed.
- [x] 6.3 Update `.agents/rules/widget-wiring.md` so the shared-helper claim
      names the real home and includes `present_window`.

## 7. Callbacks-under-borrow sweep

- [x] 7.1 Convert callback invocation to clone-then-call at
      `ui/editor_page/load_save.rs:828`, `ui/editor_page/bookmarks.rs:326/:426`,
      `ui/sidebar/workspace_section/actions.rs:311-315`, and
      `ui/sidebar/mod.rs:295/:311`, following the `minimap.rs:459` shape;
      preserve invocation order.
- [x] 7.2 Add a regression test that re-enters callback registration from
      inside a file-loaded callback and proves no borrow panic.

## 8. Shared smoke warning classification

- [x] 8.1 Add the shared Gdk broken-pipe warning family to
      `scripts/accessibility_warning_allowlist.py` (or a sibling shared
      module with a name reflecting cross-lane scope) with shared-vs-lane
      ownership documented in the module docstring.
- [x] 8.2 Import the shared classifier from
      `scripts/crash-recovery-smoke-driver.py`,
      `scripts/automation-smoke-driver.py`,
      `scripts/visual-geometry-smoke.py`, `scripts/run-visual-smoke.sh`, and
      `scripts/compare-blueprint-visuals.sh`; remove the five hand-rolled
      copies; keep lane-specific patterns lane-owned.
- [x] 8.3 Verify fingerprint/policy integration: the shared module stays in
      the accessibility source-fingerprint set; run
      `make check-accessibility-policy` and each touched lane's self-checks
      or a bounded smoke run where host support allows.

## 9. Markdown preview decomposition

- [x] 9.1 Extract the image admission/decode/apply/retire pipeline from
      `ui/markdown_preview/mod.rs` into a sibling module (pure code movement;
      no logic edits in the same commit).
- [x] 9.2 Extract table building (`BufferedTableBuilder`,
      `TableCellMarkupBuilder`) into a sibling module.
- [x] 9.3 Extract code-block theming and the documented idle+timeout repair
      mechanism into a sibling module, preserving the exact SourceId
      pair/cancellation/completion mechanism.
- [x] 9.4 Extract footnote/link handling into a sibling module (or fold into
      the nearest cohesive sibling if extraction would create
      cross-referencing fragments — record the decision).
- [x] 9.5 Confirm `mod.rs` retains the public wrapper, template contract,
      render orchestration, and trait impls only; no trait impl is split
      across files; production line count of `mod.rs` is materially reduced
      and each sibling is single-responsibility.
- [x] 9.6 Run markdown widget tests, the relevant visual lanes, and
      `make check-visual-proof-policy`; if the diff is classified
      visual-sensitive, run `make visual-geometry-smoke` and keep the proof
      artifacts.

## 10. Documentation and closeout

- [x] 10.1 Add a "Coordination vocabulary" glossary (Admission, Budget,
      Coordinator, Ledger, Retirement, Continuation, generation-counter
      conventions) to `.agents/rules/rust.md`.
- [x] 10.2 Update `AGENTS.md` and `README.md` module layout for the
      markdown_preview split and the coordinator's new home; update any rules
      references to renamed types.
- [x] 10.3 Run the full gate stack: `make check`, `make check-policy`,
      `make test`, widget lanes via `scripts/run-widget-tests.sh`,
      `make check-agent-docs` (scripts/rules changed), and confirm zero new
      runtime warnings via a `make run` exercise of preview, sidebar, and
      replace workflows.
      Evidence: `make check` clean (0 warnings, clippy `-D warnings`, rustfmt,
      policy audits); `make check-policy` passed; `make test` = 1416 non-widget
      passed / 11 skipped + widget suite "all tests passed" (no `FLAKY:`);
      `make check-agent-docs` passed (14 skills validated). The interactive
      `make run` warning sweep is not runnable in this headless session; it was
      substituted by the real-app-under-Mutter warning scans in
      `make accessibility-smoke` (preview render), `make visual-smoke` (preview
      + sidebar/workspace cases), and `make visual-geometry-smoke` — all passed
      with clean GTK/GDK/Adwaita/GIO/D-Bus/portal/AT-SPI warning scans. The
      decomposition is pure code movement with no GtkPaned/Revealer/animation
      changes, so the interactive-only warning classes (pixman zero-rect, paned
      measure) are not a risk vector for this change.
- [x] 10.4 Validate the change with strict OpenSpec validation and archive
      readiness: every spec delta requirement has landed evidence, Non-Goals
      were respected (no drive-by refactors), and any discovered
      out-of-scope finding is recorded as a candidate future proposal instead
      of expanding this one.
      Evidence: `openspec validate close-consistency-and-decomposition-gaps
      --strict` → "is valid"; all 6 capabilities' requirements map to landed
      sections 1–9; the four "Explicitly out of scope" Non-Goals (DisposalOwned
      internals, other 24 cohesive files, `retained_byte_weight` trait,
      vocabulary shrink) were untouched; the one out-of-scope finding (default
      `make check --all-features` masking default-feature `unused_imports`) is
      recorded as a candidate future proposal in
      `docs/next/default-feature-lint-gate.md`.
