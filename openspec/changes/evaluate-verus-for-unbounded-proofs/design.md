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
- Changing production code, Kani harnesses, shards, GTK axioms, or CI. The one
  exception is a comment: `journal_core.rs`'s module rustdoc states L1 as
  "within six journal steps", which contradicts `LIVENESS_STEPS = 7`, and is
  corrected (task 7.4).
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

- **Finite domains** (the `journal_core` decisions, which are `pub` and so
  reachable from `drift/`): the table covers every input. Every decision but
  one takes enums, bools and small structs of them; each domain has at most a
  few hundred inputs, so the check is sound and cheap, and does not depend on
  formatting. The exception is `run_deletion`, which takes a start step and an
  `impl FnMut(DeletionStep) -> bool` and loops: its table enumerates every
  start step with every sequence of step outcomes up to the longest deletion
  (three steps), recording the terminal and the steps performed. Verus
  supports `FnMut` closures only partly (the guide: it "does not yet take
  advantage of the capturing capabilities supported by Rust's `FnMut`"), so
  the copy's form of `run_deletion` is recorded as footprint.
- **Integer domains** (`clamped_preview_width`, `band_floor`): both are
  `pub(crate)` (the first in `lushtext-core`'s private `markdown_preview::policy`,
  the second in `gtk-lush-widgets`), so `drift/` cannot call them, and the
  change makes no production item more visible. Instead the script extracts
  each function's source, with the constants it reads
  (`PREVIEW_MIN_WIDTH_SP`, `PREVIEW_MAX_WIDTH_DIVISOR`), from the pinned
  commit (`git show <commit>:<path>`), and `drift/` compiles that extracted
  text with the workspace toolchain and writes the table. The table covers a
  fixed set of boundary and sampled inputs (the Kani harnesses' interesting
  points: `i32::MIN`, `i32::MAX`, 0, ±1, the floor, 3·floor, and a seeded
  sample). The extraction fails loudly if the function's signature is not
  found exactly once, so a rename is a drift failure, not a silent pass.
  Where the copy's rewrite footprint is zero (the Verus-annotated copy, with
  its annotations removed mechanically, is the production text), the script
  also compares the rustfmt-normalised bodies; where the copy had to be
  rewritten (for example `.max`/`.min` or `map_or`/`saturating_add` replaced
  by forms Verus accepts), the text check is skipped for that function, and
  the table is the only drift check, which the report states. To keep the
  removal mechanical, a copy that keeps the production body is written in the
  `#[verus_spec]` attribute form, with its proof steps in separate lemma
  functions.

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

Verus must prove each Kani property, and must **refute** the counterexample:
a failed proof of the unconditional one-third form is not evidence, because
incompleteness, an `rlimit` hit, or a build error look the same. The known
answer counts as reproduced only when Verus proves a concrete witness against
the claim, for example `clamped_preview_width(500, 2) == 1` with
`3 * 1 > 2` (the production unit test's point). A mutant counts as caught
only when Verus reports the named postcondition or assertion as failed. The
script's `classify` reads Verus's output for this (task 1.4). Division and
`max`/`min` may need `nonlinear_arith` or vstd arithmetic lemmas; the report
records which.

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

Further mutants are drawn from the N9 population of `journal_core`. Only the
aggregates are committed (51 mutants at `71fc2e3a`, 33 killed by either arm),
not per-mutant verdicts, so the population is listed with
`make proof-strength-list PROOF_STRENGTH_MODULE=services/draft_service/journal_core.rs`
(no verification) and each sampled mutant's kill is confirmed by a scratch
Kani run under D5's run rules. **A mutant counts for V2 only if an invariant
harness kills it** (`journal_invariants_hold_under_crashes`,
`journal_invariants_hold_across_two_windows`, or the L1 harness for V3).
The programme record (N9, `71fc2e3a`) shows why: `ownership`'s `||`→`&&`
mutant survived all six journal harnesses, because they abstract away which
body a decision protects, and only `journal_decisions_follow_their_documented_rules`
kills it. A mutant that only the decision-table harness kills is reported as
a comparison (does V2's invariant see it?), not counted for or against V2. A
mutant no Kani harness kills does not count either. M1 and M2 are invariant
harness kills, so the "at least two real pre-fix defects" floor rests on them.

A mutant is caught only when the proof reports the inductive-step obligation
(or the named invariant conjunct) as failed, never on a timeout, an `rlimit`
hit, or a build error. Before mutants are run, a **vacuity probe** shows the
invariant is not trivially true: an `assert(false)` placed after the
invariant is assumed in the step must fail, and the initial state must be
shown to satisfy it (`Init ⟹ Inv`) with a concrete instance.

The E1 stamp-only set-aside naming is **not** a V2 mutant. The journal model
keeps preserved content as a set, so S1–S4 never saw it, and only K9 kills
it. It can be used only if V2 also models set-aside naming and proves K9's
property.

A failure that the proof reports but a Kani harness does not reproduce is
investigated before it counts.

### D4. V3 is L1 from every reachable state

Kani's L1 is a fixed 7-step fault-free pass after the lane drains, checked
from states that a bounded prefix (three actions after startup) reaches, in
the **single-window** scope (`SingleWindowScope`, 3 ids). The unbounded form is "`Inv(s)`,
a drained lane and a dirty editor imply that the pass is clean within 7
steps". This is a corollary over V2's invariant, and needs no temporal logic.
V3 therefore runs only if V2 closed; if V2 stops within its budget, V3 is
recorded as "not attempted: depends on V2".

A stretch goal is the E9 form: "eventually clean, under weak fairness of the
autosave pass, while other actors interleave a bounded number of times". It
needs a ranking function over the remaining environment budget and the pass
position, or the `verus-tla` library. It is attempted only if budget remains,
and the report records which route was taken.

V3 is stated over V2's multi-window model, so its precondition also requires
the editor's window to hold the id's claim (one owning window per id). That
is a strictly larger claim than Kani's single-window L1, and the report says
so. The k = 7 bound must be shown tight in Verus as in Kani
(`a_dirty_editor_may_need_seven_steps`): by a proved witness state from
which six fault-free steps leave the editor dirty, not by a failed proof at
k = 6. The proof must fail, on the named obligation, when the fault-free
assumption is dropped, and when the lane-drain precondition is dropped. The
same vacuity probe as V2 applies to V3's precondition.

### D5. Kani-first attempts, recorded per claim, run first

For V2 and V3, before the Verus attempt, draft two Kani attempts in scratch.
Record their text and outcome as phase 3 recorded its two:
1. **Inductive step:** a one-step harness over an arbitrary state assumed to
   satisfy the invariant (the cheaper form), or a loop-contract harness under
   `-Z loop-contracts`, with S1–S4 and one owning window per id as the
   invariant, at the lane's scope (2 windows, 1 file-backed + 1 untitled id).
   The one-step harness discharges the same obligation a loop contract over
   the action loop would (assume the invariant on an arbitrary state, run one
   iteration, assert it), so it is the loop-contract attempt the rule names
   (the amended requirement says so). It removes the action bound only with
   two companions: a base-case harness (`Init ⟹ Inv`, after the first
   startup as the lane harnesses start) and a `kani::cover!` showing that a
   state satisfying the assumed invariant exists. Without the cover, a too
   strong assumption would make the step vacuously true.
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

The scratch Kani runs are not shard runs and touch no shard table. The
scratch worktree is created with `git worktree add --detach` (no branch, so
nothing is pushed or left in the shared branch list) from a recorded commit,
under `build/formal-evaluation/`, and removed with `git worktree remove` when
the evaluation ends; its patch is kept under `formal/evaluation/verus/`. The
runs use their own `KANI_TARGET_DIR` under `build/formal-evaluation/`, go
through `scripts/heavy.sh` with an explicit cap, detached (`setsid nohup`),
one harness per call, inside the script's `measure` subcommand, so
`FORMAL_EVAL_TIMEOUT` and the RSS ceiling apply. The 8-action two-window
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
  work and runs in its own `scripts/heavy.sh --mem 16G -- <one command>`
  call, under `taskset -c 8-23`, never around a long script; long runs are
  detached. A `verus-*` target that runs several checks (a proof and its
  mutants, `verus-all`) is therefore **not** wrapped whole: the script wraps
  each check, and each `cargo` build it needs, in its own `heavy.sh` call,
  and the operator runs the target itself under `taskset` only. `heavy.sh`
  waits for a pending timing measurement, so no check starts beside one, and
  one check holds the shared lock for at most `FORMAL_EVAL_TIMEOUT` (1800 s,
  the same as `scripts/measure.sh`'s default lock timeout). Verus's thread
  count is set to at most 16 and its SMT `rlimit` is recorded. Inside each
  check the script's existing `measure` subcommand (not to be confused with
  `scripts/measure.sh`) enforces `FORMAL_EVAL_TIMEOUT` and a
  `FORMAL_EVAL_MEM_MB` default of 14336 for these targets. `heavy.sh` sets no
  address-space limit (it calls `with-memory-cap.sh` without
  `--address-space`); the hard limit is the 16G scope, and the 14336 MiB RSS
  ceiling sits 2 GiB under it, so the script records `memory-ceiling` before
  the scope's OOM kill. If the OOM kill trips first, that is recorded, and
  the cap is changed only with the reason recorded.
- **Verification times are cost figures, not verdicts.** They are recorded
  the way Kani's shard figures are, through `heavy.sh`, with the 1-minute
  load at start and end. The script's `measure` subcommand records neither
  today, so task 1.4 makes it append `/proc/loadavg` at start and end to each
  result row. `scripts/measure.sh` is for timing verdicts; taking
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
  Verus release, or `rlimit` changes. Mitigation: before the report, re-verify
  V1 and V2 (and V3 if it closed) on the newest release then available, the
  next non-rolling one if it exists, otherwise the newest rolling one (the
  rolling `0.2026.11.05.31e44e6` already exists), and record the result
  (task 7.1).
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

## Stage-2 review (2026-11-15)

An independent review against the code, the rules and the shard table.
**Verdict: sound; fixed in place.** Decisions, with their reasons:
- **Integer drift (D1).** `clamped_preview_width` and `band_floor` are
  `pub(crate)`, so `drift/` cannot call them; making them public would
  change production. `drift/` now compiles their source extracted from the
  pinned commit, and the text comparison applies only where the copy kept the
  production body, since a rewritten copy cannot match it by construction.
- **`run_deletion` (D1).** It takes a closure and loops, so "every input" is
  every start step with every outcome sequence; its `FnMut` parameter is
  footprint (the Verus guide: `FnMut` capture is not yet supported).
- **Attempt 1 is the loop-contract attempt (D5, spec).** The base rule named
  only loop contracts; the one-step inductive harness discharges the same
  obligation, so the MODIFIED requirement now names it, with the base case
  and a satisfiability cover it needs to remove the action bound.
- **Refutation is a proved witness (D2, D4, spec).** "Verus fails to prove"
  is also what incompleteness and resource limits look like, so known-false
  claims need a proved witness and mutants need the named obligation to
  fail; V2 and V3 get a vacuity probe.
- **Mutant counting (D3).** The record shows invariant harnesses miss
  decision-table mutants (`ownership`'s `||`→`&&`), so only invariant-harness
  kills count for V2; per-mutant verdicts are not committed, so each is
  confirmed by a scratch run.
- **V3 scope (D4).** Kani's L1 is single-window; V3 over V2's multi-window
  model is a larger claim and needs the claim precondition.
- **Machine rules (D6).** A multi-check target wrapped in one `heavy.sh` call
  breaks "one command per call", so the script wraps each check; `heavy.sh`
  sets no address-space limit, so D6's memory sentence was corrected; the
  script records no load today, so task 1.4 adds it.
- **Docs (tasks 7.3, 7.4).** AGENTS.md no longer lists `make
  formal-evaluation` (its catalogue defers to `make help`), so the targets are
  the Makefile help, `build.md` and README.md. `journal_core.rs`'s module
  rustdoc says L1 holds "within six journal steps", but `LIVENESS_STEPS` is 7
  and `a_dirty_editor_may_need_seven_steps` shows six is too few; it is wrong
  since `67aef874`. Under the pre-existing-blockers rule it is corrected here,
  comment only (no harness input changes, so no shard run).
- **Report completeness (task 7.1).** The evidence rule requires install and
  CI cost against the 30-minute cap and the bridge options; the report list
  now names them, and the newer-release re-verification has a task.

## Open Questions

None.
