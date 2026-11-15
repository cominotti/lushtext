> **Outcome (2026-11-15).** V1, the drift check, the in-place probe and both
> Kani-first attempts were made; V2 stopped after scoping, and the maintainer
> closed the evaluation without V2 (so without V3) and without a Kani
> follow-up, accepting recommendation (a): Kani stays the only maintained
> tool. The record is `docs/next/formal-verification-verus.md`; the plan
> below is kept as proposed.

## Why

Kani is a bounded model checker. Its proofs are complete for finite targets,
such as the durable-write protocol and the whole-pixel geometry functions over
`i32`, but they stay bounded wherever the state is unbounded:

- The draft journal's invariants S1–S4 are proved for 2–3 ids and 7–8
  actions after the first startup. The two-window harness
  (`journal_invariants_hold_across_two_windows`, shard `core-multi-window`,
  8.9 min / 2.7 GiB on the runner) runs at **7** actions in the lane. Eight
  proved locally, but at 15.3 GB with the default solver (over the runner's
  12 GiB margin) and 1724 s with kissat (over its 25-minute margin), so the
  programme record carries this as a recorded bound reduction.
- Liveness L1 is proved as "a dirty editor is clean within 7 fault-free
  autosave steps, after the lane drains", after any action prefix **within
  the harness bound** (shard `core-journal-liveness`, 15.2 min / 4.8 GiB on
  the runner). The step count is already a constant; the bound that remains
  is on the prefix, that is, on which states are reachable.
- The Quint/TLA+ evaluation checked L1 as a true "eventually" property under
  weak fairness in TLC (E9): it held at 1 id and ran out of the 30-minute cap
  at 2 ids. Its TLAPS lemma (E6) noted that S1 would need a large inductive
  invariant.
- For the slice loop, both in-Kani attempts were made. Loop contracts failed
  on resources (CBMC out of memory under a 25 GB cap; the second harness had
  not finished after 80 minutes). The compositional bin-independence argument
  carried "any number of bins" within the per-bin domain. By the Kani-first
  rule, that claim gets no other tool.

Verus is a deductive verifier for Rust. It checks user-written
specifications, proofs, and loop invariants by SMT, for all inputs and all
lengths. Verified code is Rust, written in Verus's supported subset, either
inside a `verus!` block or annotated in place with `#[verus_spec]`
attributes, and the guide states that annotated code still builds with a
plain `cargo build`. This does not recreate a model in another language. It
does still need a bridge if the verified code is a copy of production, and
the journal's environment (disk, windows, crashes) is a model in any tool, as
it is in Kani. The costs are the annotation, the proof maintenance, a pinned
toolchain that is not the workspace's, and a trusted base (the `vstd`
specifications of `std`, and any `external_body` or `assume_specification`).

Verus has no built-in temporal logic. Liveness is proved with ranking
functions (`decreases` measures) or with third-party embeddings such as
Anvil's `verus-tla`. So the practical unbounded form of L1 here is the one
Kani already states: clean within 7 fault-free steps, from **every** state
the journal can reach. That follows from V2's inductive invariant.

The recorded rules forbid adopting a tool on opinion
(`formal-verification-kani`: "External modelling tools are adopted only on
recorded empirical evidence") and require two in-Kani attempts before another
tool is discussed for an unbounded claim ("Unbounded claims are attempted in
Kani first"). The maintainer authorised a narrow, time-boxed evaluation on
2026-09-25, to measure whether Verus closes these gaps at an acceptable cost,
and on 2026-11-15 made this change part of the formal-verification
programme and its goal. No unbounded claim is currently required by a
trigger, so this change also amends the Kani-first rule to allow such an
evaluation, while adoption stays behind both rules. The evaluation ends in a
**recommendation for the maintainer**, recorded in
`docs/next/formal-verification.md`; adopting Verus as a maintained tool is
out of scope and would be its own change.

The cheapest outcome is also a likely one: the first Kani attempt, a one-step
inductive harness, may remove the **action** bound inside Kani alone at the
lane's id and window scope. The evaluation runs that attempt before any
Verus work, so a Kani-only answer is found first if it exists.

## What Changes

- Add a disposable evaluation under `formal/evaluation/verus/`, outside every
  build, test, lint, policy, audit and CI gate, in two standalone Cargo
  packages, each with its own empty `[workspace]` table and `Cargo.lock`:
  - `proofs/`: the Verus copies and proofs, with a `rust-toolchain.toml`
    for Verus's pinned rustc;
  - `drift/`: the production-side table emitter, built with the workspace's
    own toolchain through a path dependency on `lushtext-core`, as the
    `stateright` comparator is.

  Both build into `build/formal-evaluation/` (gitignored), and both are
  listed in the root `Cargo.toml` `exclude`.
- Run three targets, each calibrated against what Kani already proved:
  - **V1, calibration:** `clamped_preview_width` (and `band_floor` if time
    allows), integer functions Kani proves over every `i32`. Verus must
    re-prove the known properties and refute the known counterexample
    (`preview_width_is_not_always_a_third`) by proving a concrete witness
    against it, not merely by failing to prove it. `viewport_slice`
    is not the calibration target, because it is `f64` code and Verus's float
    reasoning is limited.
  - **V2, unbounded journal safety:** S1–S4 plus "one owning window per id"
    as an **inductive invariant**, for any number of ids, windows and actions,
    including crashes and restarts, under the one-process assumption A6 (K8
    shows the claim is false without it). The proof must fail on at least two
    real pre-fix defects that the Kani journal harnesses kill: the
    `Unavailable` restore that preserved nothing, and the per-window
    coordinator (`TwoWindowBaselineScope`) that let a stale manifest copy skip
    registration. Further mutants come from the N9 proof-strength population
    of `journal_core`; only those an invariant harness kills count, because
    the decision-table harness alone kills mutants the invariants cannot
    see.
  - **V3, L1 from every reachable state:** the 7-step fault-free bound, for
    every state satisfying V2's invariant. An "eventually, under fairness"
    form with other actors interleaving (E9's form) is a stretch goal. The
    slice-bin "rest" claim is not a target (see Non-Goals in the design).
- Before V2 and V3, make and record the two Kani-first attempts for each
  claim: a one-step inductive harness (the loop-contract obligation, with its
  base case and a satisfiability cover), or a loop-contract harness, and a
  compositional argument, drafted in a scratch worktree and recorded as the
  slice attempts were. If the inductive step proves the claim for any number
  of actions at a fixed scope, Verus targets only what remains (any number of
  ids and windows). After V2, re-run the Kani inductive step with the
  strengthened invariant Verus needed.
- Probe the in-place adoption shape once: annotate `journal_core` with
  `#[verus_spec]` in a scratch worktree (never committed) and run
  `cargo verus verify` on the real crate. Record whether it works and what it
  touches, including the toolchain gap (Verus's rustc against the workspace
  pin).
- For each target, record:
  - lines of spec, proof and invariant against lines of verified code;
  - the rewrite footprint, as a diff against the production module;
  - the trusted base: every `external_body`, `assume_specification`, and
    `vstd` specification relied on;
  - verification time and memory, with the machine load at the time;
  - known answers reproduced, and mutants caught;
  - how a production change would propagate to the proof.
- Record **AutoVerus** as "not attempted". The maintainer declined sending
  project code to its external LLM API (2026-09-25).
- Write a comparison and decision record, `docs/next/formal-verification-verus.md`,
  whose decision section is a recommendation to the maintainer.
- Update the programme record and the candidate list.

## Capabilities

### New Capabilities
<!-- none: an evaluation, not a maintained capability -->

### Modified Capabilities
- `formal-verification-kani`:
  - The evidence rule names deductive verifiers that check Rust, such as
    Verus, alongside modelling tools. It adds the metrics a deductive verifier
    must record: annotation ratio, rewrite footprint, trusted base, and proof
    maintenance. A known-false claim is reproduced only by a proved witness,
    and a mutant is caught only by the named obligation failing.
  - The Kani-first rule names the one-step inductive harness, with its base
    case and a satisfiability cover, as equivalent to the loop-contract
    attempt.
  - The Kani-first rule allows a maintainer-authorised, disposable evaluation
    to measure another tool on an unbounded claim before that claim is
    needed. The evaluation still records both Kani attempts for the claim,
    and narrows its target to whatever part of the claim a Kani attempt did
    not verify. Adoption still requires both attempts to fail and the claim
    to be needed.

## Impact

- **New and disposable:** `formal/evaluation/verus/` (`proofs/`, `drift/`);
  new `verus-*` targets of `scripts/formal-evaluation.sh`, run through the
  existing local-only `make formal-evaluation FORMAL_EVAL_TARGET=…`, with each
  check in its own `scripts/heavy.sh` call; tool
  installs under `build/formal-evaluation/tools/` (the
  `0.2026.11.02.8bf68ee` x86-64 Linux archive is 36 MB, 139 MB unpacked,
  and bundles Z3; its pinned rustc 1.99.1 is installed into a private
  `RUSTUP_HOME` there); and two root `Cargo.toml` `exclude` entries.
- **Docs:** `docs/next/formal-verification-verus.md`,
  `docs/next/formal-verification.md` (§2, the deferral inventory, and the
  recommendation), `docs/next/formal-verification-next.md` (tool criteria,
  the change table, and the Lean note), and the `make formal-evaluation`
  entries in the Makefile help text, `.agents/rules/build.md` and README.md's
  formal-verification paragraph (still local-only; AGENTS.md no longer lists
  the target, since its catalogue defers to `make help`).
- **Comment only:** `journal_core.rs`'s module rustdoc, which states L1 as
  "within six journal steps" against `LIVENESS_STEPS = 7`.
- **Unchanged:** production code (beyond that comment), Kani harnesses and shards, GTK axioms,
  the responsiveness lane, CI workflows, `cargo deny`, and hakari. A
  follow-up change carries any adoption.
