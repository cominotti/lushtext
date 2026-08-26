# Test counts before and after

The completion rule requires that a migration not decrease the project test
count. Consolidating scattered `*_for_test` getters into one evidence surface
retires *functions*, not *tests*, so the count must hold or rise.

## Counting method

Three independent measures, so a change in one can be cross-checked against the
others rather than taken on trust:

1. **Executed non-widget tests** — `cargo nextest run -p lushtext-core
   --all-features`, reading the run summary. This is the number that actually
   executes, including the co-located `#[cfg(test)]` modules that moved with the
   relocated policy.
2. **Declared `#[test]` attributes in `lushtext-core`** —
   `grep -rh '#\[test\]' crates/lushtext-core/src crates/lushtext-core/tests | wc -l`.
3. **Declared `#[test]` attributes in the widget harness** —
   `grep -rh '#\[test\]' crates/lushtext/tests/ | wc -l`.

"Before" was measured by stashing the whole working tree (`git stash -u`) and
re-running the same three commands, so both sides come from one checkout and one
toolchain.

## Results

| Measure | Before | After | Delta |
| --- | --- | --- | --- |
| Executed `lushtext-core` tests | 1,372 | 1,389 | **+17** |
| Declared `#[test]` in `lushtext-core` | 1,373 | 1,390 | **+17** |
| Declared `#[test]` in the widget harness | 1,192 | 1,198 | **+6** |

No measure decreased.

## Where the movement came from

**+17 in `lushtext-core`**, all in `ui/editor_page/save/policy.rs`, which now
holds 24 `#[test]` functions: the 7 that moved with the relocated admission
policy unchanged, plus 17 new ones.

- **11 for the newly extracted pure decisions.** Seven cover the
  `QueuedSaveTicket` / `QueuedSaveFacts` seam and its predicate across current,
  re-pathed, stale-generation, not-saving, unmodified, stale-close-session, and
  no-tracked-path cases plus the explicit-destination path-comparison skip; one
  pins the `explicit_destination` versus pending-load-cancellation distinction;
  one covers the pre-emption derivation; and three cover the saved-text
  disposition across all four combinations, the capture-mode naming, and the
  write-classification default.
- **6 closing pre-existing mutation survivors** in the relocated admission
  policy: `refreshing_an_unknown_request_reports_no_match`,
  `every_admission_guard_clause_blocks_on_its_own`,
  `an_exactly_budget_sized_request_is_not_exclusive`,
  `an_exactly_budget_sized_overweight_request_needs_a_wholly_idle_lane`,
  `snapshot_counts_close_work_separately_from_ordinary_work`, and
  `documented_payload_policy_constants_hold_their_values`. See
  `mutation-parity-save-policy.md`.

11 + 6 = 17, and no assertion was folded into an existing test to make the
arithmetic work.

**+6 in the widget harness:**

- `test_save_evidence_reads_stay_side_effect_free_across_save_mutation` — the
  reentrancy proof required of an evidence surface. It drives the workflow
  through every operation that takes a mutable borrow of the state the accessor
  reads — queue, admit, terminal — reads the surface after each, and asserts that
  repeated reads of unchanged state are identical.
- Three durable-write equivalence and failure-path tests:
  `test_save_of_a_clean_unmodified_buffer_still_writes_the_buffer`,
  `test_save_failing_before_rename_keeps_previous_bytes_and_leaves_the_tab_modified`,
  and `test_save_failing_after_rename_reports_durability_unconfirmed_not_a_lost_save`.
  The first is the one `live-run.md` cites for why a clean-buffer save still
  traverses the whole workflow.
- Two teardown-observation tests added while fixing the template-child hazard on
  the sibling surfaces: `search_panel::test_evidence_reads_survive_widget_disposal`
  and `command_palette::test_evidence_reads_survive_widget_disposal`. Each was
  verified to fail against the pre-fix panicking read before being accepted.

## What was retired without losing a test

Four `*_for_test` call surfaces over three mechanisms were removed
(`save_runtime::snapshot_for_test`, `transient_save_admission_snapshot_for_test`,
`save_uses_chunked_snapshot_for_test`, `save_snapshot_inflight_for_test`). Every
call site migrated to a field on `SaveEvidence`; **no assertion was dropped and
no test was deleted**, which is why the widget-harness count moves by exactly the
one test that was added rather than falling.
