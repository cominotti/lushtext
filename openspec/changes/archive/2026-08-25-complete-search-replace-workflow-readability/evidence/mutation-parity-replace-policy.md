# Mutation parity — Replace All durable-half policy extraction (tasks 5.1, 5.6, 9.6, 10.3)

Slot 2b moved the Replace All journal half's pure decisions out of
`crates/lushtext-core/src/ui/search_panel/replace.rs` and into
`crates/lushtext-core/src/ui/search_panel/policy.rs`, and de-duplicated seven
buried rules inside `crates/lushtext-core/src/services/search_backup.rs`.

Captured on 2026-08-25 with `cargo-mutants 27.0.0`, `cargo nextest`, on a
worktree containing the complete slot-2b change.

## The baseline asymmetry, stated plainly (task 5.1)

`ui/search_panel/replace.rs` was **not** in the mutation scope. `.cargo/mutants.toml`
examines `model/**`, `services/**`, and `ui/**/policy.rs` — a coordination module
named `replace.rs` matches none of those. So the pure logic that moved
(`retained_byte_weight`, `preview_reservation_weight`,
`completed_preview_reservation_weight`, the undo-capacity admission arithmetic
inline in `try_reserve_undo_replacement`, and the generation-match predicates
inline in the guarded install and clear) generated **zero mutants before this
change**.

That makes this an **out-of-scope-to-in-scope move, not a relocation between two
scoped locations.** The mutation-testing spec's equal-counts phrasing governs the
latter. For the former, parity means **gaining** mutants that are all killed,
which is strictly stronger: a relocation can satisfy equal counts while carrying
a survivor forward, whereas this move had to close every mutant it created.

`services/search_backup.rs` is the opposite case and is stated here so it is not
mistaken for a coverage claim: it was **already** inside `services/**`, so
de-duplicating its buried rules into named private functions gained **no**
mutation reach. The win there is testability without a tempdir plus one
implementation per rule — task 6.2's recorded decision says exactly that, and
this document does not claim otherwise. What the de-duplication *did* change is
that each rule now has its own mutable, individually addressable function, so its
mutants are attributable to the rule rather than buried inside a large loader.

## Scope re-verification (required before the full run)

```
MUTANTS_RE='crates/lushtext-core/src/ui/search_panel/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list
```

Listed exactly 40 mutants, all in `ui/search_panel/policy.rs`. The two-part
scoping recorded by slot 1 is still required: cargo-mutants 27's `--re` does not
filter the `delete field` mutant kind, and the services glob exclusion is what
reduces the listed scope to the target module. Slot 2b additionally confirmed
that `MUTANTS_EXCLUDE_RE` is **not** a substitute — it does not filter
delete-field mutants either, so attribution stays by file.

## Result: `ui/search_panel/policy.rs`

```
MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
MUTANTS_RE='crates/lushtext-core/src/ui/search_panel/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh full
```

```
40 mutants tested in 5m: 36 caught, 4 unviable
```

| Population | Generated | Caught | Missed | Unviable |
| --- | --- | --- | --- | --- |
| Moved logic, **before** (in unscoped `replace.rs`) | 0 | 0 | 0 | 0 |
| Pre-existing slot-1 population (whole `policy.rs` after slot 1) | 29 | 26 | 0 | 3 |
| Whole `policy.rs` after slot 2b | 40 | 36 | 0 | 4 |
| **Slot 2b's addition** | **+11** | **+10** | **0** | **+1** |

Every one of the 11 new mutants is caught or unviable, and the pre-existing
slot-1 population is unchanged: still 0 survivors, and its 3 unviable mutants are
the same three. Reported separately above, as task 5.6 requires.

### The 11 added mutants and what kills them

| Mutant | Disposition |
| --- | --- |
| `retained_byte_weight -> u64 with 0` / `with 1` | caught by `retained_byte_weight_saturates_instead_of_wrapping` |
| `preview_reservation_weight -> u64 with 0` / `with 1` | caught by `preview_reservation_charges_source_bytes_plus_one_row_each` |
| `completed_preview_reservation_weight -> u64 with 0` / `with 1` | caught by `completed_reservation_measures_real_retention_not_the_budget` |
| `replace && with \|\| in plan_undo_reservation` | caught by `each_guarded_owner_alone_still_makes_it_a_replacement` |
| `journal_generation_is_current -> bool with true` / `with false` | caught by `journal_generation_matches_only_its_own_reservation` |
| `replace == with != in journal_generation_is_current` | caught by the same test |
| `plan_undo_reservation -> UndoReservationPlan with Default::default()` | **unviable**: `UndoReservationPlan` deliberately implements no `Default`, because "no guarded owner" and "a zero-weight guarded owner" are different admission decisions and a default would silently pick one |

### Per-survivor disposition

None. There are no survivors in either population.

### The 4 unviable mutants

Three are slot 1's, unchanged and for unchanged reasons: the two
`WorkspaceSearchFlight::submit` / `finish` `Default::default()` substitutions
(the returned types implement no `Default`) and the
`ReplacePreviewTicket::query_spec` borrow substitution. The fourth is slot 2b's
`plan_undo_reservation` substitution above. No derive changed.

## Result: `make mutants-diff` (task 10.3)

The wrapper's `ensure_diff_file` generates its diff with `git diff origin/main...`,
which is `git diff origin/main...HEAD` — the merge base against **`HEAD`**, not
against the working tree. Slot 2b's change is uncommitted, so the documented
worktree-diff workaround was used, exactly as slot 1 recorded it:

```
git diff "$(git merge-base origin/main HEAD)" > worktree.diff
MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
  ./scripts/run-mutants.sh diff worktree.diff
```

Two runs were recorded, because the first found three survivors that were closed
before the second.

```
# run 1
66 mutants tested in 5m: 3 missed, 60 caught, 3 unviable

# run 2, after the three survivors were closed
63 mutants tested in 5m: 60 caught, 3 unviable
```

| Run | Generated | Caught | Missed | Unviable | Exit |
| --- | --- | --- | --- | --- | --- |
| changed-code lane, first pass | 66 | 60 | 3 | 3 | non-zero (survivors) |
| changed-code lane, after closing them | 63 | 60 | 0 | 3 | zero |

Caught mutants by file in the clean run: `services/search_backup.rs` 47,
`ui/search_panel/policy.rs` 10, `services/content_search/replace.rs` 3.

### The three survivors, and how each was closed

All three were in code slot 2b added, and all three were closed by strengthening
a test or removing genuinely equivalent code — **not** by a scope change. The
generated count fell from 66 to 63 because one fix deleted a redundant guard,
which is a real simplification rather than an exclusion.

1. `services/search_backup.rs:270:12: delete ! in shrink_journal_to`

   Deleting the `!` makes the superset case fall through to the full
   delete-then-rebuild `save`. The original test asserted only that a *new
   manifest* was committed, which `save` also does, so it could not see the
   difference. **Closed by asserting the property that actually matters**: the
   retained entry file's inode is unchanged after a shrink. `save` destroys and
   recreates that file, which is precisely the window in which an unrestored file
   has no durable rollback copy. Test:
   `shrink_keeps_the_journal_active_and_drops_only_restored_entries`.

2. and 3. `services/content_search/replace.rs:1518:24: replace > with ==` and
   `with >=` in `rollback_file_disposition`

   Both mutated an explicit `facts.byte_size > MAX_REPLACE_FILE_BYTES`
   pre-check. That check was **redundant**: the following
   `fs_read::bounded_bytes(path, MAX_REPLACE_FILE_BYTES, ..)` already refuses a
   file larger than the limit with `LimitExceeded`, and this function's `else`
   arm classifies every read failure as `ChangedSinceReplacement` anyway. So no
   input could distinguish the three operators — the mutants were genuinely
   equivalent because the code was dead. **Closed by deleting the redundant
   pre-check** rather than by adding a multi-megabyte fixture to test a branch
   that cannot be reached. Its comment records why the size guard is the reader's
   job to find in `bounded_bytes`.

## The `services/search_backup.rs` population, for the record

The de-duplication produced 47 caught mutants and 1 unviable in that file
(`detect_orphan_journal_entries -> vec![Default::default()]`, unviable because
`RecoveryDiagnostic` implements no `Default`). Those mutants are **not** a
coverage gain — the file was already scoped — but they are newly *attributable*:
before this change the same decisions were inline in three large loaders, so a
mutant landed on the loader rather than on the rule. The 12 new tempdir-free unit
tests (`activation_requires_no_diagnostics_and_manifest_agreement`,
`incremental_activation_has_no_manifest_to_agree_with`,
`payload_budget_rejects_only_above_the_cap`,
`payload_budget_saturates_instead_of_wrapping_into_acceptance`,
`entry_count_cap_admits_the_exact_limit_and_rejects_one_over`,
`retained_weight_cap_admits_the_exact_limit_and_rejects_one_over`,
`payload_filter_accepts_entries_and_rejects_journal_bookkeeping`,
`dedup_rejects_a_duplicate_entry_file_and_a_duplicate_target_path`,
`dedup_accounts_for_an_entry_file_whose_target_path_was_rejected`,
`cleanup_is_refused_when_any_diagnostic_disallows_replacement`, plus the two
recovery-state tests) are what keep them caught.
