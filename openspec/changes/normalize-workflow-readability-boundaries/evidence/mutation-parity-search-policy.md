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
