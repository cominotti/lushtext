## Why

The maintainer's standard (2026-11-05) is that LushText be extremely
performant, handle high loads and large workspaces, and show no user-visible
loss. Two of the largest lists in the app still cut at a count:

- **Workspace search** stops at 16,384 matches (`RESULT_CAP`) and says
  "16,384+ results (truncated)". It never says how many matches there were.
- **The workspace sidebar** lists the first 10,000 entries of a directory
  (`MAX_DIR_ENTRIES`, kept at 10,000 by measurement) and ends with
  "10000+ items - showing first 10000".

`power-of-two-thresholds` (PT) rounded such caps up. Its design D13 item 1
queued the structural fix: bound lists by retained bytes and per-turn time,
not by count, and always show what is past any cut that remains.

The same measurements showed that `main` already misses its dispatch budgets
on these lists:

| Dispatch | `main` | Budget |
|---|---|---|
| First-results reveal of a large search | 22–26 ms | `B_settle` 16 ms |
| Entering Replace Preview over it | 28–29 ms | `B_settle` 16 ms |
| Expanding a 10,000-entry directory | 25–30 ms (about 2.6 µs per entry, so any directory above about 6,000 entries misses) | `B_settle` 16 ms |

The pre-existing-blockers rule makes those part of this work.

D13 item 1 also names the notes browser, the palette's per-category cut and
the minimap marker caps. This change does the **workspace slice**: search and
the sidebar, which share a worker-fed `GtkListView` shape and the same two
mechanisms. The other three are queued as their own changes in the programme
record (design D1).

## What Changes

- **A shared, Kani-proved list budget** (`model/list_budget.rs`):
  - admission against a retained-byte budget, in which what is kept is
    always a prefix of delivery order and never less than `main` kept at its
    worst case;
  - a per-turn time slice (`B_interact / 2`), in which every turn makes
    progress and stops at the first unit past the slice.
  - New Kani shard `core-list-budget`.
- **Workspace search without a match count cap.**
  - The worker retains matches up to `SEARCH_RESULTS_RETAINED_BYTES`
    (256 MiB, the power of two above `main`'s worst-case retention at 16,384
    matches). Past it, the worker keeps counting without retaining.
  - The panel says "Showing X matches in F files; N more not shown". It
    announces the limit once, and the snapshot reports `matches_found`.
  - GTK applies each turn's matches as one splice per file and one per root,
    instead of one `items-changed` per match. A turn is bounded by the time
    slice as well as the 256-event cap.
  - The reveal turn carries one batch.
  - Paths are interned per file.
  - Replace All keeps its own safety budget. Its preview states the matches
    past the search limit, and it never replaces them.
- **Workspace sidebar without a directory entry cap.**
  - A directory scan retains up to `SIDEBAR_DIRECTORY_RETAINED_BYTES` (the
    power of two at or above the larger of `main`'s worst case at 10,000
    entries and 131,072 typical entries) in one pass. It uses the existing
    two-pass top-k only when the directory exceeds the budget.
  - Rows load in time-sliced batches from the top, behind a "Loading
    entries… X of Y" tail row. Path caches are maintained per batch, so the
    terminal dispatch is O(1) in the directory size.
  - The `10000+ items` placeholder is gone. A directory past the budget ends
    with "Showing X of Y entries".
- **The three slow dispatches** (reveal, Replace Preview entry, expansion
  terminal) fit `B_settle`, or any toolkit floor that remains is recorded as a
  measured exception with a GTK axiom and a sample.
- **New GTK axioms** (after A86), each with a probe and a sample:
  - an unrealized `GtkListView`'s walk on `items-changed`;
  - an end-of-store splice cost under `GtkTreeListModel`;
  - tail `set_expanded`;
  - the first-realize cost inside a revealer.
- **Thresholds.** `RESULT_CAP`, `MAX_DIR_ENTRIES` and the bench mirror leave
  `docs/thresholds.md`. The two budgets (presentation caps, `up from` their
  derived floors), their fixed charges (coverage windows), the floors
  (derived) and the slice (`out: time`) are added. `JOURNAL_SCAN_MAX_ENTRIES` is asserted against
  `MAX_REPLACE_PREVIEW_ROWS`.
- **Automation.** New fields `content_search.matches_found`,
  `content_search.retained_match_count`, and the sidebar's
  `workspace.limited_directory_count`/`workspace.loading_directory_count`. `result_capped` keeps
  its name and now means "the budget cut". One AT-SPI anchor is renamed. The
  accessibility smoke reaches the limit through a smoke-only feature,
  `smoke-list-budgets`.
- **Not breaking:** no persisted format changes, and no action or D-Bus
  member is removed.

## Capabilities

### New Capabilities

None. Every behaviour belongs to an existing capability.

### Modified Capabilities

- `main-thread-responsiveness`:
  - the per-turn event requirement gains the time slice;
  - the large-projection requirement is restated without a count;
  - new requirements: budget-bounded search results, the workspace list
    dispatch budgets, and the shared proved list budget.
- `workspace-tree-virtualized-rendering`:
  - completeness is stated for every entry within the budget, with the budget
    row replacing the truncation placeholder;
  - new requirement for the incremental load.
- `search-replace-safety`: Replace All never acts on matches past the search
  budget, and says so.
- `gtk-accessibility-spine`: list cuts and incremental loads are stated and
  announced once.
- `dbus-automation-spine`: snapshot fields for retained and found counts.

## Impact

- **Code:**
  - `model/list_budget.rs` (new) and its `kani_proofs.rs`;
  - `services/content_search/search.rs`, `model/content_search.rs` (events,
    interned paths);
  - `ui/search_panel/{policy,execution,evidence,results,replace_execution,list_factory}.rs`;
  - `services/file_tree.rs` (single pass);
  - `ui/sidebar/{policy,evidence,test_policy}.rs` (the `WFR-WORKSPACE-TREE`
    role homes), `ui/sidebar/workspace_section/scan_execution.rs`, and the
    file-tree item's placeholder kinds (`ui/sidebar/file_tree_item.rs`);
  - `services/search_backup.rs` (assertion);
  - `ui/automation.rs`, `model/automation.rs`;
  - the `smoke-list-budgets` feature in `crates/lushtext-core/Cargo.toml` and
    the smoke scripts.
- **Tests:**
  - failing-first widget tests in `search_panel.rs` and
    `workspace_tree_virtualization.rs` (no visible loss against `main`, each
    budget holding, every dispatch within `B_settle`);
  - unit and property tests for the policy;
  - the Kani shard `core-list-budget`;
  - gtk-axioms probes and samples;
  - the accessibility smoke case `workspace-search-capped`, and a new
    `workspace-tree-limited` case.
- **Docs:**
  - `docs/thresholds.md`, `docs/automation.md`, `docs/automation-reference.md`;
  - `docs/accessibility.md` and `docs/accessibility-matrix.md`;
  - `docs/workflow-readability-matrix.md` (two rows re-derived);
  - the axiom ledger;
  - `docs/next/long-line-documents.md` (PT queue: this slice, then the three
    queued changes);
  - `docs/end-user-coverage.md` if lanes change;
  - `AGENTS.md` and `README.md` (search and sidebar descriptions that state
    10,000 caps).
- **Measurements:** PT's M2, M3 and M5 code, re-run on `main` and on the
  branch, interleaved at load below 4, plus new cases at the budgets.
