## Why

The Kani consolidation left 27 harnesses marked PROVED, but a proof is only
as strong as the property it states. Nothing tells us how much of
`slice_geometry`, `scroll_request`, `journal_core`, or `write_protocol` those
properties actually pin. A harness can pass vacuously, or assert something
weaker than the code's real contract. N9 in
`docs/next/formal-verification-next.md` proposes measuring this with the
mutation tool the project already runs. A surviving mutant is a concrete
behaviour that no harness constrains, which is a finding we can act on, not an
opinion.

Reading the lane for this proposal also found a pre-existing defect. The
configured cargo-mutants scope (`crates/lushtext-core/src/services/**/*.rs`)
generates **171 mutants inside `cfg(kani)` harness files**: 138 in
`services/draft_service/kani_proofs.rs` and 33 in
`services/filesystem/write_protocol/kani_proofs.rs`. That is out of 6,001 in
total. Ordinary builds never compile those files, so every one of those
mutants is reported as *missed*. They are noise in `make mutants-full`, and
they would drown any honest proof-strength number. Under the pre-existing
blockers rule this change fixes it.

## What Changes

- **Exclude harness code from the mutation scope.** Add `**/kani_proofs.rs`
  and `**/kani_proofs/**` to `.cargo/mutants.toml` `exclude_globs`. Prove the
  exclusion from the tool, not from reading the config: the list shrinks by
  exactly the harness population, and no production mutant disappears.
- **Add a Kani mutation oracle.** cargo-mutants can only drive `cargo test` or
  `nextest` (confirmed against its docs), so a new driver,
  `scripts/proof-strength.py`, does the following:
  - reads the `cargo mutants --list --json` diffs;
  - applies each diff in a disposable git worktree;
  - runs that module's harnesses one at a time, cheapest first, with
    `cargo kani --exact --harness … --harness-timeout …`;
  - stops at the first failing harness;
  - persists one resumable result per mutant.
- **Keep one mutant-to-harness table.** The oracle table maps each
  Kani-covered module to its ordered harness list. It lives in
  `scripts/kani-shards.py` beside the shard table, and
  `make check-kani-shards` checks it: every named harness exists, and every
  oracle module has at least one harness.
- **Measure three kill rates per module:**
  - (a) the existing unit and property tests only, via cargo-mutants and
    nextest;
  - (b) Kani only;
  - (c) both, as the union joined on the mutant name.

  Unviable mutants, timeouts, and vacuity kills (a `kani::cover!` becoming
  unsatisfiable) are reported separately and never counted as ordinary kills.
- **Triage every mutant that survives the Kani oracle** into exactly one of:
  - **add harness/assertion**: the change adds it and re-runs to show the kill;
  - **equivalent mutant**: with the reasoning, plus a narrow `exclude_re` only
    when it is also equivalent for the tests arm;
  - **accepted gap**: with the reason, for example outside the proved domain,
    or pinned by tests only.

  The results and the triage are recorded in a new "Proof strength" section of
  `docs/next/formal-verification.md`.
- **Lane placement that respects the 30-minute job cap:**
  - `make proof-strength` (with `PROOF_STRENGTH_MODULE=` and
    `PROOF_STRENGTH_SHARD=k/n`) is a local, resumable command;
  - a `workflow_dispatch`-only CI job covers the cheap modules
    (`write_protocol`, the whole-pixel geometry tier), and only if the measured
    shard times fit;
  - the `journal_core` and slice-loop tiers, whose harnesses cost up to about
    8 minutes and 8 GB each, stay local-only with a documented command.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `mutation-testing`: ADDED requirements. Formal-verification harness code is
  excluded from the mutation scope, and a proof-strength lane measures
  Kani-covered modules against both oracles.
- `formal-verification-kani`: ADDED requirement "Proof strength is measured by
  mutation". Every Kani-covered module has a recorded kill rate, and every
  survivor has a recorded triage.

## Impact

- **Config:**
  - `.cargo/mutants.toml`: the harness exclusion;
  - the oracle table in `scripts/kani-shards.py`.
- **Scripts:**
  - new `scripts/proof-strength.py`, with a `--self-test`;
  - `scripts/kani-shards.py`: the `oracle` subcommand and table check.
- **Build:**
  - `Makefile`: the `proof-strength` and `proof-strength-list` targets;
  - optionally `.github/workflows/proof-strength.yml` (dispatch-only,
    `timeout-minutes: 30`, which `check-workflow-timeouts` must accept).
- **Tests and harnesses:**
  - new or tightened Kani assertions in
    `crates/gtk-lush/widgets/src/kani_proofs.rs`,
    `services/draft_service/kani_proofs.rs`, and
    `services/filesystem/write_protocol/kani_proofs.rs`, for survivors triaged
    "add harness/assertion";
  - each harness keeps its shard and stays inside its shard budget.
- **Production code:** none changes behaviour. A survivor can reveal a real
  bug in a checked module. If that happens, the pre-existing blockers rule
  applies: a failing-first test, then the fix, in this change.
- **Docs:**
  - `docs/next/formal-verification.md` (the Proof strength section);
  - `docs/next/formal-verification-next.md` (N9 status);
  - `docs/mutation-testing.md` (scope and commands);
  - `.agents/rules/build.md`, `AGENTS.md` (which `.claude/CLAUDE.md` links to), and
    `README.md` (the new make targets).
