# `WFR-SESSION-RESTORE` mutation evidence (task 4.9)

**File-level anchors only.** Line-precise anchors freeze the file against any
later edit, so nothing below names a line number as an identity.

## The two categories, reported separately

Task 4.3 required deciding whether the bounded-turn policy was *already pure and
merely mislocated* — a relocation owing parity — or *partly inline in the GTK
adapter* — an extraction owing gain-from-zero. **It was both**, in two clearly
separable halves, so both figures are reported.

### Relocation: the bounded-turn admission policy

`SessionRestorePolicy`, `SessionRestorePlanPermit`, `SessionRestoreAdmission`,
`SessionRestoreTurn`, and the two bounded limits moved from
`ui/window/session_restore.rs` into
`ui/window/session_restore/policy.rs` **with their five co-located unit tests**.

The parity question has an unusual answer here, and it is worth stating plainly:
**the old location was not in the mutation scope.** `.cargo/mutants.toml`'s
`examine_globs` reach `ui/**/policy.rs`, and `ui/window/session_restore.rs` was
not a `policy.rs`. So the relocated code's *before* count is **0 generated**, not
"the same as after" — this is a relocation in the source tree that is a
**coverage gain** in the mutation scope. Recording it as parity would claim a
before-figure that never existed.

### Gain from zero: the journal's pure half

Extracted from the GTK adapter, none of it previously policy anywhere:

- `session_tab_identity` and its `SessionTabIdentity` — including the guard that
  an **empty** draft ID is *no* identity, because two untitled tabs with empty IDs
  are different documents and merging them would drop one of the user's buffers;
- `index_session_tabs` and `merge_session_tab`;
- `merge_persisted_session_with_current` — the close-time merge that preserves
  descriptors a still-running restore never reached;
- `startup_preloads_retained_bytes` and `fit_startup_preloads_to_reservation`;
- `startup_recovery_status_message`.

## The numbers

Invocation, using the documented working-tree workaround (`make mutants-diff`
reports `No diff hunks found` for uncommitted work, and `git add -N` is required
because this change adds whole new files):

```
$ git add -N crates/lushtext-core/src/ui/window/session_restore/ ...
$ git diff origin/main -- crates/ > /tmp/worktree.diff
$ MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 ./scripts/run-mutants.sh diff /tmp/worktree.diff
```

| Quantity | Before | After |
| --- | --- | --- |
| Mutants generated in `ui/window/session_restore/policy.rs` | **0** (no `policy.rs` existed at this path, and the admission policy's old home was outside `examine_globs`) | **83** |
| Missed | 0 | **0** |

Reachability confirmed twice: `make check-workflow-boundaries` reports **8**
workflow policy modules that are pure and mutation-scoped, up from 4 before
slot 4; and `make mutants-list` names 83 mutants under the new path, so the
depth-agnostic `ui/**/policy.rs` glob resolves under `ui/window/` for the first
time.

## Survivor accounting — every one triaged, none excluded

The first run left **19 survivors in this module**, all of one kind: *methods and
predicates the relocated tests exercised only indirectly.* The relocated tests
drove the policy end to end and asserted its outputs, which is why they passed
mutation on the planner itself but not on the accessors and guards feeding it.
Each was closed with a test that pins the behaviour, not with an exclusion.

| Survivor group | Why it survived | Closed by |
| --- | --- | --- |
| `SessionRestorePlanPermit::generation`, `SessionRestorePolicy::generation` (4 mutants) | every existing assertion compared a permit's generation *against the policy's*, so both could report the same wrong value | `a_permit_and_its_policy_report_the_generation_they_were_created_for` — asserts the literal generation on each, and that a permit from a different generation is refused |
| `SessionRestoreTurnMetrics` `generation` / `total_descriptors` field-deletion (2) | the metrics were read for turn counts and permit counts, never for identity or the starting total | `total_descriptors_counts_what_the_generation_started_with` — including the empty-generation case, so the field cannot default from another |
| `needs_next_turn` (6) | never called directly; the tests drove `plan_turn` in a loop instead | `needs_next_turn_distinguishes_untitled_from_planning_saturated` — both halves of the disjunction and the `<` boundary, plus terminal and cancelled |
| `pending_descriptors` returning `vec![]` (1) | the close-time snapshot path is adapter code; the policy method had no direct test | `pending_descriptors_reports_unmounted_tabs_at_their_real_ordinals` — an empty return here would silently drop the rest of the user's session |
| `plan_turn`'s `terminal \|\| cancelled` guard (1) | cancellation sets **both** flags, so only a terminal-but-not-cancelled generation distinguishes them | `plan_turn_refuses_for_a_cancelled_generation_as_well_as_a_terminal_one` |
| `note_terminal_projection_publication`'s guard (1) | the existing test covered exactly-once but not the two refusal conditions | `terminal_projection_publication_refuses_before_terminal_and_after_cancel` |
| `refresh_metric_counts` replaced with `()` (1) | it returns nothing, so only its *effect* can catch it | `metric_counts_track_the_live_queue_and_permits_after_every_transition` |
| `session_tab_identity`'s `!is_empty()` (1) | no test used an empty draft ID | `an_empty_draft_id_is_not_a_merge_identity` — and asserts the merge keeps both such tabs |
| `merge_persisted_session_with_current`'s `< len()` bound (3) | the merge tests never used an out-of-range persisted active index | `a_persisted_active_index_past_the_merged_end_is_dropped` — the in-range, one-past, and zero cases |
| `startup_recovery_status_message`'s `> 0` thresholds (2) | every existing case had more than one diagnostic in more than one category | `recovery_summary_distinguishes_one_issue_from_none` |

### Three genuinely equivalent mutants, and what was done instead of excluding them

Three survivors could not be killed by any test, because the mutation produced
**identical observable behaviour**. Rather than record them as accepted
equivalents, each was resolved by making the code state what it actually decides
— which is the better outcome, since an unreachable term is untestable by
definition:

| Survivor | Why no test could kill it | Resolution |
| --- | --- | --- |
| `needs_next_turn`'s `terminal \|\| cancelled` → `&&` | `cancel()` sets **both** flags, and a terminal generation always has an empty pending queue, so the second term could never change the answer | the guard now checks `terminal` alone, with a `debug_assert!` stating the invariant and a test pinning that cancellation leaves nothing pending. `plan_turn` genuinely needs both, because it counts a turn *before* it inspects the queue — and that mutant **was** killed |
| `refresh_metric_counts` → `()` | it mirrored two counters into `turn_metrics` that `metrics()` recomputes from the live collections on every read, so no observer could ever see the stored copy | the function is **deleted**. `metrics()` derives both fields at read time, and its doc comment now says why a mirror would be dead weight |

The remaining equivalent — `current_window_dimension`'s guard — belongs to the
local-history row and is recorded there.

**Zero exclusions.** No mutant was written off as out of scope, and no
`MUTANTS_EXCLUDE` entry was added.

### Final numbers

After closing every survivor, the confirming diff-scoped run reports
**246 mutants tested, 230 caught, 16 unviable, 0 missed** across all four slot-4
policy modules; this row's share is **83 generated, 0 missed**. The
`startup_recovery_status_message` `preserved > 0` threshold needed one more test
than the first attempt, because `RecoveryDiagnostic::repair_skipped` *always*
preserves in place — the unpreserved-skipped case has to be constructed
explicitly, which is itself informative about why no other test reached that arm.
