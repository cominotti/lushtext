## Why

Kani is a bounded model checker. Its proofs are complete for finite targets,
such as the durable-write protocol and the whole-pixel geometry functions over
`i32`, but they stay bounded wherever the state is unbounded:

- The draft journal's invariants S1–S4 are proved for 2–3 ids and 7–8
  actions after the first startup. The two-window harness runs at 7 actions
  in the lane: 8 proved locally, but at 15.3 GB with the default solver (over
  the runner's memory margin) and 1724 s with kissat (over its time margin).
- Liveness L1 is proved as "a dirty editor is clean within 7 fault-free
  autosave steps", after any action prefix **within the harness bound**. The
  step count is already a constant; the bound that remains is on the prefix,
  that is, on which states are reachable.
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
attributes. This does not recreate a model in another language. It does
still need a bridge if the verified code is a copy of production, and the
journal's environment (disk, windows, crashes) is a model in any tool, as it
is in Kani. The costs are the annotation, the proof maintenance, a pinned
toolchain, and a trusted base (the `vstd` specifications of `std`, and any
`external_body` or `assume_specification`).

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
2026-09-25, to measure whether Verus closes these gaps at an acceptable cost.
No unbounded claim is currently required by a trigger, so this change also
amends the Kani-first rule to allow such an evaluation, while adoption stays
behind both rules.

## What Changes

- Add a disposable evaluation under `formal/evaluation/verus/`: a standalone
  Cargo package with its own `[workspace]` table, its own `Cargo.lock`, a
  `rust-toolchain.toml` for Verus's pinned rustc, and a `.cargo/config.toml`
  that builds into `build/formal-evaluation/`, as the `stateright` comparator
  does. It is listed in the root `Cargo.toml` `exclude`, and is outside every
  build, test, lint, policy, audit and CI gate.
- Run three targets, each calibrated against what Kani already proved:
  - **V1, calibration:** `clamped_preview_width` (and `band_floor` if time
    allows), integer functions Kani proves over every `i32`. Verus must
    re-prove the known properties and fail to prove the known
    counterexample (`preview_width_is_not_always_a_third`). `viewport_slice`
    is not the calibration target, because it is `f64` code and Verus's float
    reasoning is limited.
  - **V2, unbounded journal safety:** S1–S4 plus "one owning window per id"
    as an **inductive invariant**, for any number of ids, windows and actions,
    including crashes and restarts, under the one-process assumption A6 (K8
    shows the claim is false without it). The proof must fail on at least two
    real pre-fix defects that the Kani journal harnesses kill: the
    `Unavailable` restore that preserved nothing, and the per-window
    coordinator (`TwoWindowBaselineScope`) that let a stale manifest copy skip
    registration.
  - **V3, L1 from every reachable state:** the 7-step fault-free bound, for
    every state satisfying V2's invariant. An "eventually, under fairness"
    form with other actors interleaving (E9's form) is a stretch goal. The
    slice-bin "rest" claim is not a target (see Non-Goals in the design).
- Before V2 and V3, make and record the two Kani-first attempts for each
  claim: a loop-contract (inductive-step) harness and a compositional
  argument, drafted in scratch and recorded as the slice attempts were. After
  V2, re-run the Kani inductive step with the strengthened invariant Verus
  needed, to see whether Kani alone then closes the action bound.
- Probe the in-place adoption shape once: annotate `journal_core` with
  `#[verus_spec]` in a scratch worktree (never committed) and run
  `cargo verus` on the real crate. Record whether it works and what it
  touches.
- For each target, record:
  - lines of spec, proof and invariant against lines of verified code;
  - the rewrite footprint, as a diff against the production module;
  - the trusted base: every `external_body`, `assume_specification`, and
    `vstd` specification relied on;
  - verification time and memory;
  - known answers reproduced, and mutants caught;
  - how a production change would propagate to the proof.
- Record **AutoVerus** as "not attempted". The maintainer declined sending
  project code to its external LLM API (2026-09-25).
- Write a comparison and decision record, `docs/next/formal-verification-verus.md`.
- Update the programme record and the candidate list.

## Capabilities

### New Capabilities
<!-- none: an evaluation, not a maintained capability -->

### Modified Capabilities
- `formal-verification-kani`:
  - The evidence rule names deductive verifiers that check Rust, such as
    Verus, alongside modelling tools. It adds the metrics a deductive verifier
    must record: annotation ratio, rewrite footprint, trusted base, and proof
    maintenance.
  - The Kani-first rule allows a maintainer-authorised, disposable evaluation
    to measure another tool on an unbounded claim before that claim is
    needed. The evaluation still records both Kani attempts for the claim.
    Adoption still requires both attempts to fail and the claim to be needed.

## Impact

- **New and disposable:** `formal/evaluation/verus/`; new `verus-*` targets
  of `scripts/formal-evaluation.sh`, run through the existing local-only
  `make formal-evaluation FORMAL_EVAL_TARGET=…`; tool installs under
  `build/formal-evaluation/` (the Verus archive is about 490 MB and bundles
  Z3; its rustc is installed into a private `RUSTUP_HOME` there); and the root
  `Cargo.toml` `exclude`.
- **Docs:** `docs/next/formal-verification-verus.md`,
  `docs/next/formal-verification.md` (§2 and the deferral inventory),
  `docs/next/formal-verification-next.md` (tool criteria and Lean note), and
  the `make formal-evaluation` entries in AGENTS.md and
  `.agents/rules/build.md` (still local-only).
- **Unchanged:** production crates, Kani harnesses and shards, CI workflows,
  `cargo deny`, and hakari. A follow-up change carries any adoption.
