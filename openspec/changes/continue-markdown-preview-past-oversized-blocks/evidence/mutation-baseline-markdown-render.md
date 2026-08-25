# Mutation baseline: `services/markdown_render.rs`

Reference point for the task 10.7 parity comparison. Captured after section 1's
characterization tests were added and **before** any production change to the
planner, so the baseline reflects today's `plan_markdown_inner` under today's
tests plus the new characterization coverage.

## Command

```
MUTANTS_RE='markdown_render' \
MUTANTS_EXCLUDE_RE='delete field (active|pending) from struct' \
MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
scripts/run-mutants.sh full
```

### Scoping caveat

`--re` (`MUTANTS_RE`) matches mutant *names*, and a `delete field ...` mutant's
name does not carry the file path of the struct expression it deletes. A bare
`MUTANTS_RE='markdown_render'` run therefore also generates `delete field
active/pending` mutants from `services/single_flight.rs`,
`services/palette/index.rs`, and `services/palette/notes.rs`.

**The `MUTANTS_EXCLUDE_RE` in the command above does not remove them.** This was
verified in task 10.7: `scripts/run-mutants.sh list` still lists them with the
exclude applied, and every focused run — baseline and post-change alike — tested
exactly the same 31 foreign mutants. The regex is kept in the command only so the
baseline and parity runs stay byte-identical and therefore comparable; the
foreign mutants are actually counted out **by file attribution**, using
`grep -c markdown_render.rs mutants.out/{caught,missed,unviable}.txt`. Report the
`markdown_render.rs` column, never the whole-run column.

## Result (cargo-mutants 27.x, `cargo nextest`, 12m wall)

| Outcome | Whole focused run | `markdown_render.rs` only |
| --- | --- | --- |
| Generated (tested) | 136 | 105 |
| Caught | 72 | 57 |
| Missed | 57 | 41 |
| Unviable | 7 | 7 |
| Timeout | 0 | 0 |

Viable `markdown_render.rs` mutants: 98 (57 caught / 41 missed, 58.2% killed).

## Parity expectation for task 10.7

The post-change comparison must show that the relocated and rewritten planner
logic still *generates* mutants (no coverage disappearing behind a move) and
that the killed count does not regress relative to 57/98. New survivors are to
be killed with deterministic tests rather than exclusions.
