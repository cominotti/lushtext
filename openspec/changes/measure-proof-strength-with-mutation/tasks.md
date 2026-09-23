## 1. Remove harness code from the mutation scope (pre-existing blocker)

`harden-kani-lane-and-draft-token` (tasks 6.1–6.3) owns this exclusion and is
applied first. If it has landed, tasks 1.1–1.3 are **verify-only**: confirm
from the tool's list that no `kani_proofs` mutant remains and that the
recorded figures are present, then mark the tasks done without re-editing
`.cargo/mutants.toml`. Tasks 1.4–1.5 stay here only if that change did not
already add an equivalent `cfg(kani)` declaration check.

- [ ] 1.1 Record the "before" figures from the tool. Save the output of
  `cargo mutants --list --workspace` as the before list, and record from it:
  - the total (6,001 at 170212d9);
  - the per-file counts;
  - the 171 harness-file mutants (138 in `draft_service/kani_proofs.rs`,
    33 in `write_protocol/kani_proofs.rs`).
- [ ] 1.2 Add `crates/**/kani_proofs.rs` and `crates/**/kani_proofs/**` to
  `exclude_globs` in `.cargo/mutants.toml`, with a comment that explains the
  `cfg(kani)` reason.
- [ ] 1.3 Re-list and diff against 1.1. Verify all of the following:
  - the total drops by exactly the harness population;
  - no listed name matches `kani_proofs`;
  - every production file's count is unchanged.

  Record the figures in `docs/mutation-testing.md`, under Scope.
- [ ] 1.4 Extend `scripts/kani-shards.py check` so that every file named
  `kani_proofs.rs`, or under a `kani_proofs/` directory, is declared behind
  `#[cfg(kani)]`. Add self-test cases for this.
- [ ] 1.5 Confirm the check fails first. Temporarily remove the `#[cfg(kani)]`
  from one `mod kani_proofs;` declaration and confirm the check fails; then
  restore it. Confirm `make check-policy` passes.

## 2. Oracle table and baseline

- [ ] 2.1 Add the `ORACLES` table (design D2) to `scripts/kani-shards.py`,
  covering the four modules. Give each harness its recorded seconds from the
  programme record and a `ci` or `local` tier. Record
  `a_second_writer_breaks_the_journal_invariants` as "not an oracle", with
  its reason.
- [ ] 2.2 Extend `check`, with self-test cases:
  - every oracle harness resolves to exactly one discovered harness;
  - every oracle module exists;
  - every harness file's checked module is listed or marked
    "not an oracle".

  Confirm it fails first by misspelling one oracle harness.
- [ ] 2.3 Add an `oracle <module> [--tier ci|all]` subcommand that prints the
  ordered exact harness names.
- [ ] 2.4 Establish how Kani 0.68 reports an unsatisfiable `kani::cover!`:
  1. Write a throwaway harness in the scratchpad with a deliberately
     unsatisfiable cover.
  2. Record the exit status and the result-line format.
  3. Choose exit-status or line-parsing vacuity detection accordingly.
- [ ] 2.5 Measure the costs on the unmutated tree:
  - the incremental `cargo kani` compile cost C for `gtk-lush-widgets` and
    for `lushtext-core`, after a one-line touch of each module;
  - the per-harness seconds of every oracle harness.

  Update the table's recorded seconds with the measured values.

## 3. Proof-strength driver

- [ ] 3.1 Write `scripts/proof-strength.py`, implementing design D1, D3, and
  D4:
  - the `list`, `baseline`, `run`, and `report` subcommands;
  - disposable worktree creation and reuse;
  - mutant listing inside the worktree, with the repo `exclude_re` entries
    applied;
  - the `file` post-filter, with the floor reported;
  - `patch` apply and `git checkout` restore;
  - cheapest-first, fail-fast harness execution with per-harness timeouts;
  - the six outcome classes;
  - per-mutant JSON results keyed by revision and table hash;
  - `--shard k/n`.
- [ ] 3.2 Add a `--self-test` that runs without Kani. It covers:
  - diff application and restore on a fixture file;
  - the floor post-filter;
  - outcome classification from canned Kani output (verified, failed,
    should_panic now verifying, timeout, compile error, newly unsatisfiable
    cover);
  - resume skipping;
  - shard partition stability;
  - the refusal to join arms across revisions.

  Wire it into `make check-kani-shards`.
- [ ] 3.3 Implement the tests arm (D1 step 4, D5):
  - cargo-mutants over one module with `lushtext-core/property-tests`
    enabled, reading `mutants.out/outcomes.json`;
  - the join on mutant name.

  Measure its wall time per module.
- [ ] 3.4 Add the Makefile targets `proof-strength` and `proof-strength-list`,
  with `PROOF_STRENGTH_MODULE`, `PROOF_STRENGTH_TIER`, `PROOF_STRENGTH_SHARD`,
  and `PROOF_STRENGTH_ARM`. Reuse the `KANI_VERSION` guard from `make kani`,
  and add both targets to `.PHONY` and `make help`.
- [ ] 3.5 Prove the driver on one real mutant per module:
  1. Choose a mutant a harness obviously kills, for example `WriteProtocol::step`
     replaced with `Default::default()`.
  2. Confirm it is classified killed.
  3. Confirm the developer working tree is byte-identical before and after
     (`git status --porcelain` and a checksum of the module).

## 4. Measure and triage

- [ ] 4.1 Run the baseline, then both arms for `write_protocol.rs` and the
  `ci` tiers of `slice_geometry.rs` and `scroll_request.rs`. Save the
  per-mutant results.
- [ ] 4.2 Run both arms for `journal_core.rs` and for the `local` slice-loop
  tier of the geometry modules. The only mutants to run in that tier are those
  the `ci` tier did not already kill. Shard this locally as needed, and record
  the wall time and peak memory.
- [ ] 4.3 Produce the per-module table with `proof-strength.py report`. It
  shows:
  - mutants, the floor, and unviable;
  - tests-only kills, Kani-only kills (with should_panic kills and vacuity
    kills as sub-counts), and kills by both or either;
  - timeouts and survivors.
- [ ] 4.4 Triage every Kani survivor and timeout in the order of design D7:
  real defect, then add harness/assertion, then equivalent, then accepted gap.
  List each mutant by name with its class and justification.
- [ ] 4.5 For each real defect found, first write a test that fails on the
  shipped code, then fix it, and run the relevant data-safety or geometry
  lane. If no defect was found, record that explicitly.
- [ ] 4.6 For each "add harness/assertion" survivor:
  1. Add the assertion or harness in the owning `kani_proofs` module.
  2. Assign any new harness to exactly one shard.
  3. Run `make check-kani-shards`.
  4. Re-run the unmutated shard with `make kani KANI_SHARD=<shard>`, and
     confirm it passes within its budget.
  5. Re-run the driver for that mutant and confirm it is killed.
- [ ] 4.7 For each equivalent survivor that the tests arm also leaves alive,
  add one narrowly scoped `exclude_re` with a comment. Verify it against a
  generated mutant, following the configuration requirement's
  re-verification rule.

## 5. Lane placement

- [ ] 5.1 From the measurements in 4.1, decide whether the `ci` tier fits
  shards of 25 minutes or less.
  - **If it fits:** add `.github/workflows/proof-strength.yml`:
    `workflow_dispatch` only, a shard matrix, `timeout-minutes: 30`, and the
    Fedora 44 container and Kani install as in `kani.yml`, with results
    uploaded as artifacts. Run `make check-workflow-timeouts`.
  - **If it does not fit:** record the local-only decision and the measured
    numbers.
- [ ] 5.2 If the workflow was added, dispatch it once and record its shard
  times.

## 6. Documentation sync

- [ ] 6.1 Add a "Proof strength (N9)" section to
  `docs/next/formal-verification.md`. It holds:
  - the revision, the Kani and cargo-mutants versions, and the feature set;
  - the per-module table;
  - the triage list;
  - the harnesses added;
  - any defects fixed.

  Add every accepted gap to the deferral inventory.
- [ ] 6.2 Mark N9 done in `docs/next/formal-verification-next.md`, with a
  pointer to that section and a one-paragraph summary of what the kill rates
  showed.
- [ ] 6.3 Update `docs/mutation-testing.md`:
  - the harness exclusion and its figures, under Scope;
  - the `proof-strength` commands, under Commands;
  - the relation to the Kani lane, under Relation to Other Gates.
- [ ] 6.4 Update the `make` target lists in `.agents/rules/build.md`,
  `AGENTS.md` (Build Commands), and `README.md`. If a workflow was added,
  update the CI description in `.agents/rules/build.md` and `AGENTS.md`.
- [ ] 6.5 Run `make check` and `make test`, then
  `openspec validate measure-proof-strength-with-mutation --strict`.
