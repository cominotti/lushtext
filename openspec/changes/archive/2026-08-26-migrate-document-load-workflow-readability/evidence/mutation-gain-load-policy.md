# Mutation coverage for `ui/editor_page/load/policy.rs`

**This is a gain from zero, not a relocation parity.** `model/file_load.rs`
stays where it is (see task 2 and the matrix's "Modules confirmed as domain and
staying in `model/`"), so nothing moved into or out of the mutation scope. What
changed is that the load workflow's pure decisions — previously inline in the
GTK adapter, where the scope deliberately does not reach — are now a policy
module the scope does reach. Before this change **zero** of them were mutated.

Mixing a gain with a parity claim makes both unreadable, so the numbers below
are only ever "generated / killed", never "before / after".

## Scope re-verification

The scope reaches pure policy by **convention, not by directory**:
`.cargo/mutants.toml`'s `examine_globs` contains the literal
`crates/lushtext-core/src/ui/**/policy.rs`, and the nested per-workflow path
matches it. Verified rather than assumed, with the exact commands:

```
$ ./scripts/run-mutants.sh list | grep -c 'ui/editor_page/load/policy.rs'
44
$ ./scripts/run-mutants.sh list | grep -c 'ui/editor_page/save/policy.rs'
57
$ make check-workflow-boundaries
workflow boundary policy passed: 4 workflow policy module(s) are pure and
mutation-scoped, ...
```

The save comparison is included because slot 3a first proved the nested glob is
reachable; this run confirms it for a **second** per-workflow subdirectory, which
is what the convention needed before a third workflow adopts the shape.

## Run command

```
MUTANTS_RE='ui/editor_page/load/policy\.rs' \
MUTANTS_JOBS=4 MUTANTS_TEST_THREADS=4 \
  ./scripts/run-mutants.sh full
```

Two notes a later session should not re-derive:

- `MUTANTS_RE` narrows the *build/test* selection but the run still reports the
  configured scope's other survivors. Attribute outcomes **by file**, which is
  why every count below is a `grep -c` against `mutants.out/<outcome>.txt`
  rather than a headline total.
- `MUTANTS_EXCLUDE_RE` does **not** filter `delete field ... from struct`
  mutants, so those cannot be scoped out of a focused run either. They are
  attributed by file below.

## Results

| Outcome | Count | Notes |
| --- | --- | --- |
| Generated | 44 | all in `ui/editor_page/load/policy.rs` |
| **Caught** | **41** | |
| **Missed** | **0** | |
| Unviable | 3 | `replace <fn> -> <Enum> with Default::default()` for `install_slice_action`, `abort_action`, and `load_failure_state`; those three enums have no `Default`, so the mutant does not compile. Unviable is not a survivor |

Counted with:

```
$ for f in caught missed unviable timeout; do
    printf "%s: %s\n" "$f" \
      "$(grep -c 'ui/editor_page/load/policy\.rs' mutants.out/$f.txt)"
  done
caught: 41
missed: 0
unviable: 3
timeout: 0
```

## Per-survivor disposition

One survivor appeared on the first run and was closed by an added test, not by a
scope change.

| Survivor | Verdict | Closed by |
| --- | --- | --- |
| `policy.rs:42:46: replace * with +` (`CLEAR_SLICE_CHARS = 64 * 1024`) | **real missed behavior.** `64 + 1024 = 1088` is still a *valid* clear budget, so every existing assertion — all of which were phrased relative to the constant (`clear_slice_char_count(CLEAR_SLICE_CHARS + 1) == CLEAR_SLICE_CHARS`) — passed against it. Nothing pinned the value | `the_clear_slice_budget_matches_the_shared_replacement_budget`, which asserts `CLEAR_SLICE_CHARS == model::buffer_replacement::REPLACEMENT_CLEAR_SLICE_CHARS`. Pinning it against the **shared** constant in another file is what makes the test resistant: it kills the mutant and simultaneously documents the intent — the load clear budget is deliberately the same 64 KiB the bounded buffer-replacement workflow uses, so a slice of either kind costs one paragraph-aligned pass |

Re-run after the fix, same command:

```
76 mutants tested in 7m: 16 missed, 57 caught, 3 unviable
# attributed by file:
caught: 41   missed: 0   unviable: 3   timeout: 0
```

## The 16 remaining survivors are pre-existing and out of this row

All 16 are `delete field ... from struct ... expression` mutants in files this
change does not touch:

```
$ cut -d: -f1 mutants.out/missed.txt | sort | uniq -c
      5 crates/lushtext-core/src/services/draft_service.rs
     11 crates/lushtext-core/src/services/file_tree.rs
$ git status --short | awk '{print $2}' | grep -E 'draft_service|file_tree'   # no output
```

`draft_service.rs` belongs to `WFR-DRAFT-RECOVERY` (slot 4) and `file_tree.rs`
to the workspace tree rows (slots 5 and 7). They are recorded here so a later
slot does not mistake them for load-workflow debt, and so nobody attempts to
exclude them: the `delete field` family is exactly the class
`MUTANTS_EXCLUDE_RE` cannot express.

## Merge-base note — `make mutants-diff` needs a workaround here

`scripts/run-mutants.sh` builds its diff with `git diff "${MUTANTS_BASE}..."` —
the three-dot **commit-range** form. For uncommitted work that range is empty, so
the lane reports:

```
$ make mutants-diff
Creating mutation diff against origin/main...
No diff hunks found; skipping changed-code mutation run.
```

That is a silent pass, not a clean run. The workaround, which a later session
should reuse rather than re-derive:

```
# `git add -N` is required: this change adds whole new files, and `git diff`
# does not see untracked paths.
$ git add -N crates/lushtext-core/src/ui/editor_page/load/ \
             crates/lushtext-core/src/ui/editor_page/document_identity.rs \
             crates/lushtext-core/src/ui/editor_page/restore_position.rs
$ git diff origin/main -- crates/ > /tmp/worktree.diff
$ MUTANTS_JOBS=4 MUTANTS_TEST_THREADS=4 \
    ./scripts/run-mutants.sh diff /tmp/worktree.diff
$ git reset            # drop the intent-to-add entries again
```

Result:

```
Found 44 mutants to test
44 mutants tested in 5m: 41 caught, 3 unviable
# attributed by file:
caught: 41   missed: 0   unviable: 3   timeout: 0
```

Identical to the focused `full` run above, which is expected: the module is new
in this change, so every mutant it generates is also a diff mutant.

## Anchors are deliberately coarse

Counts are file-level, and the one survivor is named by its mutation operator
and the constant it targets rather than by a frozen `line:col`. A line-precise
anchor would make a later simplification pass unable to touch the file without
invalidating this record.
