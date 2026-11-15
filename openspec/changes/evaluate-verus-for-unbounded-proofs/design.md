## Context

The programme record, `docs/next/formal-verification.md` §2, keeps Kani as
the single maintained tool, and `.agents/rules/build.md` ("Formal
Verification (Kani)") forbids a second specification language without a
recorded maintainer decision. The Quint vs TLA+ evaluation
(`docs/next/formal-verification-quint-vs-tlaplus.md`) rejected both
modelling languages for maintained properties, mainly because their models are
hand-written copies that drift from the code. It recorded `stateright` as the
strongest scale option for the journal, TLAPS as cheap for a small inductive
invariant (E6), and an unbounded L1 in TLC that held at 1 id and hit the cap
at 2 (E9).

Kani's remaining gaps are about **bounds**, not about finding bugs: S1–S4
hold for small scopes, and L1 holds from the states a bounded prefix reaches.
Verus attacks exactly that, by deductive proof over Rust.

### Measured baseline (Kani 0.68.0 / CBMC 6.11.0, from the programme record and `scripts/kani-shards.py list`, 2026-11-15)

| Harness | Bounds | Result | Runner shard budget |
|---|---|---|---|
| `journal_invariants_hold_under_crashes` | 1 window; 2 file-backed + 1 untitled id; 8 actions | PROVED (344 s, < 5 GB local) | `core-journal-and-write`: 12.0 min / 2.6 GiB (13 harnesses) |
| `journal_decisions_follow_their_documented_rules` | every input of the `journal_core` decision table | PROVED | same shard |
| `a_dirty_editor_becomes_clean_without_faults` | L1 at k = 7 after the lane drains (≤ 3 steps) | PROVED (311 s, 4.7 GB local) | `core-journal-liveness`: 15.2 min / 4.8 GiB |
| `a_dirty_editor_may_need_seven_steps` | L1 at k = 6 | `should_panic`: k = 7 is tight | same shard |
| `journal_invariants_hold_across_two_windows` | 2 windows × 1 file-backed + 1 untitled id; 7 actions | PROVED (306 s, 2.5 GB local) | `core-multi-window`: 8.9 min / 2.7 GiB |
| the same at 8 actions | local only | PROVED (647 s, 15.3 GB; kissat 1724 s, 3.1 GB) | not in the lane: over both margins |
| `TwoWindowBaselineScope`, 6 actions | not in the lane | FAILS: a body written without an entry (321 s, 14.6 GB) | — |
| `a_second_writer_breaks_the_journal_invariants` (K8) | 2 processes × 1 window, 6 actions | `should_panic` | `core-second-writer`: 7.4 min / 2.7 GiB |
| `preview_width_*` (6 harnesses) | every `i32` | 5 PROVED, `preview_width_is_not_always_a_third` a `should_panic` counterexample | `core-geometry-policies` (pull-request gate) |

The N9 proof-strength lane (programme record, "Proof strength (N9)")
mutated `journal_core` and recorded which mutants Kani kills; that population
is the mutant source for V2 beyond M1 and M2.

### Tool facts checked on 2026-11-15 (re-checked at task 1.1)

- The latest non-rolling Verus release is `0.2026.11.02.8bf68ee`
  (published 2026-11-02; the rolling `0.2026.11.05.31e44e6` is newer). Its
  `verus-0.2026.11.02.8bf68ee-x86-linux.zip` is 36,022,359 bytes, SHA-256
  `f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c`, and
  unpacks to 139 MB: `verus`, `cargo-verus`, `rust_verify`, `z3`, the
  prebuilt `vstd` (`libvstd.rlib`, `vstd.vir`) and the builtin macro
  libraries. Releases up to `0.2026.10.26` were about 487 MB; the cause of
  the drop is not known and is recorded at task 1.1, together with a check
  that `z3` is present (the `0.2026.09.06` release omitted it,
  verus-lang/verus#2914).
- Its `version.json` and `rust-toolchain.toml` pin **Rust 1.99.1**, as does
  Verus `main`. The proposal's 2026-09-25 facts (release
  `0.2026.09.20.aef82ed`, rustc 1.98.1, a 490 MB archive) are superseded.
- The workspace pins and requires **Rust 1.98.0** (`rust-toolchain.toml`,
  `rust-version`). The parallel `rust-1-100-adoption` change keeps that pin,
  because the shipped Flatpak SDK extension is still 1.98.0, and records
  1.99.1 as the floor of any future 1.99 pin. So Verus's rustc is newer than
  the workspace's, not equal to it, and either side may move on its own
  schedule (weekly Verus releases; the workspace pin follows the Flatpak SDK).
- The Verus guide (`cargo_verus.html`) states that Verus-annotated code also
  builds with a normal `cargo build`, with `vstd` from crates.io and
  `[package.metadata.verus] verify = true`; `cargo verus verify` and
  `cargo verus build` drive verification. Task 1.1 confirms the release's
  `#[verus_spec]`, `nonlinear_arith` and thread-count options from the guide
  or Context7, never from memory.
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
- Record the Kani-first attempts for each claim next to the Verus result,
  and take a Kani-only answer when one exists.
- Produce a decision record that the evidence rule accepts, ending in a
  recommendation for the maintainer.

**Non-Goals:**
- Adopting Verus as a maintained tool, lane, dependency or skill. Any
  adoption is a separate follow-up change under a recorded maintainer
  decision.
- Changing production code, Kani harnesses, shards, GTK axioms, or CI.
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

### D1. Verify a copy, check it against production by table exchange, and probe in-place once

The main proofs run on a copy of each target's code in
`formal/evaluation/verus/proofs/`. The copy keeps the evaluation off the
production toolchain and gates. The diff against production is recorded as
the rewrite footprint.

**Two toolchains, never crossed.** Verus's rustc (1.99.1) and the workspace
pin (1.98.0) differ, and each moves independently. A drift helper that
compiles `lushtext-core` with Verus's rustc would work today only because
1.99.1 ≥ 1.98.0, would fail the day the workspace's `rust-version` passes
Verus's pin, and would compile production with a compiler the project does
not ship. So the drift check never builds one side with the other side's
toolchain:

- `drift/` is built with the **workspace** toolchain, through a path
  dependency on `lushtext-core` (as `formal/evaluation/stateright/` is). It
  calls the real functions over every input and writes a canonical table
  (one line per input, sorted).
- `proofs/` is built with **Verus's** toolchain, by `cargo verus build`. A
  small executable in it calls the copied functions over the same inputs and
  writes the same table.
- The script diffs the two tables and fails on any difference.

The script selects each toolchain explicitly (`RUSTUP_TOOLCHAIN` and the
private `RUSTUP_HOME` for `proofs/`, the repository's pin for `drift/`),
because `rust-toolchain.toml` lookup walks up the directory tree and a nested
package would otherwise inherit the wrong pin. `drift/` and `proofs/` are
siblings, so neither sits under the other's `rust-toolchain.toml`.

By domain:

- **Finite domains** (every `journal_core` decision takes enums and bools):
  the table covers every input. Each domain has at most a few hundred inputs,
  so the check is sound and cheap, and does not depend on formatting.
- **Integer domains** (`clamped_preview_width`, `band_floor`): a table over a
  fixed set of boundary and sampled inputs (the Kani harnesses' interesting
  points: `i32::MIN`, `i32::MAX`, 0, ±1, the floor, 3·floor, and a seeded
  sample), **plus** a normalised text check: strip the Verus annotations from
  the copy, format both sides with rustfmt, and require identical function
  bodies. The text check is the sound half; the table catches a copy that
  type-checks differently.

A plain `cargo build` of the annotated copy under the workspace toolchain
(the guide says it works) would allow one-process comparison instead; task
1.5 may record whether it does, but the design does not depend on it.

**The copy is pinned.** Each copied function records the `origin/main`
commit it was copied from. Queued changes will edit the copied code while the
evaluation runs (`apply-verification-altitude-redesigns` and
`verify-shell-conformance-against-proven-cores` touch `draft_service`), so a
drift failure after they land is expected. The report records the commit the
proofs hold for and whether the drift check still passed at report time; the
evaluation does not chase production. It never touches those changes' areas.

The environment model (disk, windows, crashes, restarts) is a port of
`kani_proofs.rs`, as it was for `stateright`. That port is a model in both
tools, not a new bridge to production. Keeping both models is a maintenance
cost, and the report records it.

**The in-place probe.** Adoption would put annotations on production code,
so the evaluation measures that shape once:
- annotate `journal_core` with `#[verus_spec]`, in a scratch git worktree
  under `build/formal-evaluation/`, created from a recorded commit;
- run `cargo verus verify` on `lushtext-core` with Verus's toolchain;
- record whether its dependencies (GTK `-sys` crates included), edition 2024,
  `const fn`, the workspace lints, and crate-relative imports work, and keep
  the diff as a patch file in the evaluation directory.

"Blocked: the workspace's `rust-version` or a dependency is above what
Verus's rustc accepts" is a valid recorded result, and is itself adoption-cost
evidence: in-place adoption would tie the workspace's toolchain to Verus's
weekly pin. The probe never reaches a production path.

*Alternatives considered:*
- A path dependency on `lushtext-core` from the Verus package (the
  2026-09-25 design): rejected, because it crosses toolchains (above).
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
close, the journal lane, and faults.

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

Further mutants are drawn from the N9 population of `journal_core` that a
Kani harness kills (including `journal_decisions_follow_their_documented_rules`);
each is listed with the harness that kills it. A mutant no Kani harness kills
does not count as evidence for V2.

The E1 stamp-only set-aside naming is **not** a V2 mutant. The journal model
keeps preserved content as a set, so S1–S4 never saw it, and only K9 kills
it. It can be used only if V2 also models set-aside naming and proves K9's
property.

A failure that the proof reports but a Kani harness does not reproduce is
investigated before it counts.

### D4. V3 is L1 from every reachable state

Kani's L1 is a fixed 7-step fault-free pass after the lane drains, checked
from states that a bounded prefix reaches. The unbounded form is "`Inv(s)`,
a drained lane and a dirty editor imply that the pass is clean within 7
steps". This is a corollary over V2's invariant, and needs no temporal logic.
V3 therefore runs only if V2 closed; if V2 stops within its budget, V3 is
recorded as "not attempted: depends on V2".

A stretch goal is the E9 form: "eventually clean, under weak fairness of the
autosave pass, while other actors interleave a bounded number of times". It
needs a ranking function over the remaining environment budget and the pass
position, or the `verus-tla` library. It is attempted only if budget remains,
and the report records which route was taken.

The proof must fail when the fault-free assumption is dropped, and when the
lane-drain precondition is dropped.

### D5. Kani-first attempts, recorded per claim, run first

For V2 and V3, before the Verus attempt, draft two Kani attempts in scratch.
Record their text and outcome as phase 3 recorded its two:
1. **Inductive step:** a one-step harness over an arbitrary state assumed to
   satisfy the invariant (the cheaper form), or a loop-contract harness under
   `-Z loop-contracts`, with S1–S4 and one owning window per id as the
   invariant, at the lane's scope (2 windows, 1 file-backed + 1 untitled id).
2. **Compositional argument:** per-id and per-window independence, or the
   reason none applies.

**The claim is split, so a Kani success narrows Verus's target instead of
being duplicated.** "For any number of actions at the lane's id and window
scope" and "for any number of ids and windows" are separate claims:
- if attempt 1 proves the inductive step (with S1–S4 alone, or with a
  strengthening found in scratch), the action bound is gone **in Kani**. The
  report records that as option (a′) evidence, and V2 targets only the
  remaining claim (any number of ids and windows), per the amended
  Kani-first rule;
- if attempt 2 also carries the id and window generality, V2 is recorded as
  "not targeted: verified in Kani", and V3 goes the same way.

After V2, re-run attempt 1 with the strengthened invariant Verus needed. If
Kani then proves the inductive step at its own scope, the report records
this as the main comparison. Nothing in this change enters the maintained
lane; carrying such a harness is a follow-up change.

The scratch Kani runs are not shard runs and touch no shard table. They run
in the scratch worktree with their own `KANI_TARGET_DIR` under
`build/formal-evaluation/`, through `scripts/heavy.sh` with an explicit cap,
detached (`setsid nohup`), one harness per call. The 8-action two-window
proof already peaked at 15.3 GB, so an arbitrary-state step is expected to
be heavy: the cap is 16G, kissat is tried before raising it, and a raise is
recorded with its reason.

### D6. Budgets, versions, and the machine

- **Versions:** the latest non-rolling Verus release at task 1.1, its pinned
  rustc and bundled Z3, resolved from the release page. Record them with
  checksums, never from memory.
- **Isolation:** Verus's rustc goes into a private
  `RUSTUP_HOME`/`CARGO_HOME` under `build/formal-evaluation/tools/`, so the
  user's rustup state is untouched. Nothing is installed outside
  `build/formal-evaluation/` (gitignored).
- **Budgets,** declared in the report's log before each target starts. The
  total cap is 23 agent-hours (AutoVerus's hour returned to the pool is not
  re-spent):

  | Item | Budget |
  |---|---|
  | Setup and drift check | 2 h |
  | V1 | 3 h |
  | Kani-first attempts | 3 h |
  | V2 | 10 h |
  | V3 | 3 h |
  | In-place probe | 2 h |

  When a budget runs out, the target is recorded as "not completed within
  budget", with where it stopped. Early stops: V1 failing its known answers
  stops the evaluation (D2); a Kani success narrows or retires V2 and V3
  (D5); V3 waits on V2 (D4).
- **Machine rules** (`.agents/rules/verification-runs.md`, "Shared-machine
  agent work"): every Verus, Z3, cargo, and scratch-Kani command is heavy
  work and runs as `taskset -c 8-23 scripts/heavy.sh --mem 16G -- <one
  command>`, one command per call, never around a long script; long runs are
  detached. `heavy.sh` waits for a running timing campaign, so no run starts
  beside one. Verus's thread count is set to at most 16 and its SMT
  `rlimit` is recorded. Inside that scope the script's existing `measure`
  subcommand (not to be confused with `scripts/measure.sh`) enforces
  `FORMAL_EVAL_TIMEOUT` and a `FORMAL_EVAL_MEM_MB` default of 14336 for
  these targets, below the 7/8 address-space limit `with-memory-cap.sh` sets
  for a 16G scope. If the address-space limit itself trips Verus or Z3, that
  is recorded, and the cap is changed only with the reason recorded.
- **Verification times are cost figures, not verdicts.** They are recorded
  the way Kani's shard figures are, through `heavy.sh`, with the 1-minute
  load at start and end. `scripts/measure.sh` is for timing verdicts; taking
  its exclusive lock for a multi-minute proof would drain every other agent
  for no verdict. A figure taken at a load of 4 or more is labelled as such.

### D7. Decision options (a recommendation for the maintainer)

The decision section recommends one of these, and combines them only where
each part has evidence. The recommendation is recorded in
`docs/next/formal-verification.md`; acting on it is the maintainer's
decision and a separate change.
- **(a) Keep Kani alone.**
- **(a′) Keep Kani alone, and carry Kani's inductive-step harness** with the
  invariant found here, in a follow-up change, if D5's attempt or re-run
  proved it. Its runner cost against the shard margins is part of the
  evidence.
- **(b) Adopt Verus for named pure cores alongside Kani,** through a separate
  follow-up change. That change states:
  - which modules;
  - in-place or copy;
  - how the toolchain gap between Verus's pin and the workspace pin is held;
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

- **The copy may be unrepresentative of production.** Mitigation: D1's table
  exchange, the pinned copy commit, and the in-place probe.
- **Production moves under the copy.** Queued changes edit `draft_service`.
  Mitigation: the copy is pinned to a recorded commit and the report states
  which commit the proofs hold for; a later drift failure is recorded, not
  chased.
- **Proof effort balloons.** E6 already warned that S1 needs a large
  invariant. Mitigation: the per-target budgets, the early stops, and a
  result recorded honestly as "not within budget".
- **Verus's supported subset excludes a construct the core relies on**, for
  example iterator chains, `const fn`, or the crate-relative imports in
  `journal_core`. Mitigation: record it as footprint, because it is part of
  the adoption cost.
- **Proof brittleness.** SMT proofs can break with a new Z3, a new weekly
  Verus release, or `rlimit` changes. Mitigation: re-verify V1 and V2 on one
  newer release, if one exists before the report, and record the result.
- **Toolchain churn.** Weekly releases and a pinned rustc that already
  differs from the workspace's (1.99.1 against 1.98.0). Mitigation: D1 never
  crosses toolchains; record install time, size, and CI cost against the
  30-minute job cap, as the evidence rule requires.
- **The trusted base hides a gap.** Mitigation: list every trusted item. A
  proof that relies on a trusted item for a decision under test does not
  count.
- **Shared machine.** Heavy SMT and CBMC runs compete with parallel agents.
  Mitigation: D6's `heavy.sh` rules and caps.

## Migration Plan

None: evaluation only. Rollback deletes `formal/evaluation/verus/`, the
`verus-*` script targets, and the two `exclude` entries.

## Maintainer Decisions

**2026-09-25**
- **The Kani-first amendment is accepted.** An authorised disposable
  evaluation may measure a tool before an unbounded claim is needed, under the
  conditions the MODIFIED requirement states. Adoption still requires both
  Kani attempts to fail and the claim to be needed.
- **AutoVerus is not run.** No project code is sent to an external LLM API.
  Task 6.2 records it as "not attempted: maintainer declined external code
  submission", and its 1 h budget is removed.

**2026-11-15**
- **This change is part of the formal-verification programme and its goal**,
  with the other three pending formal-verification changes. It stays a
  disposable, local-only evaluation in the spirit of `make
  formal-evaluation`: never a gate, never a CI lane, tools in the gitignored
  `build/formal-evaluation/`. Adopting Verus is out of scope; the evaluation
  ends in a recommendation recorded in `docs/next/formal-verification.md`.

## Stage-1 refresh (2026-11-15)

The change was proposed on 2026-09-25 and re-read against the code and
records seven weeks later. **Decision: still worth doing, refined.** Evidence:
- The gaps it targets are unchanged on `main`: the two-window harness still
  runs at 7 actions with 8 recorded as a bound reduction, L1 is still bounded
  by the prefix, and no harness or record since closed either. The targets
  (`journal_core`, `TwoWindowBaselineScope`, `clamped_preview_width`,
  `band_floor`) still exist. Since the proposal commit (`848f94f8`),
  `journal_core.rs` and `slice_geometry.rs` are unchanged; the journal
  harness file gained the decision-table harness
  (`journal_decisions_follow_their_documented_rules`, `71fc2e3a`, tidied in
  `2d2a51b1`), and `markdown_preview/policy.rs` gained only rustdoc `Kind:`
  lines (`7d188d0d`).
- The delta spec's base text still matches `main` for both MODIFIED
  requirements.
- Verus is maintained and installable: weekly non-rolling releases through
  `0.2026.11.02`.

Corrections made: the Verus release, archive size, checksum and pinned rustc
(1.99.1, not 1.98.1); the workspace pin (1.98.0, not a 1.96 minimum); the
drift check redesigned so it never crosses toolchains (D1); the copy pinned
against queued changes; the Kani-first attempts now narrow Verus's target
(D5); machine rules aligned with `verification-runs.md` (D6); the shard
budgets in the baseline; the gate audit extended to gates added since; the
N9 mutant population added to V2; the decision framed as a recommendation;
the 2026-11-15 maintainer decision recorded.

## Open Questions

None.
