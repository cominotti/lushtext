Rules for every task:

- Headless only: never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`.
- CPUs 8–23 (`taskset -c 8-23`). Heavy jobs go through `scripts/heavy.sh` or
  `scripts/with-memory-cap.sh`, one CBMC at a time. Spawn no sub-agents.
- Measurements:
  - at a 1-minute load below 4 (`/proc/loadavg`; wait if higher), through
    `scripts/measure.sh`;
  - interleaved `main` vs branch, with the load recorded before and after
    each run;
  - long campaigns run detached (`setsid nohup`).
- Evidence goes in `.claude/handoffs/lighten-notes-browser-rows/`. This
  stage's scripts (`measure-one.sh`, `run-rowcost.sh`, `quiet-gate.sh`), its
  probe (`zz_row_cost.rs`) and its in-dialog phase harness
  (`zz_attr_notes.rs`) are there to reuse.
- Failing first means the test is committed or recorded red before the fix
  that turns it green.

## 1. Baseline, attribution and toolkit facts (before any fix)

- [ ] 1.1 Extend this stage's baseline (design B1–B4) to the palette's phases.
      Append the phase harness to a `main` worktree's
      `command_palette_category.rs` (measurement only, never committed).
      Record, under GL and cairo:
      - X1's layout and paint;
      - the deferred clear's dispatch;
      - the grouped publish and its frame at 131,072 files.

      Record in design "Measured baseline".
- [ ] 1.2 Add GTK axioms A108–A111 (design D8) to `gtk-lush-axioms`:
      - each gets a probe, an interactive/`--check` sample and a ledger row in
        `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`,
        "Verified against" the printed GTK/libadwaita versions;
      - each gets a CHANGELOG entry;
      - A111 skips its GL half, stating why, where GL is unavailable;
      - mark a probe `slow` only if it needs seconds.

      Run `make gtk-axioms` and `make check-gtk-axioms`. If A108 or A110 does
      not hold, stop, and revise D2 or D4 before task 3.
- [ ] 1.3 Find the call path that tears the realized rows down inside the
      browser's close input dispatch (13–15 ms on `main`; the profile shows
      `gtk_list_item_manager` teardown inside `AdwDialog::close`). Use a
      backtrace or `perf` in a measurement build, and record it in design D4.
- [ ] 1.4 Measure the per-row teardown cost:
      - in the A110 probe and in the app (notes and palette rows);
      - choose the retirement unit, a power of two that is a cap rounded down,
        so one unit fits `LIST_TURN_SLICE_US` with margin;
      - add it as a named constant, with its classification in
        `docs/thresholds.md`, and run `make check-thresholds`.

## 2. Failing-first widget tests

- [ ] 2.1 Add one shared phase-split dispatch helper to
      `crates/lushtext/tests/widget/common.rs`, beside
      `worst_dispatch_until`. It reports each dispatch's layout-phase and
      paint-phase time through the frame clock's `layout`/`paint` handlers.
      Use the helper; never copy it into a test module.
- [ ] 2.2 Notes browser, 16,384 rows (`notes_browser.rs`):
      - the query frame's layout phase within `B_INTERACT_US` (red on `main`:
        about 18 ms);
      - the close input dispatch within `B_INTERACT_US` (red: about 14 ms);
      - the first frame's layout phase within a bound to be set in task 8.
      - Record the red run.
- [ ] 2.3 Palette (`command_palette_category.rs`):
      - X1's layout phase within `B_INTERACT_US` (red);
      - after Escape, every dispatch until `category_holds_sources` turns
        false within `B_INTERACT_US` (red: the 10.6–11.6 ms clear).
- [ ] 2.4 Row text parity, new tests:
      - a notes row's and a palette row's text widgets expose the `label` role
        and an accessible label equal to the bound text;
      - a rebind and a retarget replace it, and unbind clears it;
      - a `LushtextRowText` and a `GtkLabel` with the same classes (`title`;
        `subtitle caption dim-label`; `heading`) and a Latin string allocate
        the same height in the same row;
      - CJK and emoji titles are not clipped (the drawn layout's ink fits the
        row's allocation, or the overflow is drawn and stated).
- [ ] 2.5 Retirement freshness:
      - extend `test_palette_category_republishes_over_realized_rows` and BN's
        `test_repeated_queries_over_realized_rows_republish_safely` with a
        publish during a retirement: no GTK warning or critical, every
        emission matching the reported length, and the new rows listed;
      - a test that the notes sources are released when the retirement
        completes.

## 3. `LushtextRowText` (design D2)

- [ ] 3.1 Add `crates/lushtext-core/src/ui/row_text/` (`mod.rs`, `imp.rs`):
      - a `GtkWidget` subclass with the CSS name `label` and the `label`
        accessible role;
      - properties: text, ellipsize mode (none, start, middle, end), `xalign`,
        `natural-width-from-text`;
      - the measure: the cached single-line height per font description
        (including size and `line-height`), and min = natural = ellipsis width
        unless `natural-width-from-text`;
      - the layout is created on the first snapshot and kept; text changes
        queue a draw only, except in `natural-width-from-text` mode;
      - the accessible label goes through `crate::ui::accessibility`;
      - `ensure_type()` wherever a template uses it.
- [ ] 3.2 Run `make check-accessibility-policy` (custom-row rules) and
      `make check-workflow-boundaries`. Declare the module in the matrix as a
      called presentation surface, cross-cutting, with no role.

## 4. Lighter rows (design D3)

- [ ] 4.1 Notes rows and section headers use `LushtextRowText`
      (`ui/window/notes/browser.rs`). Update `notes_row_widgets`, bind,
      unbind and the header projection.
- [ ] 4.2 The palette's grouped and category rows use `LushtextRowText`
      (`ui/command_palette/row_factory.rs`): the name middle-ellipsized,
      headers with `natural-width-from-text` and no ellipsis, the subtitle as
      before.
- [ ] 4.3 Tasks 2.2 (query frame) and 2.3 (X1 layout) and 2.4 are green.
      Visual proof:
      - add or extend `make visual-geometry-smoke` cases for the notes browser
        (both modes) and the palette (grouped, header, More and category rows)
        in light, dark and high contrast;
      - pixel anchors: row heights and text origins identical to `main`;
      - Latin, CJK and emoji titles.

## 5. Sliced row retirement (design D4)

- [ ] 5.1 Add one retirement helper shared by both lists. It removes units
      from the model's end per `G_PRIORITY_LOW` turn while
      `list_budget::turn_continues`, is owned by the model and its
      generation, and is cancelled by a publish or disposal. Put it in a
      cross-cutting module (no role) or as each model's own coordination, as
      the workflow convention decides, and declare it in the matrix rows.
- [ ] 5.2 Notes browser: the close input dispatch no longer tears the rows
      down. Retirement starts after `closed`, and the sources and payloads are
      released when it completes. Add `retiring_rows` to the notes evidence.
      Task 2.2's close test is green.
- [ ] 5.3 Palette: the deferred clear becomes the retirement. Re-entry cancels
      it and publishes over the remaining rows. `category_holds_sources` turns
      false at completion. Task 2.3's leave test and task 2.5 are green.
- [ ] 5.4 If evidence fields reach the automation snapshot, update
      `docs/automation.md` and `docs/automation-reference.md`, then run
      `make check-automation-docs`.

## 6. Staged first publish decision (design D5)

- [ ] 6.1 Measure the frame that first shows 16,384 rows: GL, steady state,
      4 interleaved rounds of `main` vs branch.
- [ ] 6.2 Apply the rule:
      - if the median exceeds `B_SETTLE_US`, implement the staged first
        publish:
        - `R_FIRST` is a coverage window rounded up and `R_STEP` a cap rounded
          down, by measurement, both in `docs/thresholds.md`;
        - `growing_rows` evidence;
        - generation-guarded, and cancelled by a newer publish or disposal;
        - a failing-first test that each growth frame fits `B_SETTLE_US` and
          that the final length and "Showing X of Y" are right;
      - otherwise record the figure and do not implement it.
      - Record the decision in design.

## 7. Kani

- [ ] 7.1 If `model/list_budget.rs` changed:
      - add unit tests for any new pure function;
      - run `make kani KANI_SHARD=core-list-budget` (one CBMC, under the
        memory cap) and `make check-kani-shards`;
      - if a new harness was added (design D9), add it to `SHARDS` and
        `ORACLES` in `scripts/kani-shards.py`, and record its local time
        (dispatched CI budget: the maintainer).

      If `list_budget.rs` did not change, record that no shard is affected.

## 8. Verdicts and exceptions

- [ ] 8.1 Run 4 interleaved `main`/branch rounds, detached, on the affected
      cases only. The phase harness in both worktrees, GL steady state for the
      verdict, cairo recorded:
      - notes at 8 and 16,384 rows (first frame, query frame, close input,
        retirement turns);
      - palette at 131,072 files (Enter, first publish, X1, Escape, leave
        turns, grouped publish and its frame).

      Record min–max per case in design "Measured verdicts", and the load
      before and after each run in `runs.log`.
- [ ] 8.2 Update the committed bound tests:
      - `test_browser_dispatches_stay_within_their_measured_bounds` and
        `test_palette_category_dispatches_stay_within_their_budgets` assert
        the layout phases and input dispatches against their budgets, or
        against twice a recorded exception's worst;
      - they print whole-frame worst values without asserting them under
        cairo (A111).
- [ ] 8.3 For every remaining exception (E1–E3, X1, the deferred clear, the
      grouped publish), record in design and in
      `docs/next/long-line-documents.md` that it was closed, or its measured
      figure, D1's attribution, its axiom (A1, A90, A108, A110) and its widget
      bound. Record the cairo animation paint once, against A111, as a
      test-renderer cost.

## 9. Gates

- [ ] 9.1 `git add -N` new files first, then run:
      - `make check`;
      - `make test` (all widget shards in parallel; 0 `FLAKY`);
      - `make gtk-axioms` and `make check-gtk-axioms`;
      - `make check-widget-shards` (assign any new widget module);
      - `make check-thresholds`;
      - `make check-accessibility-policy`;
      - `make check-workflow-boundaries`;
      - the rustdoc lint gate (`.agents/rules/build.md`).
- [ ] 9.2 Run `make accessibility-smoke`, `make visual-geometry-smoke` and
      `make visual-smoke` on the final tree. Widget tests changed, so all
      three must be current for the commit. The notes browser and palette
      AT-SPI anchors must resolve unchanged.

## 10. Documentation (`.agents/rules/documentation.md`)

- [ ] 10.1 The ledger rows for A108–A111, and `crates/gtk-lush/axioms/CHANGELOG.md`.
- [ ] 10.2 `docs/next/long-line-documents.md`:
      - the queue entry's status;
      - BN's and DP's records: replace "all three come from the about 205
        rows" with the measured attribution;
      - the exception outcomes;
      - follow-ups: the other lists as `LushtextRowText` candidates, and
        `GtkAccessibleText` on the widget if needed.
- [ ] 10.3 `docs/next/gtk-lush.md`: `LushtextRowText` as a GTK Lush
      extraction candidate (two consumers).
- [ ] 10.4 `docs/upstream-reports.md`: draft the A109 report
      (`GtkInscription`'s eager layout and detached style). Filing needs the
      maintainer's approval.
- [ ] 10.5 Accessibility docs:
      - `docs/accessibility-matrix.md` rows for notes and palette rows (the
        row-text role and its proof);
      - `docs/accessibility.md` if the screen-reader expectations' wording
        changes.
- [ ] 10.6 `docs/workflow-readability-matrix.md`: the `WFR-NOTES-BOOKMARKS` and
      `WFR-COMMAND-PALETTE` rows (the retirement module, the row text
      surface), re-deriving measured cells if a row is materially
      restructured.
- [ ] 10.7 Module layout and rules:
      - `AGENTS.md` / `.claude/CLAUDE.md`: add `ui/row_text/`, plus a design
        note for the notes browser and palette rows;
      - `.agents/rules/ui.md` and `.agents/rules/widget-wiring.md`: realized
        list-row text uses `LushtextRowText`; never `GtkInscription` (A109);
        retire realized rows in slices outside input dispatches (A110).
- [ ] 10.8 The `gtk-testing` skill reference:
      - under `GSK_RENDERER=cairo`, dialog animation frames cost 16–30 ms of
        paint (A111);
      - the first GL dialog of a process includes pipeline warm-up;
      - judge frames by layout phase or measure GL in steady state.
- [ ] 10.9 Run `openspec validate lighten-notes-browser-rows --strict`.
