Rules for every task:

- Run builds and checks pinned to CPUs 8–23 (`taskset -c 8-23`).
- Run heavy jobs (widget shards, release measurement builds, Kani) under
  `scripts/with-memory-cap.sh`, with one CBMC at a time.
- Run headless only. Never use `make run`, never the live desktop, and never
  touch the real `~/.local/share/lushtext` or `~/.config`.
- Never use `pgrep -f`, and never `find /`.
- Measure only when the host's 1-minute load is below 4 (`/proc/loadavg`;
  wait when it is higher). Interleave `main` and the branch, and record the
  load before and after each run in
  `.claude/handoffs/bound-lists-by-budget-not-count/runs.log`.
- Keep handoff artifacts in `.claude/handoffs/bound-lists-by-budget-not-count/`.
- A test that must fail first is run and seen failing on the branch before
  the fix, and the failure is logged in the handoff directory.

## 1. Baseline, attribution and toolkit facts (before any fix)

- [ ] 1.1 Re-measure `main`'s three blockers, and the Replace Preview entry,
  on `origin/main` with PT's measurement code
  (`.claude/handoffs/power-of-two-thresholds/amend/measure/`:
  `search_panel.rs.add` M2/M3 and `workspace_tree_virtualization.rs.add`
  M5). Use the optimized widget build and the presented surface, three
  repetitions, at load below 4. Record the figures against design
  "Measured baseline". A difference of more than one reference frame
  (8.3 ms) from PT's numbers is investigated before going on.
- [ ] 1.2 Attribute each slow dispatch, using GLib/GTK profiler marks or
  timed sections under `test-utils`, and record the split in the handoff
  directory:
  - the first-results reveal: per-match appends, the list's first realize,
    the file-row expansions, and the walk of an unrealized list (design D4's
    hypothesis);
  - the Replace Preview entry: what runs in its 27–29 ms;
  - the 10,000-entry expansion terminal: `replace_child_cache`, the state
    restore and `sync_file_row_states`.
  If an attribution contradicts design D4 or D6, update the design's
  mechanism (not its targets) in the same commit as the fix that follows
  from it.
- [ ] 1.3 Add the GTK axioms of design D7 to `gtk-lush-axioms`. Each gets a
  probe and an interactive/`--check` sample, a row in
  `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
  (numbered after A86, with "Verified against" and "Sample" columns), and an
  entry in `crates/gtk-lush/axioms/CHANGELOG.md`. The axioms:
  - the cost of `items-changed` on an unrealized versus a presented
    `GtkListView`, against the item count;
  - the cost of an end-of-store batch splice under a `GtkTreeListModel` and a
    presented `GtkListView`, at 16,384 and 262,144 rows;
  - tail `TreeListRow::set_expanded(true)` cost against position (extends
    A22);
  - a `GtkListView`'s first realize inside a revealing scrolled window,
    against model rows and realized rows.
  Then run `make gtk-axioms` and `make check-gtk-axioms`. If the splice is
  not O(batch), switch design D6 to its decided fallback (a growing custom
  `GListModel`) before section 4.

## 2. The shared list budget (`model/list_budget.rs`)

- [ ] 2.1 Failing first: unit and property tests for:
  - admission: saturation, never exceeding the limit, the refusal latch,
    the prefix property, and no loss at `n × w`;
  - turn continuation: progress at zero units, and a stop at the first unit
    at or after the slice.
  Write them against a stub that admits everything and always continues, and
  watch them fail.
- [ ] 2.2 Implement the module: integer only, with
  `#![forbid(clippy::float_arithmetic)]` and rustdoc `Kind:` lines.
  `LIST_TURN_SLICE_US` is `B_INTERACT_US / 2` (`Kind: out: time`, the class
  of every time budget in `docs/thresholds.md`).
- [ ] 2.3 Write the Kani harnesses B1–B4 and T1–T2 (design D2) in
  `model/list_budget/kani_proofs.rs`. B4 is the counter-example harness,
  kept failing by design like the minimap shard's general-`f64` cases.
  - Add the shard `core-list-budget` to `scripts/kani-shards.py`, gating
    pull requests.
  - Run it under `make kani KANI_SHARD=core-list-budget KANI_MEASURE=…`
    within `KANI_MEMORY_MAX`, record its wall time and peak memory, and pass
    `make check-kani-shards`.
- [ ] 2.4 Measure the two fixed charges (design D3), each in a release test
  process of its own, as `VmHWM` growth against an empty control, three
  repetitions, at load below 4:
  - the search fixture retains 16,384 presented matches;
  - the sidebar fixture expands 16,384 presented rows.
  For each:
  - round the per-item figure up to a power of two (a coverage window);
  - compute `MAIN_SEARCH_WORST_RETENTION` and
    `MAIN_SIDEBAR_WORST_RETENTION` (derived);
  - compute `SIDEBAR_TYPICAL_FLOOR_RETENTION` (derived: 131,072 rows of
    32-byte names under a 256-byte parent);
  - set each budget to the power of two at or above its larger derived
    floor, with a compile-time assertion per floor.
  Before fixing the sidebar budget, shrink what a typical row retains (share
  the cache paths as `Arc<Path>`) and re-measure; the typical floor is a
  target, not a casualty of the fixed charge.
  Record the figures in the rustdocs and the design.

## 3. Workspace search without a match count

- [ ] 3.1 Failing first, in `crates/lushtext/tests/widget/search_panel.rs`:
  - 40,000 ordinary matches are all shown and navigable, with no limited
    state (it fails on `main`: 16,384 shown); Replace Preview lists its
    16,384-row budget and states the 23,616 omitted;
  - `main`'s worst case (16,384 matches of 4,096-byte lines, one file each,
    4,096-byte paths) is retained in full;
  - a limited search (the budget lowered through `test_policy.rs`) retains a
    gapless prefix, and states shown and found counts that grow until `Done`;
  - each turn's matches reach the model as one splice per touched file plus
    one root splice, counted through evidence;
  - cancelling while counting past the budget stops the counting.
- [ ] 3.2 Worker (`services/content_search/search.rs`):
  - remove `RESULT_CAP`;
  - charge matches through `RetainedBudget` behind one per-search lock held
    across `admit` and the send (design D4), so channel order is admission
    order; past the budget, count lock-free behind an `AtomicBool` latch;
  - add `SearchEvent::BudgetReached` and a coalesced
    `SearchEvent::BeyondBudget`, and let `Done` carry the total;
  - keep counting without retaining past the budget, with `BeyondBudget`
    coalesced by a worker clock (at most every 250 ms, `try_send`), and the
    budget never setting `WalkStop`;
  - a service test with several walker threads and a budget that cuts
    mid-stream: every retained match precedes `BudgetReached` in the channel,
    and the retained count plus the counted remainder equals an unlimited
    run's total;
  - keep `Incomplete`, `Error` and cancellation unchanged.
  Update the service tests that pinned the cap.
- [ ] 3.3 Intern paths per file (`Arc<Path>`), shared by the file group, the
  match items, the sealed Replace Preview snapshot and the navigation index.
  Check with the data-safety skill's search domain that Replace and Undo
  still validate each file under the stable target guard.
- [ ] 3.4 GTK drain (`ui/search_panel/execution.rs`): continue while
  `turn_continues` holds and fewer than 256 events have been received. Batch
  the matches per file, apply one splice per child store and one root splice,
  then expand the new tail file rows in one pass. Every received event still
  consumes one unit.
- [ ] 3.5 The reveal turn applies one batch (design D4 b). Measure it. If a
  residual over `B_SETTLE_US` remains and task 1.3's first-realize axiom
  shows it is the toolkit's, record it as a measured toolkit-floor
  exception: in the design, in `docs/next/long-line-documents.md` and in the
  constant or function rustdoc that owns the turn. Otherwise the widget test
  of task 8.2 enforces the budget.
- [ ] 3.6 Labels and announcements: settle the limited label and the
  announcement wording against `docs/accessibility.md` (bounded,
  content-free, one polite announcement at the budget, the final counts at
  completion). Add policy unit tests for the wording. A limited search is not
  saved to history.
- [ ] 3.7 Replace Preview: install its rows in time-sliced splices behind a
  "Preparing preview…" state. Add the summary's search-limit clause. Add
  widget tests: a preview after a limited search lists only retained matches,
  states the excluded count, and replaces only checked rows; a preview after
  an unlimited search has no clause. Move `JOURNAL_SCAN_MAX_ENTRIES`'s
  assertion to `MAX_REPLACE_PREVIEW_ROWS`.
- [ ] 3.8 Measure retirement at a full budget: every retirement turn within
  `B_SETTLE_US`, and the two-generation backpressure unchanged (existing
  retirement tests extended to a budget-sized generation).
- [ ] 3.9 Evidence (`ui/search_panel/evidence.rs`): `retained_matches`,
  `matches_found`, `result_limited`, `reveal_turn_events`, and the worst turn
  duration (`test-utils`). Automation projects from it (task 6.1).

## 4. Workspace sidebar without an entry count

- [ ] 4.1 Failing first, in
  `crates/lushtext/tests/widget/workspace_tree_virtualization.rs`:
  - 16,385 entries are all listed, with no placeholder (it fails on `main`);
  - 131,072 entries with names of at most 32 bytes are all reachable by
    keyboard traversal to the last row and by `win.reveal-workspace-path`.
    Assert rendered rows (`drawn_labels`, A10) and bounded realized rows
    (A1);
  - the loading row exists while batches remain, and is gone at the
    terminal; `workspace-refresh-complete` is pending until then;
  - a budget-limited directory (budget lowered through the test policy) ends
    with "Showing X of Y entries", with exact X and Y;
  - a refresh or collapse during loading leaves no stale batch, cache entry
    or loading row, and the dispatch that supersedes a budget-sized plan
    stays within `B_SETTLE_US` (its rows retire through the plain-data
    disposal lane, or the measured drop fits);
  - the terminal dispatch's duration at 10,000 and at 131,072 entries
    differs by less than one reference frame (8.3 ms).
  Update the existing scenario "Truncation placeholder is reachable" test to
  the budget row.
- [ ] 4.2 `services/file_tree.rs`: scan in a single pass when the directory
  fits the byte budget, and use the existing deterministic two-pass top-k
  only when it does not, turning the overflowing single pass into the
  top-k's first (scalar) pass so an over-budget directory costs two passes,
  not three (design D6). Report the visible entry total. Add service tests
  for both paths and for determinism of order.
- [ ] 4.3 Replace `MAX_DIR_ENTRIES` with `SIDEBAR_DIRECTORY_RETAINED_BYTES`
  and the row charge in the workspace-tree workflow's `ui/sidebar/policy.rs`
  (its test override in `ui/sidebar/test_policy.rs`). Change the
  placeholder kind from "truncation" to "budget" and "loading" (in
  `ui/sidebar/file_tree_item.rs`), with accessible names and descriptions
  (the row metadata apply/clear pattern of `widget-wiring.md`).
- [ ] 4.4 Maintain the path caches per applied batch, guarded by the mirror
  generation, on both the expansion and the refresh-reconciliation paths.
  `finish_child_reconciliation` no longer rebuilds the whole directory.
  Remove `recache_child_rows_from_mirror` from those paths, or keep it only
  where a test shows it is O(1) in the directory size.
- [ ] 4.5 Time-slice the batch application with `turn_continues` (batches
  stay 256 rows). Add the loading row and update its label at most once per
  turn.
- [ ] 4.6 Workspace-tree evidence (`ui/sidebar/evidence.rs`): `directory_total_entries`,
  `directory_retained_entries`, `directory_limited`, `loading_tail_rows`,
  and the snapshot counts (task 6.1).
- [ ] 4.7 Remove the bench mirror of `MAX_DIR_ENTRIES` from
  `crates/lushtext-core/benches/benchmarks.rs`, or rebind it to the budget,
  and keep `cargo bench --no-run --features test-utils` building.

## 5. Thresholds

- [ ] 5.1 `scripts/check-threshold-kinds.py` self-test, failing first on:
  - a byte budget below its `up from` value;
  - a missing fixed-charge row;
  - a leftover `RESULT_CAP` row once the constant is gone.
  The last is the existing stale-row check; add a fixture case if none
  covers a removed presentation cap.
- [ ] 5.2 Update `docs/thresholds.md`:
  - remove `RESULT_CAP`, `MAX_DIR_ENTRIES` and the bench mirror;
  - add the two budgets (presentation cap,
    `up from <the larger derived floor>`), the two fixed charges (coverage
    window), the two `MAIN_*_WORST_RETENTION` values and
    `SIDEBAR_TYPICAL_FLOOR_RETENTION` (derived), and `LIST_TURN_SLICE_US`
    (`out: time`, `expr: B_INTERACT_US / 2`);
  - re-describe `JOURNAL_SCAN_MAX_ENTRIES`'s row (asserted against
    `MAX_REPLACE_PREVIEW_ROWS`).
  `make check-thresholds` passes.

## 6. Automation, accessibility and smoke

- [ ] 6.1 Snapshot fields (design D9): `content_search.retained_match_count`
  and `content_search.matches_found`; the meaning of `result_capped` and
  `search.workspace_result_capped`; the workspace snapshot's
  `workspace.limited_directory_count` and `workspace.loading_directory_count`
  (projected from `WorkspaceTreeEvidence`). Update
  `model/automation.rs`, `ui/automation.rs`, `docs/automation.md` and
  `docs/automation-reference.md` (anchors). Run `make check-automation-docs`,
  and `make automation-client-self-test` if the client reads the fields.
- [ ] 6.2 Add the Cargo feature `smoke-list-budgets` (`LUSHTEXT_SEARCH_RETAINED_BYTES`
  and `LUSHTEXT_SIDEBAR_DIRECTORY_RETAINED_BYTES` overrides). Enable it only
  in the accessibility and visual smoke builds. Prove its absence from the
  default build with CI's `cargo check -p lushtext --bins --locked`, plus a
  `cfg` audit test.
- [ ] 6.3 Accessibility smoke: the case `workspace-search-capped` keeps its
  id and reaches the limited state through the override. Replace the anchor
  `atspi-anchor-workspace-search-status-bar-16-384-results-truncated` with
  `atspi-anchor-workspace-search-limited`. Add the case
  `workspace-tree-limited` (budget row name and description). Run
  `make accessibility-smoke` headless.
- [ ] 6.4 Update `docs/accessibility.md` (announcements, budget and loading
  rows) and the `docs/accessibility-matrix.md` rows
  `A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS`, `A11Y-WORKSPACE-SEARCH-REPLACE`,
  `A11Y-WORKSPACE-DENSE-DEEP` and `A11Y-WORKSPACE-BUSY-ERROR`. Update
  `docs/accessibility-orca-checklist.md` if a manual expectation changes.
- [ ] 6.5 Visual smoke: check whether a case shows the search count label
  or the sidebar placeholder, and update its expected text. Run
  `make visual-smoke` headless.

## 7. Workflow structure and documentation

- [ ] 7.1 Re-derive the `WFR-SEARCH-REPLACE` and `WFR-WORKSPACE-TREE` rows in
  `docs/workflow-readability-matrix.md`: current sizes, per-kind seam counts
  and pure-policy consumers, excluding `#[cfg(test)]`. Name
  `model/list_budget.rs` as shared cross-cutting pure policy with two
  owners. Run `make check-workflow-boundaries`, keeping both facades within
  370 lines.
- [ ] 7.2 Update `AGENTS.md`/`.claude/CLAUDE.md` (the content search panel's
  "Result cap: 10,000 matches", the directory entry cap paragraph, the module
  layout entry for `model/list_budget.rs`) and the matching passages in
  `README.md`. Update `.agents/rules/widget-wiring.md` with the time-sliced
  batch splice pattern and the per-batch cache rule, and update
  `.agents/rules/build.md` with the new smoke feature.
- [ ] 7.3 Update `docs/next/long-line-documents.md`, in the PT row and the
  PT section queue:
  - this change's progress;
  - the three queued follow-ups (`bound-notes-browser-by-budget` with its
    32–40 ms blocker, `drill-down-palette-categories`,
    `bin-minimap-markers-per-row` with the M7 attribution precondition);
  - the process-wide sidebar ledger follow-up;
  - any toolkit-floor exception from task 3.5.
  Add the change's axioms to "Axioms each change cites".
- [ ] 7.4 Update `docs/end-user-coverage.md` if a lane's responsibility
  changed, and the `gtk-testing` skill references with any testing pitfall
  met (for example, unrealized-list measurement artifacts).

## 8. Measurements (interleaved with `main`, load below 4)

- [ ] 8.1 Re-run M2, M3 and M5 on `main` and on the branch, interleaved,
  three repetitions each. Add the branch cases at 40,000 matches, at the
  search budget (through the test policy), at 16,385, 131,072 entries and at
  the sidebar budget. Record every worst dispatch, the reveal turn, the
  Replace Preview entry and the terminal against `B_SETTLE_US`, with the load
  before and after each run, in `runs.log`.
- [ ] 8.2 Turn the budget verdicts into enforced widget assertions (the
  `window_construction` gate pattern): every dispatch within `B_SETTLE_US`
  in the optimized widget build, or the recorded exception's measured bound.
  Prove them in isolation, so that they are not flaky under the parallel
  shards (`preexisting-blockers.md`, flake discipline).
- [ ] 8.3 Memory: `VmHWM` growth at a full search budget and a full sidebar
  budget is at most `budget × 5/4`, in a release process of its own. At
  `main`'s worst case, the branch's peak is within `main`'s plus 5 %.
- [ ] 8.4 Write the verdict table into the design ("Implementation
  decisions") and the programme record.

## 9. Verification

- [ ] 9.1 `make check` (fmt, all-feature Clippy, policy audits including
  `check-thresholds`, `check-workflow-boundaries`, `check-widget-shards`,
  `check-gtk-axioms`, `check-kani-shards`).
- [ ] 9.2 `make test` (unit, integration and every widget shard), headless.
  A new widget test module is assigned to a shard in
  `scripts/widget-shards.py`.
- [ ] 9.3 `make kani KANI_SHARD=core-list-budget`. No existing shard's
  inputs change. If task 4.4 touches a Kani-modelled function, rerun its
  shard as well.
- [ ] 9.4 `make gtk-axioms` (slow probes included, locally).
- [ ] 9.5 `make accessibility-smoke`, `make visual-smoke` and
  `make performance-smoke`, headless. Add a Criterion case for the
  single-pass versus two-pass scan if it is not covered.
- [ ] 9.6 `cargo check -p lushtext --bins --locked` with default features
  (no smoke feature leaks), and
  `cargo bench --no-run --features test-utils`.
- [ ] 9.7 `openspec validate bound-lists-by-budget-not-count --strict`.
