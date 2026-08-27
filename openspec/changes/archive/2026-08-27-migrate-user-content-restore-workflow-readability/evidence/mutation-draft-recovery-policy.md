# `WFR-DRAFT-RECOVERY` mutation evidence (task 6.11)

**File-level anchors only.**

## One relocation and a gain from zero, reported separately

### Relocation: the mutation-intent epoch allocator

`ui/window/draft_ordering.rs` held `DraftMutationIntent` and
`DraftMutationOrder` — 50 production lines and 69 lines of co-located tests — and
moved **whole, with its tests**, into `ui/window/drafts/policy.rs`. The file is
retired.

As with the session row, the parity answer is that the **old location was outside
the mutation scope**: `examine_globs` reach `ui/**/policy.rs`, and
`ui/window/draft_ordering.rs` was not one. So the relocated allocator's *before*
count is **0 generated**, and the move is a coverage gain rather than a parity
case. Its five relocated tests all pass unchanged, which is the behaviour-parity
half of the claim.

The allocator was task 6.3's explicit decision point: it is **this workflow's
owned policy**, not cross-cutting. Counted as owning workflows its consumers are
one — the draft workflow's journal and its two execution modules are all the same
row — so it moved rather than staying shared.

### Gain from zero: the workflow's own decisions

Extracted from the GTK adapter:

- **candidate eligibility**, including the `installation_incomplete` term that is
  the slot's confirmed data-safety fix, and the `require_draft_dirty` flag that
  separates an autosave pass from a close pass;
- **autosave admission** — mark-pending rather than queue, so a burst of ticks
  during one long pass cannot fan out;
- **the post-snapshot freshness predicate**, which chunked capture makes necessary
  because it spans main-loop turns;
- **close-flush and readiness gating**;
- **pipeline failure accounting** and the user-facing retryable message;
- **orphan-cleanup continuation**, its exponential backoff, and its cap;
- **the grouped cleanup failure message**, which must not leak a path.

## The numbers

| Quantity | Before | After |
| --- | --- | --- |
| Mutants generated in `ui/window/drafts/policy.rs` | **0** (no `policy.rs` existed at this path; the decisions were inline in `ui/window/drafts.rs`, and the relocated allocator's old home was also outside `examine_globs`) | **54** |
| Missed | 0 | **0** |

## Survivor accounting — every one triaged, none excluded

The first run left **1 survivor in this module**, and it is the cleanest instance
of the pattern all four rows hit:

| Survivor | Why it survived | Closed by |
| --- | --- | --- |
| `ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF`'s `15 * 60` | `cleanup_backoff_doubles_and_then_caps` asserted `delay == ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF`, so `*` becoming `+` moved **both** sides of the comparison from 900 s to 75 s and the assertion still held | the same test now also pins the cap as `Duration::from_secs(900)` and the base delay as `Duration::from_secs(30)`, with the user-facing reason recorded: a user whose storage came back should not wait an hour for cleanup to resume, and a permanently unavailable volume should not be retried every thirty seconds forever |

**Zero exclusions**, and no equivalent mutants in this module — every one of its
54 was killable.

### Final numbers

After closing the survivor, the confirming diff-scoped run reports
**246 mutants tested, 230 caught, 16 unviable, 0 missed** across all four slot-4
policy modules; this row's share is **54 generated, 0 missed**.

Notably, all 15 mutants on
`draft_candidate_is_eligible` and `captured_snapshot_is_current` — the two
predicates that decide whether a buffer may be written over a draft — were
**killed on the first run**, by the tests written alongside the extraction.

## Post-review addendum: the `cleanup_types.rs` coverage regression (finding B1)

The independent review found that this change deleted five unit tests with no
surviving equivalent, two of which were the **only** coverage of
`services/draft_service/cleanup_types.rs::merge_committed_orphan_removals` — the
function deciding which manifest entries a *destructive* orphan-cleanup pass
removes. The file had dropped to **zero** tests while sitting inside the
`services/**` mutation `examine_globs`.

**Why this change's own mutation runs could not have caught it.** Every mutation
run reported above is *diff-scoped*: `make mutants-diff` generates mutants only
for changed code. `cleanup_types.rs` was **not modified** by this change — the
tests that covered it lived in the deleted `ui/window/drafts.rs`. So the file was
never in any diff, never had a mutant generated, and its coverage falling to zero
was invisible to the exact tooling meant to detect weak coverage. **A
relocation-heavy change must check test *survivorship*, not only its net test
delta and its diff-scoped mutant counts.**

### Proof the regression is closed

```
$ MUTANTS_RE='cleanup_types' MUTANTS_JOBS=1 MUTANTS_TEST_THREADS=2 \
    MUTANTS_BUILD_JOBS=2 ./scripts/run-mutants.sh full
50 mutants tested in 16m: 16 missed, 33 caught, 1 unviable
```

Per-file outcomes, from `mutants.out/{caught,missed,unviable}.txt`:

| File | Caught | Missed | Unviable |
| --- | --- | --- | --- |
| **`services/draft_service/cleanup_types.rs`** | **15** | **0** | 1 |
| `services/draft_service.rs` | 6 | 5 | 0 |
| `services/file_tree.rs` | 1 | 11 | 0 |
| `ui/window/session_restore/policy.rs` | 2 | 0 | 0 |
| others (`local_history_service`, `markdown_render`, `palette/notes`, `single_flight`) | 9 | 0 | 0 |

**`cleanup_types.rs` is 15 caught, 0 missed** — from 0 generated-and-covered
before this addendum.

**The 16 misses are the documented pre-existing floor, not a new regression.**
They are entirely in `services/draft_service.rs` (5) and `services/file_tree.rs`
(11), the same two files named in the friction record's entry on cargo-mutants
27's `--re` filter not applying to struct-field-deletion mutants. `file_tree.rs`
is slot 5's row. A focused run carries this floor; do not attribute it to a
draft-recovery change.

### The five restored tests, and what closed the gap

Restored to homes matching the new structure, each strengthened rather than
pasted back — full table in `test-counts.md`. Beyond restoring them, three tests
were added specifically to kill mutants the originals could not:

- **`cleanup_merge_requires_every_fingerprint_dimension_to_match`** perturbs
  `original_mtime_secs`, `original_path`, and `saved_at_secs` **one at a time**.
  The two restored merge tests differ in all three fields simultaneously, so a
  mutant deleting a single comparison from `DraftEntryFingerprint::matches` would
  survive both — and that mutant is precisely the one that would let a
  destructive pass delete a *newer* same-ID autosave.
- **`cleanup_merge_with_no_commits_removes_nothing`** pins the empty-commit-set
  case as a no-op rather than a manifest clear.
- **`confirmed_cleanup_count_saturates_instead_of_overflowing`** pins the
  saturating add that the diagnostic count depends on.

Non-widget suite after the restoration: **1,670 passed, 11 skipped**, up 13 from
1,657 — 5 restored plus 8 added (3 fingerprint/no-op/saturation, 3 for the S5
grouping-walk relocation, 1 seams dimension coverage folded into the restored
test, 1 local-history permit assertion pair).
