# Task 10.2 — test counts before and after

The completion rule requires that the project test count not decrease. It did not;
it rose in both lanes.

Counts were taken with the same commands on both sides of the change, on the same
checkout, with `HEAD` (`2140a4e`, slot 2a's archive commit) as the "before" state
because this change is uncommitted.

## Non-widget lane

```
cargo nextest list --workspace --all-features --message-format json \
  | python3 -c "import sys,json; d=json.load(sys.stdin); \
      print(sum(len(v['testcases']) for v in d['rust-suites'].values()))"
```

| State | Count |
| --- | --- |
| before (`HEAD`) | 1,576 |
| after | 1,606 |
| **delta** | **+30** |

Where the 30 came from:

| Added tests | Count |
| --- | --- |
| `ui/search_panel/policy.rs::replace_weight_tests` — the moved Replace All weights, the undo reservation plan, the journal generation predicate, `ReplaceApplyCounts` | 11 |
| `services/search_backup.rs::tests` — the de-duplicated pure rules (activation with and without manifest agreement, payload budget at/over/saturating, entry-count cap at/over, retained-weight cap at/over, payload-file filter, dedup by entry file and by target path including the accounted-for rejected row, cleanup eligibility) | 10 |
| `services/search_backup.rs::tests` — the `shrink_journal_to` path (retained entry inode preserved, interrupted-before-cleanup still active, fallback when not a superset, fallback with a cleanup marker, shrink-to-empty deletes) | 5 |
| `services/search_backup.rs::tests` — startup-recovery states task 10.7 named that had no explicit test (duplicate target path, incremental journal over the retained-memory cap) | 2 |
| `services/content_search/replace.rs::tests` — the rollback content-validation fix (per-disposition classification, journal kept when a target changed mid-rollback) | 2 |
| **Total** | **30** |

## Widget lane

**Counting method, because a plausible one is wrong here.** Grepping the lane's
output for `^test .* \.\.\. ok$` undercounts badly — the custom harness prints some
results as a bare `ok` line without the `test <name> ...` prefix, so that grep
reports ~625 against 1,116 actually registered. The authoritative count is the
harness's own registry:

```
cargo build -p lushtext --tests --all-features
./target/debug/deps/widget-<hash> --list | tail -1     # "<N> tests, 0 benchmarks"
```

cross-checked against the `#[test]` attributes the build script registers from:

```
# after
grep -h -c '^#\[test\]' crates/lushtext/tests/widget/*.rs | paste -sd+ | bc
# before
for f in $(git ls-tree -r --name-only HEAD crates/lushtext/tests/widget/); do \
    git show "HEAD:$f" | grep -c '^#\[test\]'; done | paste -sd+ | bc
```

Both methods agree on the "after" figure.

| State | Count |
| --- | --- |
| before (`HEAD`) | 1,109 |
| after | 1,116 |
| **delta** | **+7** |

Where the 7 came from, all in `crates/lushtext/tests/widget/search_panel.rs`:

| Added test | Finding |
| --- | --- |
| `test_replace_all_applies_only_checked_rows_and_records_apply_counts` | task 10.5 partial-check case |
| `test_evidence_reports_transaction_state_separately_from_preview_pending` | task 7.1 |
| `test_evidence_reads_stay_side_effect_free_across_journal_mutation` | tasks 7.2, 7.3 |
| `test_partial_undo_shrinks_the_journal_and_keeps_the_skipped_entry_on_disk` | review finding F7 |
| `test_replace_all_with_no_matches_writes_nothing_and_journals_nothing` | review finding F14 |
| `test_replace_all_one_match_one_file_writes_and_journals_exactly_that_file` | review finding F14 |
| `test_replace_all_many_matches_many_files_writes_and_journals_every_file` | review finding F14 |

The second-undo-refusal case task 10.5 names was added as assertions inside the
existing `test_replace_all_then_undo_restores_original_file_bytes` rather than as
a new test, because it must follow a completed undo in the same session.

**Zero `FLAKY:` lines** across every full widget-lane run of this change.

## Test seams

No `*_for_test` function was retired (slot 1 already retired all eight inspection
seams) and **none was added**. The five replace/undo actuation seams and two
accessibility probes are unchanged, so `ui/search_panel/**` still holds 7.
