## Context

The Kani consolidation (`consolidate-formal-verification-on-kani`) put 27
harnesses over four I/O-free, GTK-free modules:

| Module | Mutants (cargo-mutants 27.0.0, measured 2026-09-23) | In default mutation scope? | Harnesses that drive it |
|---|---|---|---|
| `crates/gtk-lush/widgets/src/slice_geometry.rs` | 29 | no (GTK Lush crates are outside `examine_globs`) | geometry tier (≈ 3.5 min total) and the slice-loop harnesses (35–370 s each) |
| `crates/gtk-lush/widgets/src/scroll_request.rs` | 26 | no | request tier (≈ 6 s) and the slice-loop harnesses |
| `crates/lushtext-core/src/services/draft_service/journal_core.rs` | 44 | yes | `a_dirty_editor_becomes_clean_without_faults` (57 s), `a_dirty_editor_may_need_seven_steps` (53 s, `should_panic`), `journal_invariants_hold_under_crashes` (487 s, ~8 GB) |
| `crates/lushtext-core/src/services/filesystem/write_protocol.rs` (`WriteProtocol`, `MoveProtocol`, `RenameProtocol`) | 34 | yes | five harnesses, ≈ 8 s total |

Four facts measured while writing this proposal constrain the design:

1. **cargo-mutants cannot use Kani as its test tool.** `--test-tool` accepts
   only `cargo` and `nextest`. The docs (mutants.rs) offer no custom test
   command. `cargo mutants --list --json` does carry each mutant's full diff.
2. **Harness files are currently mutated.** The configured scope generates
   171 mutants in `services/draft_service/kani_proofs.rs` (138) and
   `services/filesystem/write_protocol/kani_proofs.rs` (33). Those files are
   `#[cfg(kani)]`, so `cargo test` never compiles them, and every one of those
   mutants is reported as missed.
3. **Name filters carry the field-deletion floor.**
   `--re 'services/filesystem/write_protocol\.rs:'` lists 69 mutants, not 34,
   and the `journal_core` filter lists 79, not 44. In both cases the extra 35
   are the whole-scope struct field-deletion mutants that the mutation-testing
   spec already calls the unfilterable floor. When the repo config is active,
   `-f` did not narrow the list at all (all 6,001 came back).
4. **The tests-side evidence for the widget geometry is cross-crate.**
   `crates/lushtext-core/tests/properties/viewport_slice.rs` exercises
   `viewport_slice` and the request decisions. It sits behind the
   `lushtext-core/property-tests` feature, which the default mutation lane
   deliberately omits.

`kani` exposes `--exact`, `--fail-fast`, and `--harness-timeout`, which
together give per-harness control. The repository's hard CI ceiling is
`timeout-minutes: 30` per job, enforced by `scripts/check-workflow-timeouts.py`.

## Goals / Non-Goals

**Goals:**
- A kill rate for each module under three views: tests only, Kani only, and
  both.
- Every mutant that survives Kani is triaged into exactly one class, recorded
  in `docs/next/formal-verification.md`.
- Survivors triaged as "add harness/assertion" are actually killed in this
  change.
- The harness-mutant noise is removed from the default lane, and that removal
  is proved from the tool.
- A local, resumable command. Any CI job stays inside the 30-minute cap.

**Non-Goals:**
- Making proof strength a pull-request gate, or a pass/fail threshold on the
  kill rate. The first measurement is evidence, not a ratchet.
- Adding the GTK Lush widget modules to the default `make mutants-full` scope.
  That is a GTK Lush scope decision, left as an open question below.
- Mutating the I/O shells (`services/durable_write.rs`, the `draft_service`
  service body, `ViewportSliceBin::size_allocate`). No harness drives them,
  so a Kani oracle over them would only measure noise.
- Changing any harness bound (unwind, action count, bin count) to make the
  lane cheaper. A weakened oracle would measure a different proof.

## Decisions

### D1. A project driver consumes cargo-mutants' mutant list; cargo-mutants still runs the tests arm

`scripts/proof-strength.py` works in four steps:

1. It runs `cargo mutants --no-config --list --json -f <module>` inside the
   disposable checkout (D4), so the mutants match the checked source. It adds
   the repo's `exclude_re` entries for that file, so that calibrated
   equivalents stay excluded.
2. It keeps only the entries whose `file` is the module, which drops the
   field-deletion floor. It then reports the floor size, as the triage policy
   requires.
3. It applies each diff and runs the Kani oracle (D3).
4. For the tests arm, it runs cargo-mutants over the same module:
   - `lushtext-core` modules keep the repo config (nextest, `test_workspace`,
     and the calibrated timeouts), narrowed with `--re '<path>:'`;
   - widget modules use `--no-config --workspace --test-tool nextest -f <module>`;
   - both add `--features lushtext-core/property-tests` (see D5);
   - the driver reads `mutants.out/outcomes.json`.

The join key is the cargo-mutants mutant name (`path:line:col: description`).
Both arms list against the same revision with the same cargo-mutants version,
and the driver refuses to join if the revision or version differ.

*Alternatives considered:*
- A fake `nextest` on `PATH` that runs `cargo kani` instead. This is fragile:
  cargo-mutants passes nextest-specific arguments and parses nextest exits,
  and a shim would silently break on a cargo-mutants upgrade.
- `cargo mutants --in-place` plus a wrapper. It mutates the developer tree,
  which the in-place safety rule forbids outside CI.
- Waiting for an upstream custom test-command option. Nothing exists to wait
  on.

### D2. The oracle table lives beside the shard table

`scripts/kani-shards.py` gains an `ORACLES` table. It maps each module path to
an ordered list of `(harness exact name, recorded seconds, tier)`, where tier
is `ci` or `local`:
- `slice_geometry.rs`: `no_input_panics`, `whole_pixel_band_…`,
  `slice_containment_fails_for_general_f64`, `slice_lies_…`, and
  `slice_covers_…` at tier `ci`; then the `slice_loop_*` harnesses at tier
  `local`.
- `scroll_request.rs`: `requests_are_…`, `request_landing_fails_…`,
  `resting_bin_…`, and `request_lands_…` at `ci`; then `slice_loop_*` at
  `local`.
- `journal_core.rs`: the L1 harness (57 s), then the tight-bound
  `should_panic` (53 s), then the invariants harness (487 s), all at `local`.
- `write_protocol.rs`: all five `write_protocol::kani_proofs::` harnesses at
  `ci`.

`a_second_writer_breaks_the_journal_invariants` is deliberately **not** an
oracle. It pins an assumption (A6) rather than a property of `journal_core`,
it costs about 500 s and 9 GB, and its kill signal ("the known counterexample
vanished") says nothing the invariants harness does not already say more
strongly. The table records that reason inline.

The existing `check` subcommand is extended so that every oracle harness
resolves to exactly one discovered harness, every oracle module exists, and
every module that a harness file imports from is either listed or marked
`not an oracle: <reason>`. A new `oracle <module> [--tier ci|all]`
subcommand prints the ordered list the driver consumes. There is one table
and one checker, so the oracle cannot drift from the shards.

*Alternative:* derive the mapping automatically from harness imports. That is
rejected, because an import says nothing about which harness pins which
behaviour, and it cannot express the cheapest-first order.

### D3. Kill semantics and cost control

Each harness runs as its own
`cargo kani -p <pkg> --exact --harness <name> --harness-timeout <t>` in the
checkout, where `t = max(60 s, 3 × recorded seconds)`. Harnesses run
cheapest first, and the driver stops at the first one that fails.

The outcome classes are:
- **killed**: a harness reports verification failure. This includes a
  `should_panic` harness that now verifies, because the mutant removed a
  counterexample the harness pins, which is a behaviour change inside the
  constrained region.
- **vacuity kill**: every harness verifies, but a `kani::cover!` that is
  satisfiable on the unmutated baseline becomes UNSATISFIABLE.
- **timeout**: triaged like a survivor, never counted as a kill.
- **unviable**: the mutant does not compile under Kani. It is cross-checked
  against cargo-mutants' unviable class.
- **survived**: none of the above.

Before any mutant, the driver runs a **baseline**. Every oracle harness must
verify on the unmutated checkout. The driver records the set of satisfied
covers and each harness's measured seconds, and aborts if any harness fails.
This mirrors cargo-mutants' own baseline rule.

Whether Kani's exit status alone reveals an unsatisfiable cover is verified
in task 2.4. If it does not, the driver parses the per-harness result lines.

Estimated cost per mutant, where C is the incremental `cargo kani` compile
(to be measured in task 2.5):

| Module | Cost per mutant |
|---|---|
| `write_protocol` | C + ≤ 8 s |
| geometry tiers | C + ≤ 3.5 min |
| `journal_core` | C + up to about 10 min for survivors, and one to two minutes for mutants the L1 harness kills |
| slice-loop tier | up to about 20 min |

### D4. Isolation and resumability

The driver creates `git worktree add --detach target/proof-strength/wt <rev>`,
where `<rev>` defaults to `HEAD`, so uncommitted edits are never checked or
mutated. It applies each diff with `patch --forward <path>` and restores the
file with `git -C wt checkout -- <path>`. That is necessary because the
`+++` line of a cargo-mutants diff is a description, not a path.

It reuses one Kani target directory per worktree, so compiles stay
incremental. It writes one JSON result per mutant under
`target/proof-strength/<rev>/<module>/` with this shape:

```
{ name, class, harness, seconds, kani_version, table_hash }
```

A restart skips any mutant with a result for the same revision and
`table_hash`. `PROOF_STRENGTH_SHARD=k/n` partitions the filtered list by a
stable order, and `report` renders the Markdown table recorded in the
programme record.

### D5. The tests arm includes property tests

Property tests are the tests-side counterpart of the proofs; the
`viewport_slice` suite is the obvious case. Omitting them would understate
arm (a) and overstate how much Kani adds.

The default lane omits them to keep `mutants-full` fast. This lane covers
about 133 mutants, so the cost is acceptable, and it is measured in task 3.3.
The report states the feature set explicitly.

### D6. Lane placement

`make proof-strength` runs locally, with these parameters:
- `PROOF_STRENGTH_MODULE=<path|all>` (default `all`);
- `PROOF_STRENGTH_TIER=ci|all` (default `all`);
- `PROOF_STRENGTH_SHARD=k/n`;
- `PROOF_STRENGTH_ARM=tests|kani|both`.

`make proof-strength-list` prints the filtered population and the floor
without verifying anything.

A `.github/workflows/proof-strength.yml` job is added **only if** task 5.1's
measurement shows that the `ci`-tier modules (`write_protocol` plus the
geometry tiers) fit shards of at most 25 minutes, leaving headroom under the
30-minute cap. It would be `workflow_dispatch` only, reuse the Fedora 44
container and the Kani install from `kani.yml`, and upload the per-mutant
results. If the measurement does not fit, the lane stays local-only, and that
decision is recorded.

`journal_core` and the slice-loop tier are always `local`: a single surviving
mutant can approach 10–20 minutes and 8 GB, so any sharding that fits the cap
would need tens of jobs for one measurement.

### D7. Triage outcome handling

Every mutant that survives Kani or times out is triaged in the programme
record, in this order (the mutation-testing triage policy's order applied to
proofs):

1. **Is it a real defect?** If the shipped code violates its documented
   contract, fix it failing-first, under the pre-existing blockers rule.
2. **Add harness/assertion.** Add the missing assertion to an existing
   harness where the property belongs, or add a new harness to the shard that
   owns its module. Re-run the unmutated shard to confirm it still fits its
   budget, and re-run the driver for that mutant to confirm the kill.
3. **Equivalent mutant.** Record the argument. Add an `exclude_re` to
   `.cargo/mutants.toml` only when the tests arm also cannot distinguish it,
   scoped as narrowly as the existing calibrated entries.
4. **Accepted gap.** Record the reason, for example "outside the whole-pixel
   domain the proof states", "shell-only behaviour", or "constrained by tests
   (arm a kills it), not a proved property".

## Risks / Trade-offs

- [The incremental `cargo kani` compile of `lushtext-core` is slow (GTK
  dependency graph)] → Reuse one target directory per worktree. Measure C
  first (task 2.5). If C dominates, run the `write_protocol` and `journal_core`
  mutants for one revision in one session, so dependencies stay warm.
- [The `journal_core` survivors cost about 10 minutes and 8 GB each] →
  Cheapest-first ordering kills most mutants in the 57 s L1 harness. The lane
  is local-only, resumable, and sharded, and the report records the wall time.
- [A `should_panic` kill is a weaker signal than a proof failure] → Report
  should_panic kills as their own sub-count, so the kill rate does not
  overstate what is proved.
- [Timeouts inflate the survivor list] → The per-harness timeout is three
  times the baseline, and every timeout is triaged individually. None is
  silently treated as a kill.
- [The oracle table drifts from the harnesses] → The `check` extension runs
  in `make check-policy`, the same gate as the shard table.
- [The harness exclusion hides a future production file named
  `kani_proofs.rs`] → The name is reserved for `cfg(kani)` modules, and the
  gate asserts that every matching file is `#[cfg(kani)]`-gated at its `mod`
  declaration.

## Migration Plan

This change is additive tooling plus one scope exclusion. Rollback is
reverting the exclusion and deleting the script and targets. No persisted user
data, format, or runtime behaviour changes. Recorded results are
documentation.

## Open Questions

- Should `slice_geometry.rs` and `scroll_request.rs` join the default
  `examine_globs`, so that ordinary PR mutation covers them? That is a GTK
  Lush scope decision (`gtk-lush-stewardship`); this change only measures
  them.
- Once N1 lands and the Kani lane has measured CI times, should the `ci` tier
  of this lane run on the weekly Kani schedule rather than on dispatch only?
