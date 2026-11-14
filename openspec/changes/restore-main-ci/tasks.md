Rules for every task:

- Headless only. Never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`.
- CPUs 8–23 only (`taskset -c 8-23`).
- Heavy jobs go through `scripts/heavy.sh`, one CBMC at a time. Do not
  override its memory admission. While it waits, the host lacks headroom.
- Spawn no sub-agents.
- Evidence goes in `.claude/handoffs/restore-main-ci/`. This stage left there:
  - `spec-experiments.patch`, with the four experiment tests;
  - `exp1.log` and `exp2.log`, the experiment results;
  - `api-advisory-container{,2}.{sh,log}`, the `fedora:45` install checks;
  - `minimap-repro.sh`, a loop of N isolated runs under LOAD busy loops.
- **Failing first** means the test is recorded red on `main` code (log in the
  handoff directory) before the fix that turns it green is written.
- **Starved isolated run** means the test runs alone through
  `scripts/run-widget-tests.sh --headless -- --exact <test>`, with the process
  and two busy loops pinned to one CPU (`taskset -c 8`), or two CPUs for a
  two-thread case. Record the load before and after.

Kani: none expected (no proven module touched). Two conditional reruns:

- if 2.4 adds or edits a function in `ui/editor_page/minimap/policy.rs`,
  rerun `core-geometry-policies` once on the final code;
- if 4.x changes `model/list_budget.rs` (the only module `core-list-budget`
  imports; the sidebar constants are not read by any harness), rerun the shard
  `kani-shards.py oracle` and `grep -rln` name, once, detached.
- **Starved runs build first** on CPUs 8–23 and pin only the test run.

Timing: none (no timed path's cost changes).

## 1. Restored text owes a draft (data safety, D1)

- [x] 1.1 Failing first. Add these widget tests to `crates/lushtext/tests/widget/window.rs`.
      Start from the patch's experiments, rename them, and run each red on
      `main` code:
      - `test_a_restore_after_the_draft_settled_owes_a_draft`: the draft
        settled, then Restore. It fails with "still owed a draft".
      - `test_a_pass_captured_before_a_restore_cannot_clear_its_draft_owed`:
        a tick with the manifest delayed, then Restore. It fails the same way.
      - `test_a_small_restore_after_the_draft_settled_owes_a_draft`: the same
        with a snapshot below the direct-replacement threshold. `begin_guard`
        runs for direct mode too, so expect it red. Record whether it is; that
        says whether this defect hits every restore or only large ones.
      - `test_an_undo_restore_owes_a_draft`: the draft settled after a
        restore, then Undo Restore. Confirm it fails.
      - `test_a_restore_into_a_clean_tab_owes_a_draft`: an unmodified
        file-backed tab restores a snapshot. Assert draft-dirty **and** the
        "•" tab title. Reading the code, both are missing on `main`, because
        GTK sets the modified flag on the first delete while the handlers are
        suspended (design D1). Record red or green for each assertion.
      - `test_a_save_formatting_mirror_back_owes_no_draft`: guards the
        exempt rows. It passes on `main` and must keep passing.
      - `test_a_recovered_draft_is_not_rewritten_before_an_edit`: startup
        `apply_draft` leaves the tab not draft-dirty (the draft holds the
        body). It passes on `main` and must keep passing. It guards against
        routing the `apply_draft` terminal through the operation.

      Register them in the `window` shard if the module mapping does not cover
      them. Then run `make check-widget-shards`.
- [x] 1.2 Implement the draft-owed operation once in `ui/window/drafts/`:
      mark the tab draft-dirty, advance the generation, then schedule the
      first-dirty autosave or mark the running pass pending.
      - Route the buffer `changed` handler (`ui/window/documents.rs`) through
        it.
      - Route the `modified-changed` handler through it too (design D1's
        `set_draft_dirty(true)` inventory).
      - Route the `open_set_aside_draft` terminal (`drafts/restore_execution.rs`)
        through it. **Not** `apply_draft`/`finish_applied_draft`: the draft
        already holds that body.
      - If 1.1's clean-tab test showed a stale title, the operation also
        re-projects the modified indicator (tab title, header bar, discard
        action) through the handler's own code, not a copy.
      - Leave the rename re-stamp and the two close-abort sites as they are
        (design D1). Record any new `set_draft_dirty(true)` site found by
        `grep -rn`.
      - Apply the data-safety skill to the change (draft persistence, async
        ordering).
      - Call it from the Restore and Undo Restore terminals in
        `ui/window/local_history/restore_execution.rs`, only on their
        `Complete` arm.
      - Follow `.agents/rules/workflow-convention.md` (role placement, no new
        `*_for_test` seam).
- [x] 1.3 Change only the post-terminal assertion of
      `test_autosave_during_a_local_history_restore_keeps_the_previous_draft`
      to "draft-dirty, or the restored text already persisted". Keep its
      during-restore assertion and its final write check.
- [x] 1.4 Prove it:
      - the 1.1 tests and the existing test, 20 consecutive starved isolated
        runs each;
      - the `window` shard once, with `make test-widget-shard
        WIDGET_SHARD=window`;
      - `make crash-recovery-smoke` once, the real-process draft-recovery
        lane;
      - `make check`.
- [x] 1.5 Update the docs:
      - the drafts and local-history rows of
        `docs/workflow-readability-matrix.md` (re-derive the measured cells if
        an operation moved), then `make check-workflow-boundaries`;
      - the module comment of `drafts/restore_execution.rs` and the
        local-history facade;
      - `docs/end-user-coverage.md` if a lane mapping changes.
- [ ] 1.6 Commit (signed, conventional, staged by name) and push the branch.

## 2. A scroll survives a passive reflow burst (D2)

- [x] 2.1 Diagnose first: run the two minimap experiments from the patch
      isolated. Done in the spec stage (design B5).
      - The control passed.
      - The discriminator failed deterministically with `y: -6`, as on CI.
- [x] 2.1b Characterise a GTK-driven move. Temporarily delete the settle snap
      (`adjustment.set_value(lower)` in `finish_minimap_reflow_settle`) and
      run the `editor-page` shard plus the minimap visual-geometry case.
      - Record which tests go red and the value each one sees. These are
        D2's discriminator evidence.
      - If none go red, write a failing-first test for "Top anchor still
        repaired after reflow" before any fix.
      - Never commit the deletion.
- [x] 2.2 Reproduce the CI failure of
      `test_minimap_native_slider_covers_the_visible_text_with_an_overscroll_margin`
      with starved isolated runs: `minimap-repro.sh`, adapted to one CPU and
      two loops, 20 runs. Then instrument, behind the test-utils build only:
      - who writes `vadjustment.value` back to the lower edge (the settle snap
        in `finish_minimap_reflow_settle`, the top-edge clamp, or a reattach
        hold);
      - whether the rest state was paused when the scroll happened.

      Record the call site in design D2. 2.1's test proves the mechanism, not
      the CI flake. If no starved run reproduces the original, record that.
      The flake's proof then is the branch's `editor-page` job across at
      least eight CI runs. It failed on about half of recent runs, and four
      clean runs would still leave about 6 % false confidence (design D4).
- [x] 2.3 Failing first. Keep the discriminating test (renamed
      `test_a_scroll_during_a_passive_width_reflow_survives_the_settle`) red on
      `main` code. Add `test_a_kept_reflow_scroll_survives_later_reflows`
      ("A kept scroll survives later reflows"): after the kept scroll, a
      width reflow and then a height-only change each settle at line 1,000.
      Record it red on `main` code; it stays red against a fix that keeps the
      value but leaves the rest state at "at top" (design D2). If the diagnosis depends on GTK adjustment ordering, add the
      ledger axiom with a `gtk-lush-axioms` probe and sample, then run
      `make gtk-axioms`.
- [x] 2.4 Fix at the diagnosed site. Use the discriminator chosen from 2.1b's
      evidence (design D2) to keep a move that came from a scroll and still
      snap a move that came from GTK reallocation.
      - The discriminator is decided at the adjustment. Both tests scroll
        through raw `scroll_to_mark`, so marking intent only at app entry
        points does not fix them. Any other placement ships the exhaustive
        scroll-source inventory with a test per source.
      - Re-record the rest state from the settled adjustment when a moved
        value is kept.
      - Do not change either test's scroll path. Keep every existing minimap
      top-anchor test green, in particular the "Minimap top content remains
      visible after shell reflow" scenarios.
- [x] 2.5 Prove it:
      - the new test and the original minimap test, 20 consecutive starved
        isolated runs each;
      - the `editor-page` shard once;
      - `make visual-geometry-smoke`, or the lane covering
        `minimap-sidebar-mid` if a different target owns it;
      - `make check`.
- [x] 2.6 Update the docs: `docs/next/minimap.md`, and the `ui.md` or
      `widget-wiring.md` paragraph on reflow rest state if its rule changed.
      If a toolkit fact was learned, also update the gtk-testing references.
      Commit and push.

## 3. API advisory installs on OpenSSL 4 (D3)

- [x] 3.1 Verify the candidates in the host's `fedora:45` image. Done in
      the spec stage (design D3 table, `api-advisory-container2.log`).
      - The only working candidate is `libcurl-devel` plus `openssl-sys`
        0.9.118.
      - The lockfile diff is that one entry.
- [x] 3.2 Add `scripts/install-cargo-public-api.sh`. It downloads the crate,
      verifies its SHA-256, applies the precise `openssl-sys` update, rejects
      any other lock change (exactly one entry may change), and runs `cargo
      install --locked --path .`.
      - It takes `CARGO_PUBLIC_API_VERSION`, `CARGO_PUBLIC_API_SHA256` and
        `CARGO_PUBLIC_API_OPENSSL_SYS_VERSION` from the environment.
      - It works under `with-temp-scope.sh`, leaving no leak.
      - Before the precise update, it fails if the crate's own lock already
        pins `openssl-sys` at or above the override. That makes the
        retirement condition self-enforcing (spec "Override retired").
      - Its self-test runs offline against a locally built fake `.crate`. It
        covers a tampered checksum, an unexpected lock change and an override
        no longer needed. It is wired into `make check-policy`, or the
        script-self-test target `build.md` names.
- [x] 3.3 In `ci.yml`, add `libcurl-devel` to the job's dnf step and the two
      new pins to the top-level `env`, and call the
      script from "Install advisory API tools". Keep the job advisory, its
      20-minute timeout and its snapshot upload.
- [x] 3.4 Update the docs: `.agents/rules/build.md` (the pin list and the
      GTK Lush advisory paragraph, including the retirement condition), and
      `README.md` if it lists the advisory tools. Then run
      `make check-agent-docs` or the guidance validator `build.md` names.
      Commit and push.
- [ ] 3.5 After the push, read the branch's GTK Lush API Advisory job (do not
      wait on other jobs).
      - It must be green with eight non-empty snapshots.
      - Record the install step's duration on a cold cache against the
        20-minute timeout.
      - If the margin is thin, add the skip-if-pinned-version check (design
        D3).

## 4. Retry-rescued widget tests (D4)

Tests: each item's failing-first test is chosen by its diagnosis.

- [x] 4.1 `workspace_tree_virtualization::test_directory_retention_stays_within_its_charge`
      - Reproduce isolated. Record `list-budget-evidence` across 20 runs,
        locally and from the CI logs in design B4.
      - Measure `VmHWM` growth at 16,384 and 32,768 rows and fit slope and
        intercept. Then decide between undercharge, a constant-size
        allocator or capacity step, and a schedule-dependent transient peak
        (D4).
      - If the charge changes: it is a coverage window, so it rounds up to
        2,048. That breaks the `SIDEBAR_TYPICAL_FLOOR_RETENTION` const
        assertion against the 512 MiB budget.
        - Either raise the budget (a presentation cap) to 1 GiB, with a
          measurement that the resource bound holds, or record an exact-value
          exception with its measurement.
        - Never lower the 131,072-entry floor.
        - Update `docs/thresholds.md`, then run `make check-thresholds` and
          `cargo bench -p lushtext-core --features test-utils --no-run`
          (CI's Bench Compile).
      - Otherwise fix the measurement, and state in the rustdoc what it
        measures.
      - Keep the 5/4 bound.
      - Prove it with 20 isolated passes, then commit and push.
- [ ] 4.2 `window::test_a_bookmark_opens_in_a_never_shown_tab_in_preview_only_mode`
      - Reproduce starved and find the timed-out predicate.
      - Classify it, fix it failing first, prove it with 20 starved isolated
        passes, then commit and push.
      - If no starved run reproduces it, follow D4's "not reproducible"
        branch: CI-log diagnosis, a deterministic mechanism test red on
        `main`, and eight branch CI runs with no `FLAKY` line for it.
- [x] 4.3 `protected_view_surfaces::test_a_protected_periodic_capture_reads_the_source_not_the_buffer`
      - The same procedure, including the "not reproducible" branch.
      - Treat it as data-safety first: it is a local-history capture. Use the
        data-safety skill.
- [x] 4.4 Add each diagnosed class and its fix pattern to the gtk-testing
      skill's flake references, if it is new.

## 5. Close

- [x] 5.1 `openspec validate restore-main-ci --strict`.
      - Update `AGENTS.md` "Recent Changes" and `README.md` if they mention a
        behaviour that changed: the restore draft, the reflow scroll.
      - Update `docs/next/` records whose posture changed.
- [ ] 5.2 Push and open the PR (the stage-3 workflow opens it, after the gates; not merged).

## Stage 3 record (implementation, 2026-11-14)

Evidence is in `.claude/handoffs/restore-main-ci/` (`stage3-impl.md` is the
log). Starved runs used CPU **12**: the toolbox's cgroup allows CPUs 12–23
only, so `taskset -c 8` fails with `EINVAL`.

- **1.1** Red on `b129f378` code (`g1-red/`): settled restore, pass captured
  before a restore, small (direct) restore, Undo Restore, and the clean tab
  (both assertions). The startup-recovered draft and the set-aside open lacked
  the "•" title too (new assertions, red). Save-formatting green.
- **1.2** Two operations (design D1, "Implemented as two operations").
- **1.4** 20/20 starved runs for each of the nine draft tests; `make test`
  green (1,687 widget tests, 0 `FLAKY`); `make crash-recovery-smoke` passed.
- **2.1b** Snap deleted: the editor-page shard (404) and the 44 minimap
  visual-geometry cases all passed, so the top-anchor repair got its own test.
- **2.2** The writer is the settle snap (trace in design D2). The original
  minimap test passed 20/20 starved; it did not reproduce the CI failure
  itself, so its regression proof is the branch's `editor-page` job on at
  least eight CI runs (open, after the PR exists: `ci.yml` runs on
  `pull_request` only).
- **2.3** Axiom A112 added with probe and sample; `make gtk-axioms` green
  (101 sample checks).
- **2.4/2.5** Fix as in design D2; the three D2 tests and the original 20/20
  starved; discriminator variants recorded (`g2-green/`).
- **3.5** Open until the PR's CI run: the advisory job must be green with eight
  snapshots, and the cold install time recorded against 20 minutes.
- **4.1** Hypothesis (b)/(c): a first-expansion constant. Separate-process fit
  on the workstation: 1,644 B/row + 5.3 MB (charge 1,960 B/row). A first
  expansion in a three-directory fixture reproduced CI's over-bound modes
  locally (2,379–2,692 B/row in 6 runs). The test now measures 32,768 rows
  (20/20, 1,667–1,978 B/row); charge, budget, floor and 5/4 bound unchanged,
  so no threshold row and no `core-list-budget` rerun.
- **4.2** Not reproduced: 20/20 starved before and 20/20 after the D1/D2 code.
  Diagnosis incomplete (CI: document shown, top line 69, cursor 300); the
  failure message now carries attach/detach counts and the adjustment. Proof
  and diagnosis stay open on eight branch CI runs.
- **4.3** Diagnosed and fixed: the periodic capture found the
  automatic-capture admission held by the edit's baseline capture and
  rescheduled five minutes later. Deterministic test (3 s baseline delay) red
  without the wait (`g43/`), green with it; both 20/20 starved.
- **Kani: none (no proven module touched).**
- **Timing: none** (no timed path's cost changes; design Timing).
