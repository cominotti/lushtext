# Mutation baseline — search-panel policy modules (task 3.2)

This is the pre-relocation mutation baseline for the exemplar's two pure policy
modules, `crates/lushtext-core/src/model/search_flight.rs` and
`crates/lushtext-core/src/model/search_retirement.rs`. Section 5.3 must prove
parity against these counts after the modules relocate to
`crates/lushtext-core/src/ui/search_panel/policy.rs`.

Captured on 2026-08-24 against commit `bd9461a` (worktree clean apart from this
change's `openspec/` artifacts, `.cargo/mutants.toml`, and the new policy
check), `cargo-mutants 27.0.0`.

## Exact command

```
MUTANTS_JOBS=6 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
MUTANTS_RE='crates/lushtext-core/src/model/search_(flight|retirement)\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh full
```

`MUTANTS_EXCLUDE` is required, not cosmetic. In cargo-mutants 27.0.0, `--re`
does not filter the `delete field <field> from struct <T> expression in <fn>`
mutant kind: an unmatchable `--re` still lists 33 of them, all in
`crates/lushtext-core/src/services/**`. Excluding that glob after focused
matching reduces the listed scope to exactly the 15 mutants in the two target
modules. Section 5.3 must reuse the same two-part scoping so the before and
after runs describe the same population.

Verify the scope before running:

```
MUTANTS_RE='crates/lushtext-core/src/model/search_(flight|retirement)\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list
```

## Result

```
15 mutants tested in 3m: 1 missed, 12 caught, 2 unviable
```

| Outcome | Count |
| --- | --- |
| Generated (tested) | 15 |
| Caught | 12 |
| Missed (survived) | 1 |
| Unviable | 2 |
| Timeout | 0 |

Per module: `search_flight.rs` 5 mutants (3 caught, 2 unviable),
`search_retirement.rs` 10 mutants (9 caught, 1 missed).

## Missed mutant (baseline survivor, carried forward as-is)

```
crates/lushtext-core/src/model/search_retirement.rs:36:9:
  replace SearchRetirementSliceBudget::exhausted -> bool with true
```

This survivor is part of the baseline, not a defect introduced by the
relocation. Task 5.3 proves *parity*: after the move the same 15 mutants must be
generated and the same 12 caught. Closing this survivor is a separate,
optional improvement; if section 5 chooses to close it, the parity report must
state that the caught count rose from 12 to 13 by an added test rather than by a
scope change.

## Unviable mutants (baseline, expected)

```
crates/lushtext-core/src/model/search_flight.rs:61:9:
  replace WorkspaceSearchFlight::submit -> WorkspaceSearchSubmission with Default::default()
crates/lushtext-core/src/model/search_flight.rs:76:9:
  replace WorkspaceSearchFlight::finish -> Option<WorkspaceSearchStart> with Some(Default::default())
```

Both are unviable because the returned types do not implement `Default`. A
relocation must not change this: if either becomes viable or unviable
differently after the move, the types or their derives changed and that is a
behavior change to explain.

## Caught mutants (12)

```
crates/lushtext-core/src/model/search_flight.rs:76:9: replace WorkspaceSearchFlight::finish -> Option<WorkspaceSearchStart> with None
crates/lushtext-core/src/model/search_flight.rs:86:9: replace WorkspaceSearchFlight::clear_pending with ()
crates/lushtext-core/src/model/search_flight.rs:91:9: replace WorkspaceSearchFlight::snapshot -> WorkspaceSearchFlightSnapshot with Default::default()
crates/lushtext-core/src/model/search_retirement.rs:23:9: replace SearchRetirementSliceBudget::take -> usize with 0
crates/lushtext-core/src/model/search_retirement.rs:23:9: replace SearchRetirementSliceBudget::take -> usize with 1
crates/lushtext-core/src/model/search_retirement.rs:31:9: replace SearchRetirementSliceBudget::take_one -> bool with true
crates/lushtext-core/src/model/search_retirement.rs:31:9: replace SearchRetirementSliceBudget::take_one -> bool with false
crates/lushtext-core/src/model/search_retirement.rs:31:22: replace == with != in SearchRetirementSliceBudget::take_one
crates/lushtext-core/src/model/search_retirement.rs:36:9: replace SearchRetirementSliceBudget::exhausted -> bool with false
crates/lushtext-core/src/model/search_retirement.rs:36:24: replace == with != in SearchRetirementSliceBudget::exhausted
crates/lushtext-core/src/model/search_retirement.rs:41:9: replace SearchRetirementSliceBudget::retired -> usize with 0
crates/lushtext-core/src/model/search_retirement.rs:41:9: replace SearchRetirementSliceBudget::retired -> usize with 1
```

## Reproducing after relocation

After the move, the same population is addressed by pointing `MUTANTS_RE` at the
new path:

```
MUTANTS_RE='crates/lushtext-core/src/ui/search_panel/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh full
```

Before running `full`, section 5.3 MUST re-verify the scope with the same
two-part scoping through `./scripts/run-mutants.sh list` and confirm the listed
population is exactly the same 15 mutants as the baseline. The re-verification
is not optional bookkeeping: the unfilterable `delete field` mutant set depends
on which files `examine_globs` examines, and `crates/lushtext-core/src/ui/search_panel/policy.rs`
enters that examined set once the modules relocate, so the exclusion glob that
reduced the listed scope to 15 before the move may no longer do so after it.

Mutant names carry file and line, so line numbers will differ. Parity is
asserted on counts and on the mutated operation plus function name, not on the
literal name strings.
