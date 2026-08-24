# Mutation parity — search-panel policy relocation (task 5.3)

Post-relocation counterpart to `mutation-baseline-search-policy.md`. The
exemplar's two pure policy modules, `crates/lushtext-core/src/model/search_flight.rs`
and `crates/lushtext-core/src/model/search_retirement.rs`, are now
`crates/lushtext-core/src/ui/search_panel/policy.rs`, reached by the
`ui/**/policy.rs` naming convention in `.cargo/mutants.toml` rather than by a
hand-listed path.

Captured on 2026-08-24 with `cargo-mutants 27.0.0`, immediately after the
relocation and **before** the Replace All preview seam was added to the same
module, so the two runs describe the same population.

## Scope re-verification (required before the full run)

```
MUTANTS_RE='crates/lushtext-core/src/ui/search_panel/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list
```

Listed exactly 15 mutants, all in `ui/search_panel/policy.rs`. The two-part
scoping is still required: cargo-mutants 27's `--re` does not filter the
`delete field` mutant kind, and the services glob exclusion is what reduces the
listed scope to the target module. The concern recorded in the baseline — that
`policy.rs` entering the examined set might change what the exclusion removes —
did not materialize: the population is the same 15 mutated operations on the
same functions.

## Result

```
MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
MUTANTS_RE='crates/lushtext-core/src/ui/search_panel/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh full
```

```
15 mutants tested in 4m: 1 missed, 12 caught, 2 unviable
```

| Outcome | Before (in `model/`) | After (in `ui/search_panel/policy.rs`) |
| --- | --- | --- |
| Generated (tested) | 15 | 15 |
| Caught | 12 | 12 |
| Missed (survived) | 1 | 1 |
| Unviable | 2 | 2 |
| Timeout | 0 | 0 |

Parity holds on counts and on the mutated operation plus function name. Line
numbers differ because the modules were concatenated into one file; the mutated
functions are unchanged.

- The single survivor is the same pre-existing one:
  `replace SearchRetirementSliceBudget::exhausted -> bool with true`
  (`ui/search_panel/policy.rs:139:9`, previously
  `model/search_retirement.rs:36:9`). It was carried forward as-is, per the
  baseline's instruction; the caught count did not change, so no test was added
  to close it and none was lost.
- The two unviable mutants are the same two `WorkspaceSearchFlight::submit` /
  `finish` `Default::default()` substitutions, still unviable for the same
  reason: the returned types do not implement `Default`. No derive changed.

## Seam value object added after parity was proven

Task 5.4 added `ReplacePreviewTicket`, `ReplacePreviewFacts`, and their
`is_current` / `may_dispatch` / `supersede` operations to the same `policy.rs`.
That is new pure logic in a module the mutation scope now reaches, so it
enlarges the module's mutant population beyond the 15 above. It is excluded from
the parity comparison, which is about the relocated logic, but it was measured
with the same scoping after the seam landed:

```
29 mutants tested in 4m: 1 missed, 25 caught, 3 unviable
```

| Population | Generated | Caught | Missed | Unviable |
| --- | --- | --- | --- | --- |
| Relocated logic only (parity) | 15 | 12 | 1 | 2 |
| Whole `policy.rs` after the seam landed | 29 | 25 | 1 | 3 |

Every one of the 14 additional mutants is either caught or unviable; the only
survivor is still the pre-existing `exhausted -> true` one, at its new line
`policy.rs:142:9`. The new coverage comes from five unit tests in
`policy::preview_ticket_tests` (each freshness clause rejected independently,
option drift under the same generation, the dispatch-versus-publish difference,
and supersession of a retained request).

## Task 6.3: `make mutants-diff` against `origin/main`

Task 5.3 proved *relocation parity* with focused scoping, because that is the
only way to address the same 15-mutant population on both sides of a file move.
Task 6.3 additionally runs the changed-code lane the way the task names it, so
the change is checked by the same gate a reviewer would run.

### Tooling limitation found, and the exact commands used

`make mutants-diff` calls `scripts/run-mutants.sh diff`, whose `ensure_diff_file`
generates the diff with:

```
git diff origin/main...
```

`git diff A...` is shorthand for `git diff A...HEAD`: it compares the merge base
to **`HEAD`**, not to the working tree. While this change had uncommitted
working-tree edits, cargo-mutants read the diff's line numbers against the
on-disk source and refused the run:

```
ERROR Diff content doesn't match source file: crates/lushtext-core/src/ui/search_panel/policy.rs line 407
diff has:   "    fn zero_budget_and_large_available_counts_saturate_safely() {"
source has: "    fn partially_consumed_budget_still_has_room_in_the_same_turn() {"
The diff might be out of date with this source tree.
```

This is a limitation of the wrapper's diff generation against a dirty worktree,
not of the relocation. The lane works unchanged on a clean checkout, which is how
CI and a reviewer on a committed tree will hit it. Two runs were therefore
recorded:

1. Against `HEAD` (the committed migration), through the plain target:

```
make mutants-diff
```

2. Against the merge base **including** the working tree, by passing an
   explicitly generated diff to the same wrapper:

```
git diff "$(git merge-base origin/main HEAD)" > worktree.diff
MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
  ./scripts/run-mutants.sh diff worktree.diff
```

Both runs found the same 29-mutant changed-code population. The in-scope changed
files are `model/mod.rs`, `services/session_service.rs`,
`ui/search_panel/policy.rs`, and `ui/search_panel/test_policy.rs`; every
generated mutant landed in `policy.rs`, which is the relocated module plus its
new seam.

### Results

```
# run 1, HEAD only
29 mutants tested in 5m: 1 missed, 25 caught, 3 unviable

# run 2, after the survivor was closed
29 mutants tested in 5m: 26 caught, 3 unviable
```

| Run | Generated | Caught | Missed | Unviable | Exit |
| --- | --- | --- | --- | --- | --- |
| `mutants-diff` at `HEAD` | 29 | 25 | 1 | 3 | non-zero (survivor) |
| `mutants-diff` including worktree | 29 | 26 | 0 | 3 | zero |

Run 1 reproduces the whole-`policy.rs` figures recorded in the previous section
exactly (29 / 25 / 1 / 3), which independently confirms the focused scoping used
for the parity proof addressed the same population the changed-code lane does.
The focused scoping remains the authoritative *parity* proof, because only it can
name the pre-move population; `mutants-diff` cannot, since the pre-move files do
not exist in the diff's after-state.

### The baseline survivor is closed

Run 1's single survivor is the one the baseline carried forward:

```
crates/lushtext-core/src/ui/search_panel/policy.rs:142:9:
  replace SearchRetirementSliceBudget::exhausted -> bool with true
```

Task 6.3 requires a clean `mutants-diff`, so it was closed rather than carried
further. No production code changed. One pure unit test was added,
`policy::retirement_budget_tests::partially_consumed_budget_still_has_room_in_the_same_turn`,
which asserts the previously unasserted half of the predicate: a slice that has
released some rows but not its whole budget must still report room, or one
bounded retirement turn would stop after its first ownership category and the
disposer would crawl one value per GTK turn.

Per the baseline document's instruction, this is recorded as a caught-count rise
from an **added test** (25 -> 26), not from a scope change: the generated and
unviable counts are unchanged at 29 and 3.

## Task 6.5 follow-up: the result-cap fix enlarges the changed-code population

Closing task 6.5 required fixing the pre-existing result-cap defect described in
`tasks.md`, which changed `services/content_search/search.rs` — a file the
default mutation scope already reaches through `services/**`. The changed-code
lane was therefore rerun with the same worktree-diff workaround documented
above:

```
git diff "$(git merge-base origin/main HEAD)" > worktree.diff
MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
  ./scripts/run-mutants.sh diff worktree.diff
```

```
38 mutants tested in 5m: 35 caught, 3 unviable
```

| Population | Generated | Caught | Missed | Unviable | Exit |
| --- | --- | --- | --- | --- | --- |
| before the cap fix | 29 | 26 | 0 | 3 | zero |
| after the cap fix | 38 | 35 | 0 | 3 | zero |

The nine additional mutants are exactly the new `WalkStop` seam plus the
enclosing walker entry point, and every one is caught:

```
search.rs: WalkStop::stopped -> true / -> false
search.rs: replace || with && in WalkStop::stopped (both clauses)
search.rs: WalkStop::claim_incomplete -> true / -> false / delete !
search.rs: WalkStop::record_result_cap with ()
search.rs: search_with_plan_and_limits with ()
```

The three unviable mutants are unchanged (the two `WorkspaceSearchFlight`
`Default::default()` substitutions plus the `ReplacePreviewTicket::query_spec`
borrow), and there are still no survivors. The tests that close the new mutants
are the two service unit tests added with the fix,
`result_cap_terminates_without_touching_the_caller_cancel_flag` and
`walk_stop_separates_caller_cancellation_from_service_termination`, together
with the existing `result_cap_at_10000`, `cancel_stops_search`, and
`ambiguous_fallback_stops_before_one_over_entry_limit` coverage.
