## Context

The programme record, `docs/next/formal-verification.md` §2, keeps Kani as
the single maintained tool. The Quint vs TLA+ evaluation
(`docs/next/formal-verification-quint-vs-tlaplus.md`) rejected both
modelling languages for maintained properties, mainly because their models are
hand-written copies that drift from the code. It recorded `stateright` as the
strongest scale option for the journal, TLAPS as cheap for a small inductive
invariant (E6), and an unbounded L1 in TLC that held at 1 id and hit the cap
at 2 (E9).

Kani's remaining gaps are about **bounds**, not about finding bugs: S1–S4
hold for small scopes, and L1 holds from the states a bounded prefix reaches.
Verus attacks exactly that, by deductive proof over Rust.

Facts checked on 2026-09-25, to be re-checked at task 1.1:

- Verus ships weekly prebuilt releases (latest non-rolling:
  `0.2026.09.20.aef82ed`). The x86-64 Linux archive is about 490 MB and
  contains `verus`, `cargo-verus`, `rust_verify`, `z3`, and a prebuilt
  `vstd`. Its pinned toolchain is Rust 1.98.1, installed through rustup. The
  `0.2026.09.06` release omitted Z3 (verus-lang/verus#2914), so the install
  step must check that `z3` is present.
- Code is verified inside `verus!` or in place with `#[verus_spec]`
  attributes, with `vstd::prelude`. Unverified code is reached through
  `#[verifier::external_body]` wrappers or `assume_specification`; both are
  trusted, not checked.
- Liveness has no built-in temporal logic. Termination and progress use
  `decreases` measures; Anvil's `verus-tla` embeds TLA-style temporal
  reasoning as a library.
- AutoVerus (`microsoft/verus-proof-synthesis`) needs an OpenAI or Azure
  OpenAI API key and pins its own Verus commit built from source.

## Goals / Non-Goals

**Goals:**
- Measure whether Verus can prove, **without bounds**, a property Kani proves
  only for small scopes on this project's code.
- Measure the cost honestly: annotation volume, rewrite footprint, trusted
  base, verification time, proof maintenance, and toolchain.
- Record the Kani-first attempts for each claim next to the Verus result.
- Produce a decision record that the evidence rule accepts.

**Non-Goals:**
- Changing production code, Kani harnesses, shards, or CI.
- Replacing Kani. Kani stays the bug-finding and bounded lane whatever is
  decided.
- Verifying GTK-facing code. The GTK axiom ledger still models the
  environment.
- The slice-bin "rest" claim. The compositional Kani argument already carries
  it within the per-bin domain, so the Kani-first rule gives it no other tool.
  Its residual (geometry outside that domain) runs through `f64`
  `viewport_slice`, where Verus is weakest. It is recorded as not attempted,
  with these reasons.
- Proving more than one process. K8 shows S1 fails with two processes, so V2
  assumes A6.

## Decisions

### D1. Verify a copy, check it against production, and probe in-place once

The main proofs run on a copy of each target's code in
`formal/evaluation/verus/`. The copy keeps the evaluation off the production
toolchain and gates. The diff against production is recorded as the rewrite
footprint.

The copy is kept honest by a drift check, chosen by the code's domain:

- **Finite domains** (every `journal_core` decision takes enums and bools):
  a behavioural check. A small helper compares the copy's executable functions
  with the real ones over every input, and fails on any difference. The real
  functions come through a path dependency on `lushtext-core`, as in the
  `stateright` comparator. Verus's rustc (1.98.1) satisfies the workspace's
  1.96 minimum. This is sound and cheap, because each domain has at most a few
  hundred inputs. It does not depend on formatting.
- **Integer domains** (`clamped_preview_width`, `band_floor`): a normalised
  text check. Strip the Verus annotations from the copy, format both sides
  with rustfmt, and require identical function bodies.

Task 1.4 confirms that the helper builds with the pinned toolchain, and
records how.

The environment model (disk, windows, crashes, restarts) is a port of
`kani_proofs.rs`, as it was for `stateright`. That port is a model in both
tools, not a new bridge to production. Keeping both models is a maintenance
cost, and the report records it.

**The in-place probe.** Adoption would put annotations on production code,
so the evaluation measures that shape once:
- annotate `journal_core` with `#[verus_spec]`, in a scratch git worktree
  under `build/formal-evaluation/`;
- run `cargo verus` on `lushtext-core`;
- record whether its dependencies, edition 2024, and crate-relative imports
  work, and keep the diff as a patch file in the evaluation directory.

The probe never reaches a production path.

*Alternatives considered:*
- A textual drift check for everything: rejected, because it is fragile and
  says nothing about behaviour.
- `external_body` or `assume_specification` over the real functions: rejected
  for decisions under test, because they are trusted, not verified. A mutant
  in them would pass unnoticed.

### D2. Calibrate first (V1)

Start with `clamped_preview_width`. Kani proves its properties over every
`i32`, and keeps `preview_width_is_not_always_a_third` as a counterexample.

Verus must prove each Kani property and fail to prove the counterexample.
Division and `max`/`min` may need `nonlinear_arith` or vstd arithmetic
lemmas; the report records which.

If Verus cannot reproduce the known answers at a reasonable cost, stop and
record that before V2. This mirrors the Quint/TLA+ calibration rule.

### D3. V2 is an inductive invariant, checked against real mutants

Prove `Init ⟹ Inv` and `Inv(s) ∧ step(s, a) = s' ⟹ Inv(s')` for every
action of the multi-window model, including crash, restart, window open and
close, and faults.

- **Structure:** the preferred form is an executable `step` with
  `requires self.inv()` and `ensures self.inv()`. The step calls the copied
  `journal_core` functions directly, so the decisions under test are verified
  code, not re-stated spec functions.
- **Collections:** unbounded ids and windows need `vstd` views, such as `Vec`
  or `Map`, in place of Kani's fixed arrays. That change is part of the
  footprint.
- **Invariant:** S1–S4 and "one owning window per id" are its backbone. Each
  strengthening conjunct is recorded.

The proof must **fail** on these real pre-fix defects, reintroduced one at a
time:
- **M1:** `unapplied_restore_disposition` maps `RestoreEnding::Unavailable`
  to "preserve nothing", its pre-fix form. `journal_invariants_hold_under_crashes`
  kills it.
- **M2:** the per-window coordinator, the model's `TwoWindowBaselineScope`,
  under which a stale manifest copy skips registration. The
  `verify-multi-window-draft-journal` baseline run kills it.

The E1 stamp-only set-aside naming is **not** a V2 mutant. The journal model
keeps preserved content as a set, so S1–S4 never saw it, and only K9 kills
it. It can be used only if V2 also models set-aside naming and proves K9's
property.

A failure that the proof reports but a Kani harness does not reproduce is
investigated before it counts.

### D4. V3 is L1 from every reachable state

Kani's L1 is a fixed 7-step fault-free pass, checked from states that a
bounded prefix reaches. The unbounded form is "`Inv(s)` and a dirty editor
imply that the pass is clean within 7 steps". This is a corollary over V2's
invariant, and needs no temporal logic.

A stretch goal is the E9 form: "eventually clean, under weak fairness of the
autosave pass, while other actors interleave a bounded number of times". It
needs a ranking function over the remaining environment budget and the pass
position, or the `verus-tla` library. It is attempted only if budget remains,
and the report records which route was taken.

The proof must fail when the fault-free assumption is dropped, and when the
lane-drain precondition is dropped.

### D5. Kani-first attempts, recorded per claim

For V2 and V3, before the Verus attempt, draft two Kani attempts in scratch.
Record their text and outcome as phase 3 recorded its two:
1. **Inductive step:** a loop-contract harness, or an equivalent one-step
   harness over an arbitrary state assumed to satisfy the invariant, with
   S1–S4 as the invariant.
2. **Compositional argument:** per-id and per-window independence, or the
   reason none applies.

After V2, re-run attempt 1 with the strengthened invariant Verus needed. If
Kani then proves the inductive step at its own id and window scope, the
action bound is gone in Kani itself. The report records this as the main
comparison. Nothing in this change enters the maintained lane.

### D6. Budgets, versions, and the machine

- **Versions:** the latest non-rolling Verus release, its pinned rustc and
  bundled Z3, resolved from the release page at the start. Record them with
  checksums, never from memory.
- **Isolation:** Verus's rustc goes into a private
  `RUSTUP_HOME`/`CARGO_HOME` under `build/formal-evaluation/tools/`, so the
  user's rustup state is untouched.
- **Budgets,** declared in the report's log before each target starts. The
  total cap is 24 agent-hours:

  | Item | Budget |
  |---|---|
  | Setup and drift check | 2 h |
  | V1 | 3 h |
  | Kani-first attempts | 3 h |
  | V2 | 10 h |
  | V3 | 3 h |
  | In-place probe | 2 h |
  | AutoVerus | 1 h |

  When a budget runs out, the target is recorded as "not completed within
  budget", with where it stopped.
- **Machine rules:** runs go through the script's existing `measure` wrapper,
  which enforces `FORMAL_EVAL_TIMEOUT` and `FORMAL_EVAL_MEM_MB`, and they run
  under `taskset -c 8-23` (E-cores only). Parallelism is at most 16 jobs, with
  Verus's thread count set to at most 16. One verification runs at a time.
  Memory stays modest: the ceiling is set to 16 GiB unless the report records
  why more was needed.

### D7. Decision options

The decision section chooses one of these, and combines them only where each
part has evidence:
- **(a) Keep Kani alone.**
- **(a′) Keep Kani alone, and carry Kani's inductive-step harness** with the
  invariant found here, in a follow-up change, if D5's re-run proved it.
- **(b) Adopt Verus for named pure cores alongside Kani,** through a separate
  follow-up change. That change states:
  - which modules;
  - in-place or copy;
  - how CI runs it within the 30-minute cap;
  - how proof maintenance is kept within budget.

  It is open only for a claim whose two Kani attempts both failed and which a
  trigger actually needs.
- **(c) Revisit on a named trigger,** such as the two-window journal needing
  more than 7 actions, a third window or process kind, or GTK Lush
  publication needing a claim outside the per-bin domain.

It records the losing options' evidence and the fate of
`formal/evaluation/verus/`.

## Risks / Trade-offs

- **The copy may be unrepresentative of production.** Mitigation: D1's drift
  check and the in-place probe.
- **Proof effort balloons.** E6 already warned that S1 needs a large
  invariant. Mitigation: the per-target budgets, and a result recorded
  honestly as "not within budget".
- **Verus's supported subset excludes a construct the core relies on**, for
  example iterator chains, `const fn`, or the crate-relative imports in
  `journal_core`. Mitigation: record it as footprint, because it is part of
  the adoption cost.
- **Proof brittleness.** SMT proofs can break with a new Z3, a new weekly
  Verus release, or `rlimit` changes. Mitigation: re-verify V1 and V2 on one
  newer release, if one exists before the report, and record the result.
- **Toolchain churn and size.** Weekly releases, a pinned rustc, and a
  490 MB archive. Mitigation: record install time, size, and CI cost against
  the 30-minute job cap, as the evidence rule requires.
- **The trusted base hides a gap.** Mitigation: list every trusted item. A
  proof that relies on a trusted item for a decision under test does not
  count.

## Migration Plan

None: evaluation only. Rollback deletes `formal/evaluation/verus/`, the
`verus-*` script targets, and the `exclude` entry.

## Maintainer Decisions (2026-09-25)

- **The Kani-first amendment is accepted.** An authorised disposable
  evaluation may measure a tool before an unbounded claim is needed, under the
  conditions the MODIFIED requirement states. Adoption still requires both
  Kani attempts to fail and the claim to be needed.
- **AutoVerus is not run.** No project code is sent to an external LLM API.
  Task 6.2 records it as "not attempted: maintainer declined external code
  submission", and its 1 h budget returns to the pool.

## Open Questions

None open at proposal time.
