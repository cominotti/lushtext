# Mutation baseline — command-palette pure policy (task 4.10)

Pre-relocation baseline for the pure decision logic about to move out of
`crates/lushtext-core/src/ui/command_palette/mod.rs` and `imp.rs` into
`crates/lushtext-core/src/ui/command_palette/policy.rs`.

Captured 2026-08-25 against `bd9461a` plus this change's `openspec/` and docs
edits (no Rust source moved yet), `cargo-mutants 27.0.0`.

## The baseline is zero, by construction — and that is not "parity holds"

`.cargo/mutants.toml`'s `examine_globs` covers `model/**`, `services/**`,
`ui/**/policy.rs`, and two hand-listed pre-convention UI files. **No file under
`crates/lushtext-core/src/ui/command_palette/` is in that set**, and none of them
is named `policy.rs`. Every decision this change relocates therefore generates
**zero mutants today**.

This is the asymmetry `mutation-testing`'s equal-counts phrasing does not
describe. That phrasing governs a relocation between two already-scoped
locations, where before and after counts must match. Here the source location is
unscoped, so the move is a coverage **gain**: `policy.rs` enters `examine_globs`
by convention and the relocated decisions start generating mutants that did not
exist before. The obligation is therefore *stronger* than parity — every newly
generated mutant must be caught or unviable with a stated reason. Reporting this
as "0 → 0, parity holds" would be technically true and substantively false.

## Exact commands

Scope verification (this is the run that establishes the zero):

```
MUTANTS_RE='crates/lushtext-core/src/ui/command_palette/' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list
```

Output: empty, exit 0. **0 mutants listed.**

## The two-part scoping is still required, and here is the proof

Dropping `MUTANTS_EXCLUDE` shows why the slot-1 evidence file insisted on it. In
cargo-mutants 27.0.0 `--re` does not filter the
`delete field <field> from struct <T> expression in <fn>` mutant kind:

```
MUTANTS_RE='crates/lushtext-core/src/ui/command_palette/' \
./scripts/run-mutants.sh list        # 34 lines listed
                                     # 0 of them mention command_palette
```

All 34 are `delete field` mutants in `crates/lushtext-core/src/services/**`,
matched by no part of the regex. Survivors must therefore be **attributed by
file**, never by trusting `--re` alone, and the post-move run in
`mutation-parity-palette-policy.md` reuses the same two-part scoping so the two
runs describe the same population.

## Contemporaneous control

The already-migrated exemplar policy module was listed in the same session to
prove the tooling was working rather than silently matching nothing:

```
MUTANTS_RE='crates/lushtext-core/src/ui/search_panel/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list        # 29 mutants listed
```

29, not the 15 recorded in slot 1's baseline, because slot 1's later work added
policy to that module. The number is not being compared to anything here; it is
the control that says an empty palette listing means "nothing in scope", not
"the harness matched nothing at all".

## Reproducing after relocation

```
MUTANTS_RE='crates/lushtext-core/src/ui/command_palette/policy\.rs' \
MUTANTS_EXCLUDE='crates/lushtext-core/src/services/**/*.rs' \
./scripts/run-mutants.sh list        # re-verify scope first
./scripts/run-mutants.sh full        # same env
```

Re-verifying the scope with `list` before `full` is mandatory, not bookkeeping:
the unfilterable `delete field` population depends on which files `examine_globs`
examines, and `ui/command_palette/policy.rs` enters that examined set the moment
it exists, so the exclusion glob that reduced the listing to zero before the move
may not reduce it to only-palette mutants after it.
