Rules for every task:

- Headless only: never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`.
- CPUs 8–23 (`taskset -c 8-23`); heavy jobs under `scripts/with-memory-cap.sh`;
  one CBMC at a time.
- Measurements at a 1-minute load below 4 (`/proc/loadavg`; wait if higher),
  interleaved `main` vs branch, with the load recorded before and after each
  run.
- Evidence goes in `.claude/handoffs/drill-down-palette-categories/`.
- Failing first means the test is committed or recorded red before the fix
  that turns it green.

## 1. Baseline, attribution and toolkit facts (before any fix)

- [ ] 1.1 Re-measure PT M1 on `main` (optimized widget build, presented):
      the grouped publish dispatch with every category full in All mode, at
      512, 16,384 and 131,072 indexed files. Record min–max per case in the
      design's "Measured baseline".
- [ ] 1.2 Measure the empty-query worker (palette open to first rows) on
      `main` and with a measurement-only count continuation, both the
      full-path and the name-prefiltered exclusion, at 131,072 files with 0,
      32 and 256 open tabs (design D1, M2). Decide: if the full-path pass fits
      within 1 ms of `main`'s worker time, keep the simpler predicate and
      record why; otherwise the prefilter.
- [ ] 1.3 Measure the category ranking on the worker at 131,072 matches
      (score, heap, sort, ordinal array), and how long a superseded category
      query runs after cancellation (design D3). If the sort exceeds
      `B_SETTLE_US`, add a task splitting it into the heap and an
      interruptible merge before task 4.
- [ ] 1.4 Measure peak RSS (`VmHWM`) and the note-source build time at
      131,072 sidecar bookmarks on `main` (count 16,384) and with the counts
      removed (design D7, M7); and `main`'s synchronous palette open-editor
      capture dispatch with 20,000 editor bookmarks (M8 baseline).
- [ ] 1.5 Add GTK axioms A96 and A97 (design D9) to `gtk-lush-axioms`: each a
      probe (A96 marked `slow` where it times 131,072 items), an
      interactive/`--check` sample and a ledger row in
      `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
      ("Verified against" the pinned GTK and libadwaita). A97 also pins that
      the capture controller sees a key release, and that two synthesized
      Backspace presses without a release both reach it (the auto-repeat
      shape D5's rule relies on). Run
      `make gtk-axioms` and `make check-gtk-axioms`. If A96 finds the
      natural-height measure walks items, change D2 (fixed list height in the
      category view) before task 5 and record it.

## 2. Pure policy and model (failing first)

- [ ] 2.1 `model/list_budget.rs`: `rows_within(limit, charge)` and Kani
      harness B5 in two parts (design D9): the closed form over every `u64`
      limit and `charge > 0` (`r * charge <= limit < (r + 1) * charge`,
      checked, no overflow), and agreement with a `RetainedBudget` for
      results of at most `MAX_SEQUENCE` rows; `charge == 0` returns
      `usize::MAX`, unit-tested. Unit tests first. Add `rows_within` and B5's
      harnesses (with measured seconds) to the list-budget `Oracle` in
      `scripts/kani-shards.py`. Rerun shard `core-list-budget` whole
      (`make kani KANI_SHARD=core-list-budget`) and record its time,
      updating `LIST_BUDGET_MINUTES` if it grew; `make check-kani-shards`.
- [ ] 2.2 `model/palette.rs`: `PaletteCategory` with stable ids, labels per
      mode (All mode's "Open Tab Notes"), parsing; `PaletteSearchRow::More`.
      Unit tests first, including id round-trips.
- [ ] 2.3 `ui/command_palette/policy.rs`, unit and property tests first:
      - the More decision and its visible text and accessible name (grouped
        thousands);
      - `next_category_row` and page moves in both views (headers skipped in
        the grouped view, clamping in the category view);
      - the leave-selection rule (More row, else the category's last row, else
        the first activatable row);
      - `backspace_leaves_category` (design D5): empty query and a release
        seen since it became empty; press-press stays, press-release-press
        leaves;
      - the entry announcement, "Showing X of Y results" and the note-source
        notice, including "at least";
      - the category budget constants, `PALETTE_CATEGORY_FLOOR_RETENTION` and
        its `const` assertion against `MAX_INDEXED_FILES`.
- [ ] 2.4 `transient_dismissal/policy.rs`: the palette rung returns "leave
      category" while a category view is shown, else "close palette"
      (unit tests first; existing ladder tests unchanged).

## 3. Failing-first widget tests

Add to `crates/lushtext/tests/widget/command_palette.rs` (or a new module
assigned in `scripts/widget-shards.py` if that shard would exceed its
measured budget; `make check-widget-shards`). Each is recorded red on the
branch before the fix:

- [ ] 3.1 No visible loss: with 131,072 indexed files and an empty query,
      the workspace group ends with a More row stating the exact hidden count
      (with 0 and with 32 open tabs); after opening it, the last file is
      reached by Page Down and opens the right path;
      a pointer double-click on a far row opens it.
- [ ] 3.2 Ranking identity: for a non-empty query, the category view's first
      64 rows equal the grouped rows, in order, for every category in All
      mode.
- [ ] 3.3 Leaving: Escape (in the entry, in the list, and with focus in the
      editor), Backspace on an empty query, the back button and Tab each
      return to grouped results with the query kept and the More row
      selected; after the first Escape **the palette is still revealed** and
      a workspace search panel below it is still shown (the entry's
      `stop-search` path fails this on a leave that does not set the
      child-handled latch, design D5); a second Escape closes the palette.
      Holding Backspace over `rs` (presses without a release) empties the
      query and stays in the category.
- [ ] 3.4 Freshness: a category completion arriving after leaving publishes
      nothing; a grouped completion arriving after entering publishes
      nothing; an index replacement while a category view is shown republishes
      it.
- [ ] 3.5 No item walk: at 131,072 rows, opening, typing, moving the
      selection and refreshing accessibility create row objects only around
      the realized rows (evidence `category_item_requests` bounded by a small
      multiple of A1's cap).
- [ ] 3.6 Accessibility: the More row's name, the category list's label and
      set size, the back button's name, one entry announcement across three
      queries, the budget notice announced once (forced through
      `CommandPaletteTestPolicy` with a small budget).
- [ ] 3.7 Note source: with byte ceilings forced small, a text cut in
      bookmarks no longer hides folder notes (fails on `main`'s `Stop`), and
      more than 16,384 small notes are all searchable; with more than 16,384
      open-editor bookmarks the palette's capture runs in turns, each within
      `B_SETTLE_US` under the existing turn cap actuator, and every bookmark
      the 4 MiB budget holds is searchable (fails on `main`'s 16,384 count).
- [ ] 3.8 Activating a More row never reaches the window's activate callback
      and keeps the palette open; click-away from a category view closes once.

## 4. Services (GTK-free)

- [ ] 4.1 `services/palette/fuzzy.rs`: count continuation on the empty-query
      path, cancellable at `PALETTE_CANCEL_CHECK_INTERVAL`; O(1) when the
      predicate is trivially true. Tests first.
- [ ] 4.2 The workspace exclusion's count predicate per task 1.2's decision,
      with a property test of equality against the full predicate (random
      indexes, open sets, symlinked aliases).
- [ ] 4.3 `services/palette/grouped.rs`: per-group match counts and the More
      row; existing grouped tests unchanged.
- [ ] 4.4 `services/palette/category.rs`: the category search returning
      `CategoryRanking`, `PALETTE_CATEGORY_CANDIDATE_BYTES` (candidate plus
      `u32`; a unit test pins 36 on x86_64) and the two `const` assertions
      (`FLOOR <= RETAINED_BYTES`, `RETAINED_BYTES / 2 < FLOOR`); property test
      of the 64-row prefix identity (design D3).
- [ ] 4.5 `services/palette/notes.rs`: `PALETTE_NOTE_SOURCE_LIMITS` without
      counts and with `SkipAndCount`; extend BN's superset property test to the
      palette's limits; remove `MAX_PALETTE_NOTE_ENTRIES`.
- [ ] 4.7 `ui/window/notes/`: move the open-editor capture out of
      `NotesBrowserState` into an owner both flights use; the palette's note
      refresh captures in sliced turns and submits after the last one, and a
      palette close cancels it; `capture_open_editor_note_evidence` drives the
      same capture; retire `open_editor_note_snapshots_bounded`; invert
      `policy.rs`'s test that pins the palette's `Stop` (design D7).
- [ ] 4.6 Criterion benchmarks in `crates/lushtext-core/benches/benchmarks.rs`
      for the empty-query count at 131,072 files and the category ranking at
      131,072 matches.

## 5. GTK adapter

- [ ] 5.1 `resources/ui/command-palette.blp`: the category bar and the
      category scroller and list; `make blueprint-generate`,
      `make check-blueprint`.
- [ ] 5.2 Correct the stale module doc of `ui/window/notes/row_model.rs`
      (it still describes remove-then-add of every row). Then
      `ui/command_palette/category_model.rs`: `PaletteCategoryModel` with
      the retarget cache, its prune constant and emission discipline, and a
      clear on leave and close that releases the source snapshots (design
      D2); a widget test republishing repeated queries over realized rows
      (BN's crash regression shape) and one asserting the model's strong
      count on the index snapshot drops after leaving.
- [ ] 5.3 `item.rs`: `KIND_MORE`; the item factory handles it and rebinds on
      content change.
- [ ] 5.4 `query_execution.rs`: the category in the request, the dispatch to
      the category search, the publish arbitration (generation and category).
- [ ] 5.5 `imp.rs`: enter and leave, the two views' navigation without a walk,
      Page Up and Page Down, Backspace on empty, `dismiss_one_level()` from
      `stop-search`, accessibility refresh from the visible view's length.
- [ ] 5.6 Window: the dismissal ladder's palette rung; the palette's
      `connect_escape_consumed` replacing the close-only callback in
      `ui/window/imp.rs`, setting the child-handled latch for a leave and a
      close (design D5); the action `win.set-command-palette-category`
      registered in `actions.rs` and handled in `palette_shell.rs`, catalog
      entry and observed action in `services/action_catalog`.
- [ ] 5.7 `evidence.rs` fields (design D10) and `ui/automation.rs` projection
      (design D11).
- [ ] 5.8 Facade narration within 370 lines; `make check-workflow-boundaries`.

## 6. Lanes

- [ ] 6.1 Accessibility smoke: a `command-palette-category` scenario on the
      dense-files fixture (enough files for a More row): the More row's name,
      the category list's label, Escape back to the grouped list. Add its
      anchors to `docs/accessibility-matrix.md`.
- [ ] 6.2 Visual smoke: a `command-palette-category` capture (light and dark),
      and a `cargo-gtk-proof` check that the category bar and list fit the
      overlay at narrow width.
- [ ] 6.3 Automation: `scripts/lushtext-automation.py` needs no new command
      (actions are catalog-checked); add a scenario-helper case if the smoke
      drives the action; `make automation-client-self-test`.

## 7. Documentation

- [ ] 7.1 `docs/thresholds.md` rows and rustdoc `Kind:` lines (design D12);
      `make check-thresholds` red first on the removed constant and the new
      ones, then green.
- [ ] 7.2 `docs/automation.md`, `docs/automation-reference.md` (fields, action,
      ids); `make check-automation-docs`.
- [ ] 7.3 `docs/accessibility.md`, `docs/accessibility-matrix.md`
      (A11Y-PALETTE-FILES/-NOTES/-COMMANDS/-DISMISS rows and a category row),
      `docs/accessibility-orca-checklist.md`.
- [ ] 7.4 `docs/workflow-readability-matrix.md`: re-derive
      `WFR-COMMAND-PALETTE`'s measured cells; add the fourth owner to
      `model/list_budget.rs`'s consumers; the transient-dismissal row's ladder
      input; `WFR-NOTES-BOOKMARKS`'s open-editor capture gaining the
      palette's refresh as a consumer.
- [ ] 7.5 `README.md` (palette features), `AGENTS.md` (module layout:
      `category_model.rs`, `services/palette/category.rs`; design decision
      text for the palette), `docs/next/gtk-lush.md` (the lazy-model extraction
      candidate), `docs/next/long-line-documents.md` (the queue entry's status
      and a record section like BL's and BN's), `docs/end-user-coverage.md` if
      lanes change.

## 8. Verdicts

- [ ] 8.1 Measure M1–M8 (design D8) interleaved with `main`, two rounds, and
      record the table in the design's "Implementation decisions".
- [ ] 8.2 Any toolkit-floor exception: attribution, its axiom, and a widget
      assertion at twice its measured worst.
- [ ] 8.3 Full gates: `make check`, `make test`, `make gtk-axioms`,
      `make check-gtk-axioms`, `make kani KANI_SHARD=core-list-budget`,
      `make check-kani-shards`, `make check-widget-shards`,
      `make accessibility-smoke`, `make visual-smoke`. Fix any pre-existing
      blocker found (`.agents/rules/preexisting-blockers.md`).
