Rules for every task:

- Headless only.
- Measurements at a 1-minute load below 4, interleaved `main` vs branch,
  with the load recorded before and after each run.
- Heavy jobs under `scripts/with-memory-cap.sh`.
- Evidence goes in `.claude/handoffs/bound-notes-browser-by-budget/`.
- Failing first means the test is committed or recorded red before the fix
  that turns it green.

## 1. Baseline, attribution and toolkit facts (before any fix)

- [ ] 1.1 Re-measure PT M6 on `main` (optimized widget build, presented),
      and attribute its worst dispatch with GLib/GTK profiler marks or
      `perf`: dialog build, open-editor snapshot, first `AdwSidebar`
      population, first preview, first frame.
      - Measure three cases: 512 rows; 16,384 and 131,072 rows with the
        render cap, the source entry count and the sidecar scan count
        raised by a measurement-only patch (`main`'s source stops at 16,384
        rows, so the render cap alone cannot reach 131,072); and 20,000
        bookmarks in one open editor (with the snapshot count raised).
      - Also measure the snapshot cost per bookmark, the selection-restore
        scan at 131,072 rows (design D2), and the worker query time at
        131,072 rows (design Risks).
      - Record each figure in the design's "Measured baseline".
- [ ] 1.2 Decide from 1.1 whether the dialog build alone exceeds
      `B_SETTLE_US`. If it does, add a staged-build task (design D6) before
      task 3.
- [ ] 1.3 Add the GTK axioms of design D7 (A92–A95) to `gtk-lush-axioms`.
      - Each gets a probe (marked `slow` where it times 131,072 items), an
        interactive/`--check` sample and a ledger row in
        `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
        ("Verified against" libadwaita 1.9.3 / GTK 4.22.5).
      - Run `make gtk-axioms` and `make check-gtk-axioms`.
      - Record A93's identity answer (design D2 fallback) and A94's header
        role (design D9) in the design.

## 2. Pure policy (`ui/window/notes/policy.rs`)

- [ ] 2.1 Failing first, as unit and property tests:
      - `group_note_rows` matches the old GTK-side grouping (four category
        passes) for random sources, both modes, and empty categories;
      - the section lookup returns the right section for every position, at
        every boundary;
      - the limited-label and announcement wording covers both modes,
        "at least", and the query form (design D5);
      - the admission superset property: for random row charges and byte
        limits, the count-free skip-and-continue rule admits a superset of
        `main`'s count-and-byte prefix latch (design D3);
      - the snapshot restart rule restarts on any generation change between
        an editor's first and last slice, and never captures an editor in
        one unbounded unit (design D4).
- [ ] 2.2 Implement them. The module stays free of GTK imports and keeps its
      `forbid` attributes.

## 3. The virtualized list (design D1, D2)

- [ ] 3.1 Failing first, in `crates/lushtext/tests/widget/window.rs` (or a
      new `notes_browser.rs` module, assigned a shard in
      `scripts/widget-shards.py`):
      - **No visible loss.** 20,000 closed-file bookmarks plus 20,000
        open-editor bookmarks are all listed; the last row is reached by
        `End`, scrolled to, and drawn (`drawn_labels`, A10); no refine
        notice appears.
      - **Virtualization.** Realized row widgets at 131,072 rows stay within
        the A1 cap.
      - **Selection survives a query** (design D2), without reloading the
        preview.
      - **Keyboard.** Down across a section boundary skips the header;
        Enter previews without opening; Tab order; Escape; the narrow
        collapsed layout's back button.
      - **Both modes.** Browse Bookmarks lists only bookmarks.
      - **Generation guards.** A stale query or a stale source cannot
        publish into a newer model or activate a replaced row (existing
        delay hooks).
      - **Automation indexes.** `win.select-notes-browser-row` and
        `win.open-notes-browser-selection` use the flat index without
        headers.
- [ ] 3.2 Implement `ui/window/notes/row_model.rs`: the `GListModel` +
      `GtkSectionModel` subclass and the row object, with A93's cache
      fallback only if the probe requires it.
- [ ] 3.3 Change `browser.rs` to the `GtkListView`, the item and header
      factories (styled like `AdwSidebar`), the empty-state stack and one
      scroller. Remove `append_notes_sidebar_sections`,
      `build_notes_sidebar_item` and `sidebar_item_index`.
- [ ] 3.4 Move grouping into `query_notes_browser_source`, returning grouped
      indexes and section bounds. Remove `NOTES_BROWSER_RENDER_LIMIT` and
      its `max` parameter for the browser path.
- [ ] 3.5 Migrate the roughly 35 `find_adw_sidebar` call sites to a
      list-view helper in the widget `common` module, assertions unchanged.
      The whole widget suite stays green.

## 4. Source without counts; the sliced snapshot (design D3, D4, D5)

- [ ] 4.1 Failing first:
      - a source above 16,384 rows (no byte cut) is admitted in full;
      - a text-budget cut inside the bookmarks still lists the folder and
        document notes that fit (red on `main`, which stops at the first
        refusal);
      - the palette's note source is unchanged under the same cut (it keeps
        `ByteRefusal::Stop`);
      - a text-budget cut (test override in `test_policy.rs`) shows
        "Showing X of Y …", raises exactly one announcement across three
        later queries, and lists the rows that fit after a refused row;
      - a path-byte cut says "at least";
      - the 20,000-bookmark editor is captured across several turns, each
        within `B_SETTLE_US`;
      - a bookmark added between slices is captured consistently, and the
        restart is counted in the evidence;
      - a snapshot byte cut counts the rows it did not retain into Y;
      - closing the dialog mid-capture removes the turn source.
- [ ] 4.2 `services/palette/notes.rs`:
      - make the `entries` and `sidecar_entries` limits `Option`;
      - add `on_byte_refusal: ByteRefusal` (`Stop` for the palette,
        `SkipAndCount` for the browser), so a byte refusal skips the row
        and construction continues;
      - add `rows_found` and `found_is_lower_bound` to the metrics;
      - keep counting after a byte refusal.

      The palette's limits and behaviour stay unchanged, and its existing
      tests stay green.
- [ ] 4.3 `source_execution.rs`: the sliced capture with `turn_continues`
      and `RetainedBudget` at the unchanged 4 MiB, as a named resumption
      point in the facade (`notes/mod.rs`, within 370 lines). Captured
      bookmarks are sorted on the worker, not on GTK, and the capture counts
      the rows past the byte cut without cloning them. The palette keeps
      its one-turn plan.
- [ ] 4.4 Labels and the once-per-generation announcement
      (`query_execution.rs`, `policy.rs`).
- [ ] 4.5 Evidence (`evidence.rs`): the fields of design D8.

## 5. Thresholds

- [ ] 5.1 `scripts/check-threshold-kinds.py` self-test, failing first on a
      leftover `NOTES_BROWSER_RENDER_LIMIT` row and a missing
      `OPEN_EDITOR_SNAPSHOT_SLICE_ROWS` row.
- [ ] 5.2 Update `docs/thresholds.md` (design D10). `make check-thresholds`
      passes.

## 6. Automation, accessibility and smoke

- [ ] 6.1 Snapshot fields `notes.browser_open`, `notes.browser_row_count`,
      `notes.browser_rows_found` and `notes.browser_limited` (design D9):
      - failing first in the automation snapshot test;
      - documented in `docs/automation.md` and
        `docs/automation-reference.md`;
      - `make check-automation-docs` and
        `make automation-client-self-test` if the client changes.
- [ ] 6.2 `LUSHTEXT_NOTES_SOURCE_TEXT_BYTES` under `smoke-list-budgets`.
      Prove it absent from default builds with
      `cargo check -p lushtext --bins --locked`.
- [ ] 6.3 Accessibility smoke:
      - the new case `notes-limited`, with the anchor
        `atspi-anchor-notes-browser-status-bar-showing`
        (role `status bar`, the existing limit label);
      - the existing `notes-populated` and `notes-no-results` cases keep
        their anchors;
      - section headers, or the row descriptions, are checked per A94.
- [ ] 6.4 Update:
      - `docs/accessibility.md`;
      - `docs/accessibility-matrix.md` (`A11Y-NOTES-EMPTY`,
        `A11Y-NOTES-POPULATED`, "Notes and bookmark rows");
      - `docs/accessibility-orca-checklist.md` (limited announcement,
        section headers);
      - `docs/end-user-coverage.md` if a lane's responsibility moves.
- [ ] 6.5 Visual smoke: re-capture `notes-few`, `notes-dense` and
      `notes-constrained`. Compare them with `main`. Fix any difference in
      look, or record it in the design.

## 7. Workflow structure and documentation

- [ ] 7.1 Re-derive the `WFR-NOTES-BOOKMARKS` row in
      `docs/workflow-readability-matrix.md` (`row_model.rs` declared as a
      called presentation surface). `make check-workflow-boundaries` passes.
- [ ] 7.2 Update `AGENTS.md`/`.claude/CLAUDE.md` ("Bookmarks and rich notes";
      the `notes/` module line) and the `ui/window/AGENTS.md` notes
      contract if it names `AdwSidebar`.
- [ ] 7.3 Update `docs/next/long-line-documents.md`:
      - item 1's sub-bullet;
      - the "Axioms each change cites" table (A1, A10, A87; added A92–A95);
      - the next-release-notes paragraph ("the notes browser 512 rows"
        becomes "every note and bookmark");
      - follow-ups: the local-history browser's `AdwSidebar`, and the
        palette's counts in `drill-down-palette-categories`.

## 8. Measurements (interleaved with `main`, load below 4)

- [ ] 8.1 Re-run task 1.1's cases on `main` and on the branch, interleaved:
      - every browser dispatch against `B_SETTLE_US`, and the key-press
        dispatch against `B_INTERACT_US`;
      - the snapshot unit cost, which fixes
        `OPEN_EDITOR_SNAPSHOT_SLICE_ROWS`;
      - any residual: shrink it, or record a toolkit-floor exception with
        an axiom (design D6).
- [ ] 8.2 Turn the verdicts into enforced widget assertions with a 2× margin
      (the BL task 8.2 pattern).
- [ ] 8.3 Memory and worker time: `VmHWM` growth at a full 64 MiB source on
      the branch, against `main` at its count cap. Worker query time at
      131,072 rows. Act on design Risks if it is over 100 ms.
- [ ] 8.4 Write the verdict table into the design ("Implementation
      decisions").

## 9. Verification

- [ ] 9.1 `make check`: fmt, all-feature Clippy, and the policy audits,
      including thresholds, workflow boundaries, Blueprint, automation docs
      and GTK axioms.
- [ ] 9.2 `make test`: unit, integration and every widget shard, headless.
      No `FLAKY` and no `TEMP-LEAK`.
- [ ] 9.3 `make kani KANI_SHARD=core-list-budget`, only if
      `model/list_budget.rs` changed (design D7). Otherwise record that it
      did not change.
- [ ] 9.4 `make gtk-axioms` (slow probes included, locally).
- [ ] 9.5 `make accessibility-smoke`, `make visual-smoke` and
      `make automation-client-self-test`.
- [ ] 9.6 `cargo check -p lushtext --bins --locked` with default features.
- [ ] 9.7 `openspec validate bound-notes-browser-by-budget --strict`.
