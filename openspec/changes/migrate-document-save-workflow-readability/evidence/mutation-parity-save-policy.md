# Mutation parity: the save workflow's pure policy

Evidence for `migrate-document-save-workflow-readability` (slot 3a), covering the
relocation of `crates/lushtext-core/src/model/save_admission.rs` to
`crates/lushtext-core/src/ui/editor_page/save/policy.rs` and the pure decisions
newly extracted into that same module.

**Two claims, reported separately, because mixing them makes both unreadable.**
The relocation is a **parity** claim: the same mutants must be generated and the
same ones killed. The extraction is a **gain from zero** claim: those mutants did
not exist before, so there is no baseline to be at parity with, and the standard
is that they are fully killed.

## Scope re-verification

The default mutation scope reaches pure policy through the literal glob
`crates/lushtext-core/src/ui/**/policy.rs` in `.cargo/mutants.toml`. This change
is the first to put a `policy.rs` at a **nested** path
(`ui/editor_page/save/policy.rs` rather than `ui/<workflow>/policy.rs`), so the
glob's reach was verified mechanically rather than assumed. A silent no-match
would have looked like a clean run while deleting coverage.

```
$ ./scripts/run-mutants.sh list | grep -c 'ui/editor_page/save/policy.rs'
58
$ make check-workflow-boundaries
workflow boundary policy passed: 3 workflow policy module(s) are pure and
mutation-scoped, every migrated matrix row names complete, existing roles, and
the programme record's slot ledger agrees with the matrix
```

Both tools accept the nested path: cargo-mutants' globset generates mutants for
it, and the boundary checker counts it as one of the three pure, mutation-scoped
policy modules. No `.cargo/mutants.toml` entry was added, changed, or widened —
the config's own comment states that no new entry is needed when a workflow
migrates, and that holds for a nested role home too.

## Commands run

Baseline, in a clean worktree at the pre-change commit (`4fcca96`):

```
$ git worktree add <scratch>/base HEAD
$ cd <scratch>/base
$ ./scripts/run-mutants.sh list | grep -c 'model/save_admission.rs'
42
$ MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 \
    MUTANTS_RE='model/save_admission\.rs' ./scripts/run-mutants.sh full
```

After, in the working tree:

```
$ ./scripts/run-mutants.sh list | grep -c 'ui/editor_page/save/policy.rs'
58
$ MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 \
    MUTANTS_RE='ui/editor_page/save/policy\.rs' ./scripts/run-mutants.sh full
```

`MUTANTS_RE` matches against the `--list` output, and the regex is not anchored,
so both runs also swept a small number of unrelated `services/` mutants that
happened to match. Per-file counts below were extracted from
`mutants.out/outcomes.json` and filtered to the file under test, so the unrelated
sweep does not contaminate either figure.

**`make mutants-diff` reported "No diff hunks found; skipping changed-code
mutation run"** on this change, because the wrapper diffs against `origin/main`
and every edit here is an uncommitted working-tree change. That is the workaround
case the task list anticipated. The changed-code run was therefore driven with an
explicit diff file:

```
$ git add -A && git diff --cached origin/main > <scratch>/change.diff && git reset
$ MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 \
    ./scripts/run-mutants.sh diff <scratch>/change.diff
Found 58 mutants to test
58 mutants tested in 6m: 2 missed, 53 caught, 3 unviable
```

That agrees with the focused per-file run below: the same two equivalent mutants
survive, and nothing else does.

`make mutants-diff` was also not used for the parity claim. It diffs against
`origin/main`, and a file that *moves* appears to it as a wholly new file with no
baseline, which is exactly the comparison this evidence has to make. The
worktree-baseline form above is the documented workaround and is what produces a
real before/after pair.

## Claim 1 — relocation parity

Mutant **generation** is identical. The two `--list` sets were compared by mutant
description with the file prefix stripped:

| Set | Count |
| --- | --- |
| Generated at baseline, `model/save_admission.rs` | 42 |
| Generated after, `ui/editor_page/save/policy.rs` | 58 |
| Baseline mutants with no counterpart after (**lost**) | **0** |
| Mutants present only after (**gained**, claim 2) | 16 |

Zero lost. Every mutant the module generated in `model/` is still generated in
its new home, operator for operator and function for function; only line numbers
shifted.

Mutant **outcomes** for the relocated items:

| Outcome | Baseline | After relocation | After added tests |
| --- | --- | --- | --- |
| Caught | 29 | 29 | **39** |
| Missed | 12 | 12 | **2** |
| Unviable | 1 | 1 | 1 |
| Total | 42 | 42 | 42 |

**Parity holds exactly at the moment of the move**: 29 caught / 12 missed / 1
unviable before and after, and the twelve survivors were the same twelve mutants
in the same functions. The relocation neither gained nor lost a kill.

This change then went further than parity required and closed ten of the twelve
pre-existing survivors, because they were plain test gaps in a durable-write
admission policy rather than anything about the move. The tests added are in the
module's own `#[cfg(test)]` block:
`refreshing_an_unknown_request_reports_no_match`,
`every_admission_guard_clause_blocks_on_its_own`,
`an_exactly_budget_sized_request_is_not_exclusive`,
`an_exactly_budget_sized_overweight_request_needs_a_wholly_idle_lane`,
`snapshot_counts_close_work_separately_from_ordinary_work`, and
`documented_payload_policy_constants_hold_their_values`.

### Per-survivor disposition

Ten survivors closed by added tests:

| Survivor | Disposition |
| --- | --- |
| `refresh_queued -> true` | closed — the new test asserts `false` for an unknown request id, both with an empty queue and with an unrelated request queued |
| four `\|\| -> &&` in `admit_next` (queue-empty, active-slots-full, own-exclusive, external-exclusive clauses) | closed — each guard disjunct is now asserted on its own against an otherwise-admissible queue |
| `> -> <` in `admit_next` (protected residency with external weight) | closed — asserts that protected-residency pressure with external weight blocks even a wholly idle save lane, and that the same pressure with no external weight admits |
| `> -> >=` in `admit_next` (the `weight > budget` exclusivity test) | closed — a request weighing exactly the budget must admit without seizing the lane |
| two `== -> !=` in `snapshot` (queued and active close counts) | closed — a mixed queue of one close and two ordinary requests is asserted through queued, partially admitted, and fully admitted states |
| `* -> +` and `* -> /` in the fixed-overhead constant | closed — the constants are now asserted as literals. The pre-existing test restated the same expression symbolically, so it could not observe the expression changing. This is a reusable lesson: a policy constant asserted through its own defining expression is not covered |

Two survivors remain, and both are **equivalent mutants** rather than test gaps.
They are recorded here rather than excluded, because narrowing the mutation scope
to hide an equivalent mutant would also hide the next real one in the same
function:

| Survivor | Why it is equivalent |
| --- | --- |
| `policy.rs:189` `\|\| -> &&` on `self.max_active == 0` | Rust binds `&&` tighter than `\|\|`, so the mutant reads `(queued.is_empty() && max_active == 0) \|\| active.len() >= max_active \|\| …`. When `max_active` is 0 the very next disjunct, `active.len() >= max_active`, is unconditionally true, so the guard still returns `None`. The mutated program has the same behaviour for every reachable input |
| `policy.rs:268` `> -> >=` on `request.weight > self.budget` in `request_fits` | The two branches differ only for a request weighing exactly the budget. On that input the original takes the fitting branch and requires `active_weight + external + weight <= budget`, which needs `active_weight` and `external` to both be zero; the mutant takes the overweight branch and requires `active.is_empty() && external == 0`. Those agree whenever a grant carries a positive weight, which every admitted grant does — `admit_next` charges `request.weight`, and a zero-weight save is not constructible through the workflow (the permit's own `debug_assert!` states that admitted saves carry a byte charge) |

## Claim 2 — coverage gained from zero

The pure decisions extracted out of the GTK adapter into `policy.rs` were not
under mutation before this change, because they were inline in
`ui/editor_page/load_save.rs` and `ui/editor_page/save_runtime.rs`, neither of
which is in `examine_globs`. **The baseline is zero by construction**, so this is
not a parity claim and must not be read as one.

| Extracted decision | Mutants | Caught | Missed | Unviable |
| --- | --- | --- | --- | --- |
| `queued_save_is_current` (the admission seam predicate) | 12 | 12 | 0 | 0 |
| `save_may_preempt_pending_load` (the pending-load derivation) | 2 | 2 | 0 | 0 |
| `classify_saved_text` (formatting acceptance + mirror-back) | 1 | 0 | 0 | 1 |
| `save_capture_mode` (capture-mode naming) | 1 | 0 | 0 | 1 |
| **Total** | **16** | **14** | **0** | **2** |

**Zero survivors on the gain.** The two unviable mutants are the whole-function
`Default::default()` substitutions on `classify_saved_text` and
`save_capture_mode`, the two functions returning a workflow enum; cargo-mutants
could not build those substitutions and classifies them as unviable rather than
missed, so they are not coverage gaps. The two `bool`-returning functions have no
unviable mutants, because `bool` does have a `Default` — both of
`queued_save_is_current`'s whole-function `-> bool` substitutions (`true` and
`false`) built fine and were killed, which is why its count is 12 rather than the
10 an operator-only reading would give.

The ten kills on `queued_save_is_current` are the ones that matter most: that
predicate is where the archetype defect lived, and every boolean operator,
negation, and comparison in it is now individually killed by the module's own
unit tests — including
`explicit_destination_and_pending_load_cancellation_stay_distinct`, which pins
the `explicit_destination` clause against the path comparison it controls.

## What this change did not do

- No `.cargo/mutants.toml` entry was added, removed, or widened. In particular no
  `exclude_re` entry was added to hide a survivor.
- The remaining hand-listed UI `examine_globs` entries and `exclude_re` method
  lists were checked for save-path names: none names a save path, so none
  retires with this workflow.
- `services/editor_io.rs` and `services/durable_write.rs` keep their pure rules
  as private functions with direct unit tests. They are already inside the
  mutation scope through `services/**`, so extracting a `services/*/policy.rs`
  would buy no coverage, and moving them under `ui/` would invert dependency
  direction.

Mutant anchors in this document are deliberately **coarse** — file-level
generated/killed counts and function names, not per-line identifiers — so a later
simplification pass can reformat the module without invalidating the recorded
evidence.
