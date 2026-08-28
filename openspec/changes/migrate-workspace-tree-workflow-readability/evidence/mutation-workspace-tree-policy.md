# Mutation evidence — `WFR-WORKSPACE-TREE` policy (tasks 3.5, 3.6, 6.9)

This change's own `mutation-testing` amendment requires two things of this file:
the **unfilterable floor** must be stated, and **relocation parity** must be
reported **separately** from **extraction gain from zero**. Both are done below,
and the floor was measured rather than recalled.

## 1. The unfilterable floor, measured from the tool

`cargo-mutants 27.0.0` (matching the `CARGO_MUTANTS_VERSION=27.0.0` CI pin).

`--re` is documented as *"Regex for mutations to examine, matched against the names
shown by `--list`"*, and struct-field-deletion mutants **do** appear in `--list` with
names of the form `delete field <f> from struct <S> expression in <fn>` — so the
filter looks as though it should apply to them. It does not. Measured with a pattern
that cannot match any mutant name:

```
MUTANTS_RE='ZZZ_NO_SUCH_MUTANT_ZZZ' make mutants-list
→ 34 mutants still generated, all 34 of them `delete field` mutants
```

**The floor is 34 mutants** across the configured `examine_globs` scope:

| File | Field-deletion mutants |
| --- | --- |
| `services/file_tree.rs` | **12** |
| `services/draft_service.rs` | 11 |
| `services/markdown_render.rs` | 3 |
| `ui/window/session_restore/policy.rs` | 2 |
| `ui/window/notes/policy.rs` | 2 |
| `services/single_flight.rs` | 2 |
| `services/local_history_service.rs` | 2 |
| **total** | **34** |

Control run confirming the arithmetic composes:

```
MUTANTS_RE='WorkspaceScanFlight' make mutants-list
→ 50 = 16 in model/workspace_scan.rs (the intended target) + the same 34-mutant floor
```

### An important refinement: `--in-diff` *does* bound the run

Worth recording because it changes how a focused run should be requested. The floor
is a property of the **name filter**, not of scoping generally:

| Scoping mechanism | Bounds the run? | Evidence |
| --- | --- | --- |
| `--re` (name regex) | **No** — the 34-mutant field-deletion floor runs regardless | the two runs above |
| `--in-diff <diff-file>` | **Yes** — genuinely bounded | the run in §3 generated exactly **4** mutants, with **no** floor |

So a change that needs a genuinely narrow proof should scope by diff, and a change
that scopes by name owes the floor statement. This change does both, and reports each
accordingly.

## 2. Relocation parity — proved mutant-by-mutant, not by counting

Both relocation sources lived under `crates/lushtext-core/src/model/**/*.rs` and were
therefore **already inside `examine_globs`**. Unlike slot 4's relocations there **is**
a before-count, so parity is a real claim that can fail.

### Method

Parity requires **generated and killed** to be unchanged, so a count match alone is
not sufficient. The baseline was measured in a **separate short-path worktree at
`origin/main`** (`/tmp/w5b` — short deliberately, per slot 4's lost run to
`libmutter-ERROR: Failed to create socket` under a deep scratch path), scoped one
file at a time through the wrapper's `smoke` mode, which passes `--no-config --file`
and therefore carries **no field-deletion floor**:

```
# baseline, in a worktree at origin/main
MUTANTS_SMOKE_FILE=crates/lushtext-core/src/model/workspace_persistence.rs ./scripts/run-mutants.sh smoke
MUTANTS_SMOKE_FILE=crates/lushtext-core/src/model/workspace_scan.rs        ./scripts/run-mutants.sh smoke

# after, in the working tree
git diff origin/main -- crates/lushtext-core/src/ui/sidebar/policy.rs > policy2.diff
./scripts/run-mutants.sh diff policy2.diff
```

File-level anchors: `crates/lushtext-core/src/model/workspace_persistence.rs` and
`crates/lushtext-core/src/model/workspace_scan.rs` before;
`crates/lushtext-core/src/ui/sidebar/policy.rs` after.

### Result — every figure reconciles

| Population | Generated | Caught | Unviable | **Missed** |
| --- | --- | --- | --- | --- |
| baseline `model/workspace_persistence.rs` | 34 | 23 | 4 | **7** |
| baseline `model/workspace_scan.rs` | 16 | 12 | 4 | **0** |
| baseline total | **50** | 35 | 8 | **7** |
| after, `ui/sidebar/policy.rs` diff region | **54** | 38 | 9 | **7** |
| of which this change's own extraction (§3) | 4 | 3 | 1 | 0 |
| **after, relocated populations only** | **50** | **35** | **8** | **7** |

**Parity holds on every axis**: 50 generated → 50, 35 caught → 35, 8 unviable → 8,
7 missed → 7. The old locations now generate **zero** mutants, because both modules
are deleted from `model/`.

### The stronger claim: the missed set is identical mutant-by-mutant

Counting could coincide by accident, so the seven survivors were matched
individually. The relocation shifted the persistence code by a **constant +198
lines**, and all seven map at exactly that offset with identical columns and
identical mutant descriptions:

| Baseline site | Offset | After site | Mutant |
| --- | --- | --- | --- |
| `:22:9` | +198 | `:220:9` | `replace WorkspacePersistenceGeneration::value -> u64 with 1` |
| `:107:37` | +198 | `:305:37` | `replace \|\| with && in WorkspacePersistenceState::start` |
| `:174:9` | +198 | `:372:9` | `replace WorkspacePersistenceState::has_pending_work -> bool with true` |
| `:174:24` | +198 | `:372:24` | `replace != with == in has_pending_work` |
| `:174:40` | +198 | `:372:40` | `replace \|\| with && in has_pending_work` |
| `:174:68` | +198 | `:372:68` | `replace \|\| with && in has_pending_work` |
| `:192:9` | +198 | `:390:9` | `replace WorkspacePersistenceState::durable_generation -> ... with Default::default()` |

A single constant offset across every site is itself evidence that the move was a
**literal text relocation** rather than a rewrite — the property task 5.5 asks for,
obtained here as a by-product of the parity proof.

**These seven are baseline, not regressions**, and are not attributed to this change.
They are triaged in §4, because the relocation put them in a file this change owns.

### The working-tree constraint that produced a false green

Recorded because it exits **0** and looks like a pass. `make mutants-diff` builds its
diff with `git diff "${MUTANTS_BASE}..."` — a three-dot **commit range**, which does
not include the working tree. With this change's edits uncommitted the first
invocation reported:

```
Creating mutation diff against origin/main...
No diff hunks found; skipping changed-code mutation run.
```

Zero mutants tested, exit code 0. This is the same class of gate blindness
`.agents/rules/build.md` records for untracked files, and **`git add -N` does not fix
it**, because the problem is the commit range rather than the index. The workaround,
which a later slot should reuse, is to generate the diff from the working tree
explicitly and pass it as an argument, as shown above. `MUTANTS_IN_PLACE=1` refuses a
dirty worktree outside CI, so the default copy-based mode was used.

## 3. Extraction gain from zero — measured, reported separately

The extraction out of the GTK adapter has **no** before-count, because GTK adapters
are outside the mutation scope by design. Its result is therefore a **gain from
zero that cannot fail**, and it is reported here as its own figure so a parity loss
in §2 could never hide behind it.

### What was extracted

`ui/sidebar/policy.rs` gained `confirmed_delete_verdict` + `ConfirmedDeleteVerdict`,
the decision that had been inline in the delete worker's GTK closure in
`ui/sidebar/workspace_section/actions.rs`. This was the fix for a confirmed **HIGH**
data-safety defect (see `evidence/data-safety.md`): the confirmed delete removed by
**path** with no identity recheck, so a same-kind substitution during the user-paced
confirmation window destroyed a different file — recursively, for the directory
branch.

Extracting it rather than inlining the comparison is what brings it inside the
`ui/**/policy.rs` mutation scope, which is the point: *this* workflow's decisions are
the ones that most need the coverage, because they rename and delete the user's own
documents.

### Exact invocation and file-level anchors

```
git diff origin/main -- crates/lushtext-core/src/ui/sidebar/policy.rs > policy-worktree.diff
MUTANTS_JOBS=2 MUTANTS_TEST_THREADS=4 ./scripts/run-mutants.sh diff policy-worktree.diff
```

Anchor: `crates/lushtext-core/src/ui/sidebar/policy.rs`, hunks at `@@ -132,6 +132,57 @@`
and the test-module hunk.

### Result

```
Found 4 mutants to test
ok       Unmutated baseline in 50s build + 9s test
4 mutants tested in 2m: 3 caught, 1 unviable
```

**3 caught, 1 unviable, 0 missed — zero survivors on the first run.**

The four mutants, enumerated from `--list` so the unviable one is accounted for
rather than merely subtracted:

| Mutant | Outcome |
| --- | --- |
| `policy.rs:180:5: replace confirmed_delete_verdict -> ConfirmedDeleteVerdict with Default::default()` | **unviable** — `ConfirmedDeleteVerdict` derives no `Default`, so the mutant does not compile. Deliberate: a verdict type with a `Default` would let a caller obtain a "delete may proceed" answer without asking, which is the defect class this whole finding is about. |
| `policy.rs:181:27: replace match guard current_inode == Some(expected) with true` | caught |
| `policy.rs:181:27: replace match guard current_inode == Some(expected) with false` | caught |
| `policy.rs:181:41: replace == with != in confirmed_delete_verdict` | caught |

The three caught mutants are exactly the three ways to break the identity check —
always proceed, never proceed, and invert the comparison — and each is killed by a
named unit test rather than incidentally:

- `a_confirmed_delete_proceeds_only_against_the_identity_the_user_was_shown` kills `false`,
- `a_same_name_different_object_is_refused` kills `true` and `!=`,
- `a_vanished_target_is_refused_rather_than_treated_as_already_done` and
  `an_unreadable_original_identity_is_refused` pin the two `None` arms, which is where
  a "nothing to do" reading would have reintroduced the defect.

### Full-module context

The extended `ui/sidebar/policy.rs` now generates **23** mutants in total (up from 19
before this change), of which the 4 above are new. The module imports no `gtk4`,
`glib`, `gio`, `libadwaita`, or `sourceview5`, so it remains reachable through the
literal `ui/**/policy.rs` convention; `make check-workflow-boundaries` passes.

## 4. Triage of the seven inherited persistence survivors

The relocation carried seven pre-existing survivors into `ui/sidebar/policy.rs`,
which this change owns. The `mutation-testing` amendment this change lands requires
the owning change to triage rather than pass them on again, in the documented order:
decide whether each represents real missed behaviour, then add or tighten
deterministic tests, then consider a small refactor, and **only then** a narrow
documented exclusion.

**Outcome: 7 survivors → 0.** Six were killed by tightening assertions; the seventh is
provably equivalent and carries a narrow documented exclusion. No production behaviour
changed and no test was weakened.

Final verification run, with the source frozen for its whole duration:

```
git diff origin/main -- crates/lushtext-core/src/ui/sidebar/policy.rs > policy4.diff
MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 ./scripts/run-mutants.sh diff policy4.diff
→ 53 mutants tested in 6m: 44 caught, 9 unviable
→ missed.txt: 0 lines
```

53 rather than 54 because of the one narrow exclusion. The full arc:

| Stage | Generated | Caught | Unviable | **Missed** |
| --- | --- | --- | --- | --- |
| baseline, both modules at their old homes | 50 | 35 | 8 | **7** |
| after relocation, before triage | 54 | 38 | 9 | **7** (parity: the same 7) |
| after triage and one exclusion | 53 | 44 | 9 | **0** |
| after the structural migration | 55 | 46 | 9 | **0** |
| final, after seam retirement and the pass-2 fixes | 58 | 48 | 10 | **0** |
| **final, after the fix cycle** | **59** | **49** | **10** | **0** |

Two rows of that arc each need a reason, and an earlier revision of this file gave
both under the same opening words ("The final run is ..."), which read as one sentence
contradicting itself. They describe **different rows**:

- **55 rather than 53** — the *structural migration* row. It extracted two more
  decisions into `policy.rs`: the **persist debounce** literal, and
  **`workspace_scope_kind_name`**.
  The latter arrived with **2 survivors** — a `&'static str` returner with no direct
  unit test, because its only assertions lived in a *widget* test, which is outside the
  mutation lane's test surface. That is a reusable trap: extracting a decision **into**
  the mutation scope does not bring its widget-level coverage with it. Killed at step two
  with `scope_kind_names_are_the_documented_protocol_tokens`, which pins both literals
  and asserts that two different workspaces share one kind token.
- **58** — the *pass-2 fixes* row. It moved two more decisions into
  `policy.rs`: `superseded_load_action` (which of two unsafe options a superseded load
  may take) and `merge_superseded_workspace_load`. Both arrived **with tests**, having
  learned from `workspace_scope_kind_name` arriving with two survivors, so this run
  needed no triage round.
- **59, the final row** — the *fix cycle* row. `confirmed_delete_verdict` gained a third
  outcome (`ReconcileAlreadyGone`), so its match generates one more mutant. It arrived
  with two tests — one pinning the new outcome, one pinning that it never collapses with
  a same-name substitution — and was **caught on the first run**, no triage round.

**Fix-cycle invocation and result, recorded so it is reproducible:**

```
git diff HEAD -- crates/lushtext-core/src/ui/sidebar/policy.rs > policy-fix.diff
MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
  ./scripts/run-mutants.sh diff policy-fix.diff
→ Found 59 mutants to test
→ ok  Unmutated baseline in 48s build + 9s test
→ 59 mutants tested in 6m: 49 caught, 10 unviable
→ missed: 0
```

The `--in-diff` file is generated against `HEAD` rather than `origin/main` because this
change is uncommitted; `MUTANTS_BASE` defaults to `origin/main`, which for an uncommitted
worktree produces an empty diff and a silently skipped run.

### Six killed at step two — tighten the assertions

Every one survived for the same reason: the existing assertion was satisfied by the
mutant's own value. That is the classic weak-assertion shape, so the fix was step
two, not a refactor and not an exclusion.

| Survivor | Why it survived | Test added |
| --- | --- | --- |
| `value -> u64 with 1` | `value()` was only ever asserted against another `value()` or against a freshly defaulted generation — both satisfied by a constant | `a_generation_reports_its_own_ordinal_rather_than_a_constant` pins the 1st/2nd/3rd ordinals and that a default generation is the **zeroth** |
| `\|\| -> && in start` | no test had a state that was **both** busy and newly dirty | `a_busy_worker_refuses_a_second_start_even_with_newer_work_pending` — with the conjunction, a second concurrent write of the same file would start |
| `has_pending_work -> true` | no test asserted the **negative** | `a_settled_state_reports_no_pending_work` |
| `!= -> == in has_pending_work` | same missing negative | same test — a defaulted state has `requested == durable` |
| `\|\| -> && at col 40` | no test had dirty work with **neither** a worker nor a failure | `dirty_work_alone_is_pending_work_without_a_failure_or_a_worker` — with the conjunction the close flush would let the window close over an unwritten workspace list |
| `durable_generation -> Default::default()` | the only assertion compared it **against** a defaulted generation, which the mutant returns verbatim | `the_durable_generation_advances_past_the_default_on_success` |

### One excluded at step four — provably equivalent

`policy.rs:...:68: replace || with && in WorkspacePersistenceState::has_pending_work`.

`has_pending_work` is `a || b || c` where `a = requested != durable`,
`b = in_flight.is_some()`, `c = failed.is_some()`. Replacing the **second** `||`
yields `a || b && c`, which Rust parses as `a || (b && c)` — **not** `(a || b) && c`.
The two forms differ only when `a` is false **and exactly one** of `b`/`c` is true.

Both such states are unreachable, established from the state machine rather than
assumed:

- **`in_flight` and `failed` are mutually exclusive.** `apply_failure` sets
  `in_flight = None` *before* recording `failed`; `start` and `apply_success` clear
  `failed`.
- **Each of them implies `requested != durable`.** `start` refuses unless
  `requested != durable`, and `durable` advances only in `apply_success`, which
  clears both `in_flight` and `failed`.

So in every reachable state where `b` or `c` holds, `a` already holds, and the
trailing disjuncts are **defensive redundancy rather than live logic**. No test can
distinguish the mutant, which makes it equivalent rather than a coverage gap — the
one case the documented order permits an exclusion for.

Two things keep the exclusion honest:

1. **The invariant is pinned by a test**, not by this prose:
   `an_in_flight_write_and_a_recorded_failure_are_mutually_exclusive_and_both_imply_dirt`
   walks start → failure → retry → success and asserts the exclusivity and the
   dirtiness implication at each step. If a future change makes either
   discriminating state reachable, that test fails and **the exclusion must be
   removed rather than widened**.
2. **The exclusion is scoped to one operator in one function.** Verified from the
   tool afterwards: the function's return value (2 mutants), the first `||`
   (`:...:40`), and the `!=` (`:...:24`) all still generate and are all killed.

### An operational hazard this triage surfaced

**Editing the source while a copy-based mutation run is in flight invalidates its
results, silently.** A `cargo fmt --all` was run mid-run; cargo-mutants was working
from its own copy and reported `:372:68` as MISSED against line numbers that had
shifted, so a later hand-check of "the mutant at that position" tested a *different*
expression and appeared to kill it. The false conclusion was caught only by applying
the mutation literally — and note the near-miss: the hand-check added parentheses,
producing `(a || b) && c`, which **is** killed, rather than the `a || (b && c)` the
tool actually generates. Two separate traps in one investigation:

- do not edit any file in the mutation scope while a run is in flight; and
- when hand-verifying an operator mutation, reproduce the operator **exactly** and
  let Rust's precedence apply, because `&&` binds tighter than `||`.

## 5. `services/file_tree.rs` survivor triage (task 10.7)

**Complete.** The triage itself lives in `evidence/mutation-file-tree-survivors.md`,
which carries the applied-deletion table and the equivalence proof; its outcome is
**12 generated, 3 killed by this change, 1 already killed, 8 equivalent with a proven
argument and a named durable fix**. This section keeps only the population framing, so
the floor and the population are not conflated. (An earlier revision of this file said
"_Pending._" while its sibling reported the finished triage; the contradiction is
resolved here in favour of the sibling, which has the evidence.)

**A correction to the inherited handoff.** Slots 4 and 5a handed on "**11**
pre-existing surviving field-deletion mutants in `services/file_tree.rs`". The tool
generates **12** field-deletion mutants in that file. Those are different quantities —
**12 generated, of which 11 were recorded as surviving**, so exactly one is already
killed. Triage runs against the generated list of 12 and reports which one is killed,
rather than assuming the inherited 11 is the whole population.

All twelve are `DirectoryScan` struct-literal fields in two scan functions
(`scan_directory_bounded_with_cancel_and_bytes`, `scan_directory_without_byte_limit`):
`examined_entries` ×4, `error` ×3, `peak_retained_entries` ×2, `peak_retained_bytes`
×2, `cancelled` ×1. They are **scan-metrics reporting fields**, which is why they
survive: the functions' behavioral contract is asserted through the returned entries,
not through the metrics. That shape is the triage input, and it points at
"add or tighten deterministic tests" — step two of `.agents/rules/build.md`'s order —
rather than at an exclusion.

These are **baseline, not regressions**, and this change does not attribute them to
itself. The file already carries one narrowly scoped `exclude_re` entry for the
`classify_entry` symlink match guard: that is the shape an exclusion must take, and it
must not be widened.
