## 1. Programme record and stale docs

- [x] 1.1 Create `docs/next/formal-verification.md`, covering:
  - the motivation: gap A (wrong model of the environment) versus gap B (code
    against spec);
  - the pragmatic tool mix chosen by the maintainer on 2026-09-23;
  - phases 0–5 with posture, scope, and exit criteria;
  - the GTK axiom list A1–A13 with current pinning status;
  - the durable-write POSIX axioms A1–A7;
  - the draft invariants S1–S4 and L1;
  - the deferral inventory.
  This was written during the explore session that proposed this change.
- [x] 1.2 Update `docs/next/draft-mtime-validation.md`:
  - its status says "Deferred" but the behaviour has shipped, so mark it
    superseded;
  - point it at this change and at `draft-restore-validation`.
- [x] 1.3 Add a pointer to the programme record from the `docs/next/` index or
  from AGENTS.md, wherever planned-work records are listed.

## 2. Draft journal wedge (D1, D2)

- [x] 2.1 Write a failing integration test in
  `crates/lushtext/tests/integration/draft.rs`:
  - leave a stable-path-hash body without an entry, as a crash between the
    body write and the batch commit would;
  - assert that the next `update_manifest` returns `Ok`;
  - assert that the draft is recoverable.
  Today the test fails with `Partial`.
- [x] 2.2 Write a failing test for a pass with two new file-backed drafts that
  crashes after both bodies and before the commit. Assert that both bodies are
  recoverable.
- [x] 2.3 Implement write-ahead registration (D1). One batched `update_manifest`
  registers every id absent from the persisted manifest before its body write.
  It carries the path and mtime captured at snapshot, and ids that are already
  registered skip it. Keep pure admission and sequencing decisions in
  `ui/window/drafts/policy.rs` and coordination in `autosave_execution.rs` and
  `journal.rs`.
- [x] 2.4 Write a test that a registered-but-bodiless entry is never retired by
  orphan cleanup between registration and the body write of the same pass.
- [x] 2.5 Implement session-hash attribution in reconciliation (D2). Entries it
  reconstructs have an unknown mtime, so they are preserved as local-history
  snapshots (D3) and are not restored.
- [x] 2.6 Implement the `drafts/set-aside/` move for unattributable bodies. Its
  bodies are excluded from the recoverable inventory and from orphan cleanup,
  and each move emits a bounded diagnostic.
- [x] 2.7 Verify rollback safety: an older build's inventory ignores
  `drafts/set-aside/`. Check the `looks_like_draft` classification in the
  inventory scan.
- [x] 2.8 Update draft evidence, the automation snapshot fields if any change,
  the `WFR-*` drafts row in `docs/workflow-readability-matrix.md`, and the
  AGENTS.md "Draft persistence" decision. Run `make check-workflow-boundaries`.

## 3. Stale drafts are set aside, not deleted (D3)

- [x] 3.1 Write a failing test. Today a file-backed draft whose backing mtime
  changed is deleted; the test asserts instead that it is preserved as a
  `Periodic` local-history snapshot of the file, byte-identical to the stale
  body and timestamped with the draft's `saved_at_secs`.
- [x] 3.2 Write a failing test for a file outside local-history policy. The
  body must be preserved byte-identically in `drafts/set-aside/`.
- [x] 3.3 Implement preserve-then-retire ordering through the
  local-history capture path, with `drafts/set-aside/` as the fallback. If
  preservation fails, the body and its entry are kept and the draft is not
  restored over the file. No persisted-format change is allowed.
- [x] 3.4 Update the stale-draft inline alert:
  - the text names where the edits went;
  - when they went to local history, add a "Show in Local History" action that
    opens the browser for that file, wired through the existing
    `LushtextInfoBar` connectors;
  - if the announcement or keyboard path changes, update
    `docs/accessibility.md` and `docs/accessibility-matrix.md`;
  - if new actions become visible, update `docs/automation*.md`.
- [x] 3.5 Add widget coverage for the alert action, confirming it opens the
  local-history browser on the preserved snapshot.

## 4. ViewportSliceBin swallowed request (D4)

- [x] 4.1 Write a widget probe that fails if the request is swallowed: a
  `scroll_to` in the same allocation as an upper or page reconfiguration, with
  the target within `reconfigure_shift` of the published offset. Assert the
  row ends inside the outer viewport. Use the sidebar, the adoption lab, or a
  synthetic `GtkScrollable` fixture (the D4 open question).
- [x] 4.2 Fix the stale comment in `scroll_request.rs` ("the next allocation
  re-decides"), whatever the probe's outcome.
- [x] 4.3 Implement the deferral: skip the write-back and queue one
  re-allocation when the shift guard suppresses a divergence. Keep the
  decision pure in `scroll_request.rs`, with unit and property tests.
- [x] 4.4 Run the full rendered-bounds stillness, allocation-count, and
  correction-count suites in `workspace_tree_virtualization.rs` and
  `gtk_lush_adoption.rs`. All must stay green. If settles persist into stable
  frames, stop and design a discriminator, recording it as axiom A8.
- [x] 4.5 Update the GTK Lush widgets CHANGELOG and README, the public-API
  snapshot if it changes, and the gtk-lush-stewardship evidence. Run
  `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`,
  `make gtk-lush-doctests`, and `make gtk-lush-examples`.

## 5. Durable-write fixes (D5, D6, D7)

- [x] 5.1 Write a failing unit test: an injected after-rename parent-sync
  failure during local-history migration must not attempt a copy, and a retry
  completes the migration.
- [x] 5.2 Implement `EXDEV` classification in the filesystem boundary, and
  restrict the copy fallback in `local_history_service.rs` to it.
- [x] 5.3 Write failing unit tests for `parent_or_current`, covering a bare
  name and `./name`, plus a durable write to a bare relative name. Implement
  D7 across the durable-write module.
- [x] 5.4 Write a pure temp-name predicate with tests covering: the exact
  pattern, an unknown tag, the current pid, a fresh file, a non-regular file,
  and a look-alike user file.
- [x] 5.5 Implement the bounded startup sweep of the app-data directories, off
  GTK, and the same-target sweep after a successful workspace durable write.
- [x] 5.6 Check the filesystem-boundary allowlist and audits. Run
  `make check`.

## 6. Verification and sign-off

- [x] 6.1 Run `make test` (unit, integration, and widget, headless) and
  `make test-prop`.
- [x] 6.2 Run `make crash-recovery-smoke`, and extend the smoke driver with a
  kill point inside the body-write-to-commit window if the harness can target
  it deterministically.
- [x] 6.3 Run the `data-safety` skill's explicit audit over the draft and
  durable-write changes.
- [x] 6.4 Update README.md if user-visible recovery behaviour changed. It did
  for stale drafts.
- [x] 6.5 Update `docs/next/formal-verification.md`: mark phase 0 complete,
  and record any discriminator or axiom learned in task 4.4.
