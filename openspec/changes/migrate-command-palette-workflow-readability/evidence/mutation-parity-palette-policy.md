# Mutation coverage — command-palette pure policy (tasks 9.2–9.4)

Post-migration mutation evidence for
`crates/lushtext-core/src/ui/command_palette/policy.rs`, the pure decision logic
this change extracted out of `ui/command_palette/mod.rs` and `imp.rs`.

Captured 2026-08-25, `cargo-mutants 27.0.0`, `cargo nextest` runner.

## This is a coverage gain, not an equal-counts parity claim

`mutation-testing`'s parity requirement is phrased as equal before/after counts.
That phrasing governs a relocation between two locations that are **both already
in the mutation scope**. This move is not that. `.cargo/mutants.toml`'s
`examine_globs` covers `model/**`, `services/**`, `ui/**/policy.rs`, and two
hand-listed pre-convention UI files; **no file under
`crates/lushtext-core/src/ui/command_palette/` was in that set**, so the pre-move
baseline is **zero generated mutants by construction**
(see `mutation-baseline-palette-policy.md`).

The obligation here is therefore *stronger* than parity: the relocated decisions
enter the scope for the first time and every mutant they generate must be caught
or unviable with a stated reason. Reporting "0 → 0, parity holds" would be
technically true and substantively false, so it is not reported that way.

## Scope re-verification, then the run

Both parts of the scoping are required. `--re` alone does not filter the
`delete field <field> from struct <T> expression in <fn>` mutant kind, so the
exclusion glob is what reduces the listed population to the target module. The
`list` run is not optional bookkeeping: `ui/command_palette/policy.rs` entered
`examine_globs` the moment it was created, so the population had to be
re-confirmed rather than assumed.

```
MUTANTS_RE='crates/lushtext-core/src/ui/command_palette/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list
# 56 mutants listed; 0 of them outside command_palette/policy.rs

MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
MUTANTS_RE='crates/lushtext-core/src/ui/command_palette/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh full
```

Survivors were attributed **by file**, not by trusting `--re`, for the same
reason.

## Result

| Outcome | Baseline (pre-move) | Run 1 | Run 2 (final) |
| --- | --- | --- | --- |
| Generated (tested) | 0 (unscoped) | 56 | 56 |
| Caught | 0 | 46 | **50** |
| Missed (survived) | 0 | 4 | **0** |
| Unviable | 0 | 6 | 6 |
| Timeout | 0 | 0 | 0 |

```
56 mutants tested in 5m: 50 caught, 6 unviable
```

## Run 1 survivors and how each was closed

All four were closed by **added tests**, not by scope changes, exclusions, or
`exclude_re` entries.

| Survivor | Why it survived | Test added |
| --- | --- | --- |
| `policy.rs:53:51` and `:53:58: replace * with +` | `MAX_PENDING_INDEX_UPDATE_BYTES` is spelled `4 * 1024 * 1024`; no test pinned the product, so `4 + 1024 + 1024` still admitted every queued mutation in the fixtures | `declared_queue_ceilings_are_the_documented_values` asserts the byte ceiling is exactly `4_194_304` plus the four sibling constants |
| `policy.rs:89:9: replace FileIndexUpdate::apply with ()` | the unit tests exercised admission and arbitration but never applied a mutation to a real `FileIndex`, so a no-op `apply` was invisible | `each_update_kind_actually_mutates_the_index` applies Create, Delete, and Rename to a two-file index and asserts the count and the moved path |
| `policy.rs:355:14: replace > with >= in next_activatable` | direction is `delta > 0` and every existing case passed ±1, so `>=` differed only at `delta == 0` | `next_activatable_treats_only_a_positive_delta_as_forward` pins that a zero delta scans backward |

The `apply` survivor is the most substantive of the four: it says the pre-move
code had **no** test proving an index mutation reached the index through this
path at the unit level. That gap existed before the migration and was invisible
because the file was unscoped; it is now closed.

## Unviable mutants (6, expected, unchanged between runs)

```
policy.rs:160:5: replace admit_index_update -> IndexUpdateAdmission with Default::default()
policy.rs:208:5: replace select_batch_kind -> FileIndexUpdateBatchKind with Default::default()
policy.rs:285:9: replace FileIndexMutationTicket::arbitrate -> FileIndexMutationArbitration with Default::default()
policy.rs:318:5: replace classify_index_retirement -> Option<FileIndexRetirementKind> with Some(Default::default())
policy.rs:53:51: replace * with /
policy.rs:53:58: replace * with /
```

The first four are unviable because the returned types deliberately do **not**
implement `Default`: there is no sensible default admission decision, batch kind,
arbitration outcome, or retirement lane, and not deriving `Default` is what makes
that unrepresentable. If a later change adds a `Default` derive to any of them,
these become viable and must be caught — treat that as a design signal, not a
mutation-count regression.

The two `* with /` mutants on the byte ceiling are unviable because
`4 / 1024 / 1024 == 0` makes `MAX_PENDING_INDEX_UPDATE_BYTES` zero, which the
`u64` literal arithmetic rejects at compile time in this position.

## `make mutants-diff` (task 9.4), both runs recorded

**Run A — the wrapper as invoked, which found nothing.**

```
$ make mutants-diff
Creating mutation diff against origin/main...
No diff hunks found; skipping changed-code mutation run.
```

This is the failure mode the slot-1 evidence file warned about, and the cause is
exact: `scripts/run-mutants.sh` builds its diff with
`git diff "${MUTANTS_BASE}..."`, whose three-dot form compares the merge base to
**HEAD**. With the change uncommitted, HEAD equals the merge base, so the diff is
empty and the lane skips rather than fails. **A skip here is not a pass**, which
is why run B exists.

**Run B — the explicit merge-base workaround, supplying the working-tree diff.**

New files are untracked, so `git diff` alone would omit them; `git add -N` records
intent-to-add without staging content, and the paths are reset immediately
afterwards so the worktree is left exactly as it was.

```
$ git add -N <the seven new ui/command_palette/*.rs files>
$ git diff origin/main -- crates scripts > worktree.diff   # two-dot: includes the working tree
$ git reset -q -- <the same seven files>
$ grep -c '^@@ ' worktree.diff
62

$ MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
  MUTANTS_DIFF_FILE=worktree.diff ./scripts/run-mutants.sh diff
56 mutants tested: 50 caught, 6 unviable
```

**The diff lane selected exactly the same 56-mutant population as the focused
run**, which is the useful cross-check: `services/palette/index.rs` also changed
in this diff (the hand-rolled coordinator became a type alias) and contributed
**zero** new mutants, because the change is a net deletion of code that had
mutants into aliases that have none. No mutant outside
`ui/command_palette/policy.rs` entered the scope.

## `exclude_re` retirements

**None**, as expected. The two palette-adjacent entries name
`services/palette/index.rs`'s `truncate_to_index_limit` and the `commands.rs`
property-test bridge; this change moves neither, and neither became retirable.
No hand-listed `examine_globs` path was added: `ui/command_palette/policy.rs` is
in scope purely by the `ui/**/policy.rs` convention.
