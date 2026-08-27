# `WFR-BUFFER-REPLACEMENT` mutation evidence (tasks 3.3, 3.8)

**File-level anchors only.** Line-precise anchors freeze the file against any
later edit, so nothing below names a line number as an identity.

## Task 3.3 — the `policy: none` probe, and its answer

The change's spec delta requires a workflow's pure logic to be **entirely**
cross-cutting before the row is complete without a `policy.rs`, so the outcome had
to be a finding with evidence rather than the starting premise. The probe examined
the three candidates task 3.3 names, plus two the read turned up.

| Candidate | Verdict |
| --- | --- |
| slice accounting in `delete_one_slice` | **split.** The *iterator* work is inseparable from `TextIter` and stays in `execution`. The *metrics accounting* — saturating clear/slice counts, the installed-byte **high-water mark** rather than a sum, and the direct-replacement record — is pure and moved to `policy` as `BufferReplacementMetrics`'s recorders |
| terminal classification in `finish_session` | **extracted.** `terminal_is_complete` and `guard_restores_on_terminal` are pure predicates over `Option<BufferReplacementCancelReason>`, and the second is load-bearing: a disposed page must **not** have its editability, cursor, syntax highlighting, or file monitor restored |
| supersession in `clear_owner_and_start_pending` / `replace_buffer_bounded` | **extracted** as `start_disposition`. The decision is "does a session still own this editor after the cancellation attempt", which decides whether the newcomer starts now or parks — and parking behind nothing would hang the caller |
| the cancellation disposition (found by the read, not in the task list) | **extracted** as `cancel_disposition`. This is the workflow's single most consequential decision: a session that has already mutated the buffer owes the user a bounded clear pass, because a half-installed document must never be left visible |
| the phase-transition and turn-admission rules (also found by the read) | **extracted** as `after_clear_slice`, `insertion_is_complete`, and `turn_may_run`. `turn_may_run` encodes the deliberate asymmetry that a *cancelled* clear runs even after the caller's own freshness check has gone stale |

**So `policy: none` is not this row's answer.** The probe found five separable
pure decisions in the GTK adapter, four of them determining whether a partial
buffer can be seen or whether a caller learns the truth about its terminal — and
all five were previously unreachable by mutation testing. Extracting them is an
ordinary **gain-from-zero** extraction, and the row declares
`ui/editor_page/buffer_replacement/policy.rs`.

Note the asymmetry with slot 3b's `file_load.rs` decision, which the task flagged:
that module stayed in `model/` *and* its workflow still owned a `policy.rs`. "The
domain module stays" has never by itself implied "the workflow owns no policy",
and this row confirms it from the other side.

**Consequence for the amendment.** Amendment (a) — a row whose only pure policy is
cross-cutting is still complete — therefore ships **stated but not exercised**.
It remains correct (the gate already tolerated `policy: none`, and the spec now
says such a row is complete), but slot 4 provides no first user. Recorded as a
friction note for slots 5 through 7.

## Task 3.8 — the numbers

### Gain from zero: the new `policy.rs`

Invocation, run to completion on a clean-of-other-work tree:

```
MUTANTS_RE='buffer_replacement/policy\.rs' MUTANTS_JOBS=2 MUTANTS_TEST_THREADS=4 \
  ./scripts/run-mutants.sh full
```

| Quantity | Before | After |
| --- | --- | --- |
| Mutants generated in `ui/editor_page/buffer_replacement/policy.rs` | **0** (the file did not exist; the logic was inline in a GTK adapter, which the mutation scope deliberately excludes) | **19** |
| Killed | 0 | **15** |
| Missed | 0 | **0** |
| Unviable | 0 | **4** |

The 4 unviable are all the same shape —
`replace <fn> -> <Enum> with Default::default()` for
`for_one_retained_body`, `start_disposition`, `cancel_disposition`, and
`after_clear_slice` — and are unviable because those return types deliberately
implement no `Default`. That is the correct design: there is no sensible default
start disposition, cancel disposition, or clear progress, and inventing one to
make a mutant viable would weaken the types.

Reachability confirmed twice: `make check-workflow-boundaries` reports
**"5 workflow policy module(s) are pure and mutation-scoped"** (up from 4), and
`make mutants-list` names 19 mutants under the new path — so the depth-agnostic
`ui/**/policy.rs` glob resolves at `ui/editor_page/buffer_replacement/policy.rs`,
its third adopting directory.

### Relocation parity: `model/buffer_replacement.rs` is unchanged

The amendment's no-duplication rule means the cross-cutting module's existing
coverage must neither drop nor be re-generated under a new path. It is unchanged
by construction: **the file was not edited by this migration** (`git status` lists
`model/file_load.rs` only, for the task 2.1 doc comment). Its
`next_replacement_boundary`, `next_clear_char_count`, and
`BufferReplacementPlan::for_sizes` keep their five co-located unit tests and their
single implementation, called from three owning workflows and duplicated by none.

**No relocation parity numbers are owed for this row**, because no pure logic was
relocated — the extraction is out of a GTK adapter, which was out of scope, into a
policy module, which is in scope. Reported separately from the gain above exactly
as the task requires.

### The diff-scoped run (task 10.4)

`make mutants-diff` reports `No diff hunks found; skipping changed-code mutation
run.` for uncommitted work, because `scripts/run-mutants.sh` builds its diff from
`git diff "${MUTANTS_BASE}..."`. The documented working-tree workaround, which
slot 3b established:

```
$ git add -N crates/lushtext-core/src/ui/editor_page/buffer_replacement/
$ git diff origin/main -- crates/ > /tmp/worktree.diff
$ MUTANTS_JOBS=2 MUTANTS_TEST_THREADS=4 ./scripts/run-mutants.sh diff /tmp/worktree.diff
```

`git add -N` is required because this change adds whole new files, and `git diff`
without it omits untracked paths entirely — which is precisely how a migration can
appear to have no changed code to mutate.

Result:

```
Found 19 mutants to test
ok       Unmutated baseline in 56s build + 10s test
19 mutants tested in 4m: 15 caught, 4 unviable
```

**Zero missed.** The diff-scoped set is exactly the 19 policy mutants and nothing
else — no field-deletion floor, because diff mode restricts by hunk — which is the
cleanest possible confirmation that every line of pure logic this change added is
covered.

## Two corrections made during the migration, recorded because they change the numbers

**1. A tautological policy function was removed.** The first cut extracted
`terminal_is_complete(cancellation) -> bool`, which is `cancellation.is_none()`.
It bought no safety, and it forced a **dead default** at the call site
(`cancellation.unwrap_or(Stale)` in a branch where `cancellation` is always
`Some`). Replacing it with an exact `match cancellation { None => Complete,
Some(reason) => Cancelled { reason } }` removes the dead arm and reads better.
That is why the generated count is **19, not 21**: it took its 2 mutants with it.
The claim of five separable pure decisions is unaffected — terminal
classification survives as `guard_restores_on_terminal`, which encodes the real
decision (a disposed page must **not** have its projections restored).

**2. A latent `BorrowMutError` was introduced and caught.** The first cut wrote
`match start_disposition(editor.imp().replacement.active.borrow().is_some())`.
A `match` scrutinee's temporaries live for the **whole match**, so that shared
`Ref` on `active` was still alive when the `Immediately` arm called `begin`,
which takes `borrow_mut()` on the same cell — a runtime panic on exactly the path
where a superseded session terminated inside its own turn. The pre-migration code
was safe because a plain `if` condition's temporary drops before the block.

The boolean is now read into a local before the match, with the reason recorded at
the site. **The pre-existing widget coverage would have caught it**:
`test_bounded_buffer_replacement_supersession_publishes_only_latest_body` calls
`replace_buffer_for_test` twice synchronously, which is exactly that path. It and
the five neighbouring cancellation/supersession/disposal tests were re-run
against the fixed build and all pass. Recorded because it is a general hazard of
this convention's mechanical work: **moving an `if` condition into a `match`
scrutinee silently extends a borrow's lifetime.**

## Pre-existing survivors in the same run, recorded rather than absorbed

The focused (`--re`) run's total was 53 mutants, not the policy module's own count: **cargo-mutants 27's
`--re` filter does not apply to its struct-field-deletion mutants**, so 32 of
those ran regardless of the regex. That is a tooling observation worth handing
forward — a "focused" run is focused plus that floor.

Of those 32, **16 survived** — **11 in
`crates/lushtext-core/src/services/file_tree.rs` and 5 in
`crates/lushtext-core/src/services/draft_service.rs`** — all of the form *"delete
field X from struct Y expression in Z"*. **None is in a file this change
touches**, and none is in the extracted policy module. They are pre-existing
baseline survivors of the configured full scope, not a regression introduced here,
and they are outside this change's row scope: `services/draft_service.rs` is the
shared draft service (slot 4 leaves its behavior unchanged by design) and
`services/file_tree.rs` belongs to `WFR-WORKSPACE-TREE` (slot 5).

Recorded here so a later slot reading `mutants.out` does not attribute them to
this change, and so slot 5 knows `file_tree.rs`'s field-deletion survivors are
waiting for it.

## Confirmed again after the whole slot landed

Re-run with all four slot-4 rows migrated: **246 mutants tested, 230 caught, 16
unviable, 0 missed** across the four new `policy.rs` modules, of which this row's
share is 19 generated and 0 missed. The count is 246 rather than 248 because two
equivalent mutants disappeared with the code that made them equivalent — see the
session-restore and local-history evidence files.
