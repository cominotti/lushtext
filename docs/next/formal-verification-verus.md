# Formal Verification — Verus for Unbounded Proofs, Measured

Status: **decision record** of the OpenSpec change
`evaluate-verus-for-unbounded-proofs`, written on 2026-11-15. It measures
Verus, a deductive verifier for Rust, on claims Kani holds only for bounded
scopes. The programme record
[`formal-verification.md`](./formal-verification.md) keeps phases, results,
axioms, and deferrals; this file keeps the evaluation and its recommendation.
**The maintainer decided on 2026-11-15** (§13): recommendation (a) is
accepted, Kani stays the programme's only maintained tool, and the
evaluation is closed without V2 and without a Kani follow-up.

The models live in `formal/evaluation/verus/`. They are **disposable
evaluation models, not maintained properties**: no build, test, lint, policy,
audit, or CI gate reads that directory, no production crate depends on it,
and no claim elsewhere rests on these models alone (§10, gate audit).

**Results in one paragraph.**

- **V1 (calibration) reproduced every known answer** on
  `clamped_preview_width`: the five properties Kani proves, the monotonicity
  over two calls, a proved witness against the known-false one-third form,
  and a Kani-rejected mutant caught on the named postcondition, in about 1 s.
- **The drift check works** in both directions it must: the copy's
  9,036-line decision table matches production's at the copy commit, and
  each of two deliberate one-line changes fails it.
- **The in-place probe verified the real `journal_core` inside
  `lushtext-core`** (37 functions, 0 errors, 15.7 s) with Verus's own rustc,
  after a rewrite of seven kinds and eight new crates.
- **The Kani-first inductive step** at the lane's two-window scope was not
  proved within its 3 h budget. After three rounds of strengthening
  (31 conjuncts), its base case passes and the invariant is satisfiable, but
  three actions still break four conjuncts, in six (action, conjunct)
  pairs (§4.1).
  On the way it found a ten-action trace beyond the 8-action bound in which
  the Kani model loses accepted work. Production cannot take that trace, so
  it is a model over-approximation, not a defect (§4.2).
- **V2 (the unbounded journal invariant in Verus) was not completed.** It
  stopped after scoping; no Verus model of the journal was written (§5), and
  the maintainer closed it unattempted on 2026-11-15. V3 depends on V2 and
  was not attempted.

**Recommendation (§13): option (a), keep Kani alone,** with option (c)'s
triggers named for a revisit; **accepted by the maintainer on 2026-11-15.**
Verus is not recommended for adoption: no trigger needs an unbounded claim,
and the one measurement that would argue for it (V2) does not exist. If an unbounded journal claim is ever needed, the
cheaper next step is to finish the Kani inductive step from the recorded
round-3 invariant, not to port the model to Verus.

## 1. Tool versions, checksums, and the machine

| Item | Value | Source |
|---|---|---|
| Verus release | `0.2026.11.02.8bf68ee` (latest non-rolling, published 2026-11-02) | release page, 2026-11-15 |
| Verus commit | `8bf68ee741df833ea5d0fe6a2255126fdea87e47` | the archive's `version.json` |
| Archive | `verus-0.2026.11.02.8bf68ee-x86-linux.zip`, 36,022,359 bytes, SHA-256 `f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c` | downloaded and checked by `verus-install` |
| Unpacked | 133 MB: `verus`, `cargo-verus`, `rust_verify`, `z3`, prebuilt `vstd` and the builtin macro libraries | `du -sh` |
| Pinned rustc | 1.99.1 | `version.json` (`"toolchain": "1.99.1-x86_64-unknown-linux-gnu"`) |
| Bundled Z3 | 4.16.0 | `z3 --version` |
| `vstd` from crates.io | `=0.0.0-2026-11-01-0251` (what this release's `cargo verus new` pins) | the generated `Cargo.toml` |
| Newer rolling release | `0.2026.11.05.31e44e6` | release page; used for re-verification (§8) |
| Workspace toolchain | Rust 1.98.0 (`rust-toolchain.toml`, `rust-version`) | repository |
| Kani | 0.68.0 / CBMC 6.11.0, the pinned lane versions | `scripts/kani-shards.py` |

**Why the archive shrank from about 487 MB to 36 MB.** Read from the two
archives' zip central directories with HTTP range requests, without
downloading them: releases up to `0.2026.10.26` (487,532,246 bytes) also
shipped Verus's build tree, a `deps/` directory of 1,259 MB unpacked and a
`build/` directory of 118 MB, plus rustc rlibs and developer tools
(`line_count`, `bump_crate_versions`). The new archive ships only the runtime
binaries. `z3` is present, so the `0.2026.09.06` gap (verus-lang/verus#2914)
did not recur; `verus-install` checks it anyway.

**Options used, checked against the Verus guide (Context7,
`/verus-lang/verus`, 2026-11-15) and the release's `--help`, not from
memory:** `#[verus_spec(r => requires … ensures …)]` attribute
specifications; `#[verus_verify(dual_spec, open)]` with
`#[verus_spec(r => returns f(args))]`, which generates an open spec function
from the same executable body; `#[verifier::allow_in_spec]` with a `returns`
clause; `assert(…) by (nonlinear_arith)`; `#[verifier::rlimit(N)]` and the
global `--rlimit`; `--num-threads`; `--time`; `--multiple-errors`;
`--no-verify`. Partial verification (`--verify-module`) needs
`cargo verus focus`, which forwards Verus's arguments to the root crate only;
Cargo options must precede the target selection (`--manifest-path` before
`--lib` or `--bin`).

**Machine.** The shared workstation of
`.agents/rules/verification-runs.md`, each heavy command in its own
`scripts/heavy.sh` call with an explicit memory cap. Runs made before the
parallel agent work rule (`.agents/rules/parallel-agent-work.md`) reached
this branch were pinned to CPUs 8–23 with `taskset`; the rolling-release
re-verification and the final drift check ran in the inherited cpuset.
Several other agents ran heavy work at the same time, so **every figure here
was taken at a 1-minute load of 4 or more** (the result table records load at
start and end of each run) and is a cost figure, not a timing verdict
(design D6). Caps below 16G (4G for the V1 checks, 8G for drift, the in-place
probe, and the smaller Kani runs) were chosen so the memory admission would
start them sooner; each kept `FORMAL_EVAL_MEM_MB` 2 GiB under its cap, so a
memory stop shows as `memory-ceiling`, not as an OOM kill. One Kani run hit
that ceiling at 8G and was re-run at 16G (§4.1).

**AutoVerus** (`microsoft/verus-proof-synthesis`) needs an OpenAI or Azure
OpenAI API key and pins its own Verus commit, built from source. It was not
run (§7.2).

## 2. Install and CI cost

| Step | Cost |
|---|---|
| Download | 36,022,359 bytes, 1 s |
| Unpack | 133 MB |
| Verus's rustc, `rustup toolchain install 1.99.1 --profile minimal` into a private `RUSTUP_HOME` | 593 MB, about 6 s |
| Whole cold `verus-install` (download, checksum, unpack, rustc) | 7 s, into a throwaway directory |
| `vstd` and the builtin crates from crates.io (first build) | about 26 s (the first `build-error` sample, which built them) |
| V1, a warm check | 0.6–1.1 s total, under 30 MiB peak tree RSS |
| Drift check | two release builds (production at the compared commit with the workspace toolchain, and the copy with Verus's) plus a diff; 44 s against `cfee0c48` with both target directories already built once; a cold run adds a release build of `lushtext-core` and its GTK dependencies |
| In-place probe, first build of `lushtext-core` and its GTK dependencies under rustc 1.99.1 | about 107 s; the verification itself 15.7 s, 1,298 MiB |

Against the repository's 30-minute CI job cap, a Verus job for V1 and the
in-place `journal_core` would fit easily: seconds to install, about 2
minutes to build `lushtext-core` cold with the second rustc, and seconds to
verify. The cost that
matters is not the job time but the second toolchain: Verus pins rustc 1.99.1
while the workspace pins 1.98.0, Verus releases weekly, and the workspace pin
follows the Flatpak SDK, so the two never move together (§7.1).

**Bridge options back to Rust**, as measured here:

1. **Copy plus drift tables** (V1 and the `journal_core` copy): the proof
   holds for a copy pinned to a commit, and a table exchange shows the copy
   behaves like production on every finite input and on seeded integer
   samples. It needs no change to production and no shared toolchain, and it
   drifts silently until the check is re-run (§3).
2. **In-place annotation** (§7.1): the proof holds for the shipped code, at
   the cost of a production dependency on `vstd`, a rewrite footprint in the
   module, and a build of the whole crate with Verus's rustc.
3. **`#[verifier::external_body]`**: trusted wrappers around unverified code.
   Not used for any decision under test; `run_deletion` stays outside
   `verus!` and is drift-checked, not verified.

## 3. Setup and the drift check

Budget declared: 2 h (setup and drift).

`formal/evaluation/verus/` holds two sibling standalone packages, each with
its own empty `[workspace]` table and `Cargo.lock`, both in the root
`Cargo.toml` `exclude`:

- `proofs/` builds with Verus's rustc, selected by `RUSTUP_TOOLCHAIN` and a
  private `RUSTUP_HOME`, never by directory lookup;
- `drift/` builds with the workspace toolchain at the compared commit,
  through a path dependency on `lushtext-core`, in a detached scratch
  worktree, with that commit's `Cargo.lock` (a fresh resolution picked newer
  gtk-rs crates that fail to build, `IsA<Accessible>` errors).

Both include `table/emit.rs`, which enumerates every input of every
`journal_core` decision (for `run_deletion`, every start step with each of
the eight three-step outcome sequences, recording the terminal, the steps
performed, and whether it ran past three) and, for `clamped_preview_width`,
the 25 × 25 edge pairs plus 4,096 seeded pairs and their `% 1024` variants.
`clamped_preview_width` is `pub(crate)` in production, so `verus-drift`
extracts it and the constants it reads with `git show` from the compared
commit, fails unless the signature occurs exactly once, and compiles the
extracted text in `drift/`. No production item was made more visible. Where
the copy keeps the production body (both V1 and every `journal_core`
decision except `orphan_body_decision`), the rustfmt-normalised function text
is compared too.

| Run | Result |
|---|---|
| copy at `85758eaf` | **pass**: 9,036 table lines identical; `clamped_preview_width` text identical |
| failing first: `drift-ownership-ignores-stale` (drops `facts.backing_stale \|\|` from `ownership`) | **fails** on the table: `ownership … backing_stale: true` gives `OtherVersion` in production and `Unregistered`/`Journal` in the copy |
| failing first: `drift-preview-reordered-clamp` (`.min(max).max(floor)`) | **fails** on the text; the table is identical, because the reordered clamp is equivalent whenever the bound is at least the floor, which it always is |

The second row shows why both comparisons exist: a semantically equivalent
edit passes the table and fails the text, so a changed body is noticed even
when the table cannot tell.

Copy commit: every copy was taken from
`85758eaf8fb8190439719dd6926a5b2b3ed21fb0` (`origin/main`, 2026-11-15). The
final drift result against the then-current `origin/main` is in §8.

## 4. Kani-first attempts (design D5)

Budget declared: 3 h. Scratch worktree
`build/formal-evaluation/scratch/kani-step`, created with
`git worktree add --detach` at `85758eaf`, never committed; its diff is
`formal/evaluation/verus/kani-step.patch`. Every run went through
`scripts/heavy.sh --mem <cap> --` around the script's
`verus-measure` subcommand, one harness per call, with its own target
directory under `build/formal-evaluation/`.

### 4.1 Attempt 1: the one-step inductive harness

The harness is the loop-contract obligation in its one-step form, over the
lane's two-window scope (`Journal<TwoWindowScope, 2, 2, 1>`: 2 windows, 1
file-backed and 1 untitled id, one process):

```rust
#[kani::proof]
#[kani::unwind(9)]
fn verus_eval_inductive_step() {
    let mut journal = any_state();
    kani::assume(journal.inductive_invariant());
    journal.step(0, any_below(2), kani::any(), any_below(2), kani::any());
    assert!(journal.contents_well_formed(), "step: W0");
    assert!(journal.s1_holds(), "step: S1");
    assert!(journal.s3_holds(), "step: S3");
    assert!(journal.s4_holds(), "step: S4");
    assert!(journal.single_owner_holds(), "step: one owner");
    assert!(journal.strengthening_holds(), "step: strengthening");
}
```

`any_state()` is an arbitrary state within the model's own domains (content
ids below the edit bound, arbitrary ancestor sets, an arbitrary lane holder,
every editor and window field arbitrary). The step's own inline assertions
(no body without an entry; S2 inside cleanup) are checked as well. Its
companions: `verus_eval_base_case` (the invariant after the first startup,
as the lane harnesses start) and `verus_eval_invariant_is_satisfiable` (a
`kani::cover!` that a non-trivial state, a running window with an open dirty
editor and accepted content, satisfies the assumed invariant). Diagnosis
harnesses put one `kani::cover!` on each (action, broken conjunct) pair, so
one run names every action that breaks every conjunct.

| Round | Invariant | Result | Time | Peak RSS | Load start/end |
|---|---|---|---|---|---|
| r0 | S1–S4, one owner, W0 (well-formed contents) | **fails**: a body written without an entry, W0, S1, S3, one owner | 133.4 s | 3,339 MiB | 8.3 / 10.6 |
| r1 | + R1 (7 lane and process conjuncts) | **fails**: W0, S1, S3, strengthening | 63.3 s | 4,031 MiB | 4.8 / 6.1 |
| diag-r1 | per-conjunct covers | memory ceiling at the 8G cap (7,266 MiB); re-run at 16G: W0 broken by Register, Commit, DeletionStep, RestoreApply, Startup; S1 by WriteBody, Commit, DeletionStep, ExecCleanup, RestoreApply, CloseWindow, Crash; S3 by Save, Discard, RestoreApply | 103.0 s | 7,663 MiB | 12.9 / 14.8 |
| diag-r2 | + R2 (8 per-editor conjuncts) + model fix F1 (§4.2) | W0, S3, S4 and one owner no longer break; S1 still broken by WriteBody; 11 strengthening conjuncts broken | 209.4 s | 13,606 MiB | 10.2 / 14.9 |
| diag-act-r2 | one harness per action | Startup C9; OpenWindow C26; RestoreApply C14; DeletionStep C26; Save and Discard C21, C22; Commit C6, C7, C21, C22; WriteBody S1, C6, C17, C19, C20, C25 | 747.3 s | 5,483 MiB | 12.6 / 14.9 |
| diag-act-r3 | + R3a–R3d, R1g and R2b relaxed, R2c exempting ids being deleted | S1 and every r2 pair but three (DeletionStep C26; Commit C21, C22) no longer break; six pairs still broken: ExecCleanup C30; DeletionStep C26, C30; Commit C21, C22, C30 | 838.7 s | 6,391 MiB | 10.6 / 10.7 |
| base case (r3 invariant) | INV after the first startup | **pass** | 28.0 s | 1,744 MiB | 11.1 / 12.6 |
| satisfiability cover (r3 invariant) | a running window, an open dirty editor, accepted content | **satisfied** | 40.5 s | 1,857 MiB | 12.9 / 12.1 |

Each round's conjuncts are numbered in `strengthening_mask()` (bits 0–30)
in the patch. The strengthening found, in words:

- **R1 (process and lane):** a stopped process holds no lane, no
  restoration, and no cleanup owner; the lane holder is a running window and
  matches what that window is doing (a pass for an open dirty editor, a
  deletion for an editor with a deletion in progress, a cleanup owner); a
  window's manifest copy is never newer than the disk's.
- **R2 (per editor):** a registration token exists only while its window
  holds the pass lane for that id; a written body awaits a commit only under
  the lane; a dirty editor's content is accepted or newer than every accepted
  one; an untitled id's body is listed by its entry; a running window's
  claim and the entry agree, except for an id some window is deleting; a
  preserved copy covers what a deletion retires.
- **R3:** a stopped process's windows list nothing (R3a); a token and a
  written body never coexist (R3b, which holds only with F1); a dirty buffer
  holds an edit (R3c); an open editor whose id has no entry holds no
  unresolved accepted work beyond the preserved copies (R3d).

**Outcome: not proved within budget.** The step harness was not re-run on
the round-3 invariant, because the per-action diagnosis already shows it
fails: three actions still break four conjuncts, in six pairs. What the
remaining pairs say:

- **R3d (C30) is too strong.** Removing a missing-body entry (ExecCleanup),
  a deletion's entry removal (DeletionStep), and a commit that drops an entry
  (Commit) all leave an open editor without an entry while its buffer still
  holds accepted work. S1 permits that, because the buffer counts as
  recoverable. R3d was meant for WriteBody's R2f break (writing an unlisted
  untitled body), so it needs a narrower form, for example one that only
  covers the editor about to write.
- **R2c (C26) under DeletionStep, and R2b/R2h (C21, C22) under Commit** are
  counterexamples to induction that were not yet classified. Each is either
  an unreachable state that needs one more conjunct, or a real gap of the
  model, and telling which needs a replay like §4.2's.

The diagnosis loop costs about 14 minutes of solver time a round at a
16G cap (5.5–6.4 GiB peak), plus the memory admission's waits on the
shared machine. The 3 h budget covered three rounds. A fourth round would
start from the patch.

**What this means for V2's target.** The action bound is **not** removed in
Kani, so V2's target stays the whole claim: any number of actions, ids, and
windows (design D5).

### 4.2 A trace beyond the bound: model over-approximation F1

r0's S1 counterexample, replayed as a concrete trace in the lane's
single-window scope (`verus_eval_trace_untitled_orphan`, untitled id 2), loses
accepted work in **ten actions after startup**:

1. Open, 2. Edit, 3. Register, 4. WriteBody, 5. Register, 6. WriteBody with
a fault (the fault releases the lane while the first written body still
awaits its commit), 7. Inspect, 8. ExecCleanup (deletes the now unlisted
untitled body), 9. Commit (accepts the deleted generation), 10. Crash.

Kani reports `S1: accepted work became unrecoverable` (40.6 s, 1,757 MiB).
The maintained harness `journal_invariants_hold_under_crashes` stops at 8
actions, so it cannot reach this.

It is **not a production defect**. In production
(`ui/window/drafts/autosave_execution.rs`) a pass takes the lane, writes
each candidate once, and commits before `finish_autosave_pipeline` releases
the lane, so no second registration can happen while a written body awaits
its commit. The model lets `Register` run again in that state, which is an
over-approximation. The scratch harness fixes it as **F1** (`register`
returns while `editor.written.is_some()`), and R3b depends on it.

The maintained harness was not changed: this change touches no harness. The
follow-up that would carry the inductive step must carry F1 with it, and its
own review must decide whether F1 belongs in the maintained model
(programme record, deferral inventory).

### 4.3 Attempt 2: the compositional argument

**None applies to the journal's ids or windows.** Per-id independence fails
because the ids share the process lane (one pass, deletion, or cleanup at a
time), the manifest (one commit covers every id), and orphan cleanup (which
inspects every id's body). Per-window independence fails because the windows
share the process journal: the whole point of the two-window harness is the
interaction through the lane and the manifest (phase 4, multi-window), and
the per-application coordinator was the fix for exactly such interactions. A
proof note extending one id or one window to many would have to show that
these shared resources never couple the ids, and the counterexamples of the
two-window harness show they do.

The existing attempts are cited, not repeated:

- **L1** (`a_dirty_editor_becomes_clean_without_faults`, k = 7, after the
  lane drains): bounded liveness from the states a bounded prefix reaches.
  The inductive form of its precondition is what V3 would have targeted.
- **The slice loop** (`extend-closed-loop-geometry-verification`, phase 3):
  loop contracts failed on resources, and the bin-independence argument
  carried "any number of bins rests and does not oscillate" within stated
  per-bin bounds. **The slice claim is not targeted here.**

## 5. V2: unbounded journal safety in Verus

Budget declared: 10 h. Target, as §4 left it: the whole claim, S1–S4 and
one owning window per id for any number of actions, ids, and windows (§4
removed no part of it).

**Not completed: stopped after scoping, then closed.** No Verus model of the
journal was written, and on 2026-11-15 the maintainer closed V2 without an
attempt (§13). What exists, and where it stopped:

- the `journal_core` copy in `proofs/src/journal_core.rs`, verified by Verus
  as executable code (every decision inside `verus!` with a generated open
  spec, so a model would reason about the decision bodies themselves), and
  drift-checked (§3);
- the strengthened invariant of §4.1, which is the invariant a Verus model
  would start from (with the open pairs of §4.1), in the patch;
- the port plan: `vstd` `Map<Id, Editor>` per window and `Map<WindowId,
  Window>` per process for unbounded ids and windows, the action relation as
  a spec function, and one lemma per action for `Inv ∧ step ⟹ Inv'`.

**Why it stopped.** The Kani model the port would copy is about 1,700 lines
of harness code, and the inductive invariant needed three rounds of
strengthening (31 conjuncts) at only two ids and two windows; quantified
versions over unbounded maps would need trigger annotations and per-action
lemmas for each of 16 actions. The implementation session spent its time on
the Kani-first attempt the rule requires first, the drift check, and the
in-place probe, and ended V2 at its scoping step. The 10 h budget was not
used up; V2 is recorded as not completed rather than as having run out of
budget.

Not done, as a consequence: the vacuity probe (task 4.2), mutants M1 and M2
and the N9 sample in Verus (4.3), the Kani re-run with Verus's invariant
(4.4), and the V2 metrics (4.5). The propagation walkthrough (4.5) is done
for the copy, below, and the drift check was re-run against current
`origin/main` (§8).

**How a production change would propagate.** A new `RestoreEnding` variant
in `journal_core` would need: (1) the copy updated by hand, inside `verus!`;
(2) the table in `table/emit.rs` extended, whose exhaustive `_exhaustive`
`match` fails to compile until the variant is listed, so the drift check
cannot silently skip it; (3) in a V2 model, the restore step's lemma
re-proved, and any invariant conjunct mentioning restore endings revisited;
(4) `verus-drift` re-run against the new commit. Steps 1, 2, and 4 are
mechanical; step 3 is the proof-maintenance cost the rule asks about, and it
is the one this evaluation could not measure.

## 6. V3: L1 from every reachable state

**Not attempted: depends on V2** (design D4), which the maintainer closed
unattempted on 2026-11-15. Task 5.2, the E9 form under weak fairness, is not
attempted either.

## 7. In-place probe and AutoVerus

### 7.1 In-place probe

Budget declared: 2 h. Scratch worktree `build/formal-evaluation/scratch/inplace`
at `85758eaf`; its diff is `formal/evaluation/verus/in-place-probe.patch`
(5 files, +187/−32, of which most is `Cargo.lock`).

Command: `cargo verus focus -p lushtext-core --lib -- --verify-module
services::draft_service::journal_core`, with Verus's rustc 1.99.1.

**Result: 37 verified, 0 errors**, 15.7 s total (3.9 s verification),
1,298 MiB, after three iterations. What the attempt found:

| Concern | Finding |
|---|---|
| Workspace `rust-version` (1.98) against Verus's rustc (1.99.1) | builds: `rust-version` is a minimum |
| GTK `-sys` dependencies | build under rustc 1.99.1 (the host has the GTK development files); about 107 s cold |
| Edition 2024, workspace lints | no problem |
| `const fn` | `#[verus_verify(dual_spec, open)]` works on the `const fn` decisions |
| `impl FnMut` parameter of `run_deletion` | left unannotated: an item without `#[verus_verify]` is external, so it is not verified (and not trusted for any decision) |
| `cfg_attr(kani, derive(kani::Arbitrary))` | coexists with the Verus attributes |
| Crate imports | the imported types (`model::draft`'s three, `cleanup_types`' retention reason) need `#[verus_verify]` in their own modules, so the footprint reaches beyond `journal_core` |
| Early `return` | not allowed in a `dual_spec` body ("return is not allowed in pure context"): `orphan_body_decision` becomes one `if` chain |
| `#[verus_verify]` on an `impl` block | "Multiple #[verus_spec] attributes"; the attributes go on each method |
| Associated items in a `returns` clause | need `Self::` (`Self::from_window_view`) |
| Associated constant `TRUSTED` | needs `#[verus_verify]`; an external associated constant is "not supported" |
| `use vstd::prelude::*` | needed in every annotated module |

**What the `vstd` dependency would cost an adoption.** Eight new crates in
`Cargo.lock` (`convert_case`, `synstructure`, `verus_builtin`,
`verus_builtin_macros`, `verus_prettyplease`, `verus_state_machines_macros`,
`verus_syn`, `vstd`), all MIT or MIT/Apache-2.0, which `deny.toml` allows;
the fresh resolution also moved `windows-sys` in several entries. An
adoption would regenerate the hakari `workspace-hack`, regenerate
`cargo-sources.json` for the Flatpak's offline vendoring (`make
cargo-sources`), and carry the same crates into the Snap build. `vstd`
builds with the workspace's 1.98.0 for a plain `cargo build` (the guide's
promise; not run here), but verifying needs Verus's rustc, so every
verification run builds the whole crate and its GTK dependencies a second
time with a different compiler.

**Verdict: in-place works, and the toolchain gap is the cost**, not a
blocker: it holds today because 1.99.1 compiles this workspace, and it
breaks the first week Verus moves to a rustc the workspace's code or
dependencies do not build with.

### 7.2 AutoVerus

**Not attempted: the maintainer declined sending project code to an external
LLM API (2026-09-25).** It needs an OpenAI or Azure OpenAI API key and pins
its own Verus commit built from source (§1).

## 8. Re-verification and the final drift result

**Newest release.** No newer non-rolling release existed on 2026-11-15, so
V1 was re-verified on the newest rolling one, `0.2026.11.05.31e44e6`
(commit `31e44e600408242839ff456e84550e9f849344e3`, rustc 1.99.1, Z3
4.16.0). V2 and V3 have nothing to re-verify.

| Run | Result | Time | Peak RSS | Load start/end |
|---|---|---|---|---|
| first attempt, with `proofs/`'s `vstd = "=0.0.0-2026-11-01-0251"` | **build error**: `vstd` does not compile, 133 × "unrecognized verifier attribute" | 16.6 s | 808 MiB | 10.7 / 12.6 |
| V1 with the rolling release's own `vstd` (a git dependency at its commit) | **pass**: 6 verified, 0 errors | 26.9 s | 914 MiB | 11.5 / 11.2 |
| `v1-bound-div-to-mod` on the rolling release | **violation** on the named postcondition | 22.1 s | 950 MiB | 11.2 / 10.2 |

The first row is itself a finding: **`vstd` is tied to the Verus release
that built it.** A non-rolling release's `cargo verus new` pins a crates.io
`vstd` published the same week; a rolling one pins `vstd` by git revision
(`{ git = "https://github.com/verus-lang/verus.git", rev = … }`, a 66 MB
fetch). An upgrade therefore moves the Verus binaries, the `vstd`
dependency, and in an in-place adoption the production `Cargo.lock`, together
and every week. `verus_check` now rewrites the copy's `vstd` from a
per-release table (`VERUS_VSTD` in the script), and the classifier reported
the mismatch as `build-error(101)`, never as a refutation.

**Final drift result.** `VERUS_DRIFT_REV=cfee0c48` (`origin/main` on
2026-11-15, after the copy commit): **pass**, 9,036 table lines identical
and `clamped_preview_width`'s text identical. The queued changes merged
since `85758eaf` did not touch the copied code.

## 9. Metrics against Kani and against Quint/TLA+

| Target | Verus | Kani | Quint/TLA+ |
|---|---|---|---|
| V1 `clamped_preview_width` | 6 verified, 1.0 s, 28 MiB; known-false form refuted by a proved witness; mutant `/`→`%` caught on the named postcondition (1.1 s) | 5 proofs and 1 `should_panic`, under 0.2 s each (`core-geometry-policies`) | not evaluated |
| Annotation ratio, V1 | specification 18 lines (two spec functions, five `ensures` clauses), proof 17 lines, against 6 lines of code: about 6 : 1 | 79 lines of harness for the same function | — |
| Rewrite footprint, V1 | none: the body is production's, the specification is in the attribute, and the drift check compares the text | none | — |
| Trusted base, V1 | Verus, Z3, `vstd`'s specifications of `i32::max`/`min` | Kani, CBMC | — |
| `journal_core` decisions | 37 verified in place (15.7 s); the copy verified as executable code | every input, `journal_decisions_follow_their_documented_rules` (0.35 s) | — |
| Journal invariant, any number of actions, 2 windows | V2 not completed | inductive step not proved within budget after three rounds (31 conjuncts); diagnosis rounds of 103–839 s, peaks of 5.5–13.6 GiB; the base case passes and the invariant is satisfiable | TLAPS (E6) proved the T3 lock's at-most-one-writer unbounded in 0.4 s, and warned that S1 needs a large strengthening, which §4.1 confirms |
| Journal invariant, any number of ids and windows | V2 not completed | no compositional argument applies (§4.3) | — |
| L1 from every reachable state | V3 not attempted | k = 7 from bounded-prefix states | TLC (E9) held at 1 id under weak fairness, and exceeded the cap at 2 |

The rule's evidence bar for an unbounded proof ("fails on at least two real
pre-fix defects that a Kani harness also rejects") was not reached for any
Verus target, because V2 was not completed. V1 is a calibration target with
a bounded domain (`i32`), so its proof is complete in Kani as well.

## 10. Gate audit

Re-run of the Quint change's audit (§10 there) for the new paths, reading
each gate's scan roots.

| Gate | Sees `formal/evaluation/verus/`? | Sees this report? | Sees the script's new targets? |
|---|---|---|---|
| `make fmt`, `make check-fmt` | no: workspace members, plus `tests/widget` | no | no |
| Clippy, nextest, `cargo deny`, hakari | no: workspace members only, and both packages are in the root `exclude` | no | no |
| `scripts/check-filesystem-boundary.sh` | no (named `crates/` roots) | no | no |
| `check-temp-scope` | no (self-test only) | no | no |
| `check-agent-helpers` | no | no | **yes**: its self-test greps `scripts/` for `pgrep -f`/`pkill -f`; the script uses neither |
| `check-settings-writes`, `check-font-zoom-registration` | no (`crates/lushtext-core/src`, `crates/lushtext/src`, Blueprint) | no | no |
| `check-thresholds` | no (`crates/**/src`) | no | no |
| `check-responsiveness-lane`, `check-blueprint`, `check-automation-docs`, `automation-client-self-test` | no (their own scripts and files) | no | no |
| `check-kani-shards`, `check-widget-shards`, `check-gtk-axioms` | no (`crates/**`) | no | no |
| `check-flatpak-permissions`, `check-flatpak-ci-builds`, `check-end-user-smoke-workflow`, `check-workflow-timeouts` | no (the manifest, `.github/workflows`, the Makefile's smoke targets) | no | no |
| `check-explicit-simd` | no (`crates/**/*.rs` and `rust.md`) | no | no |
| `check-workflow-boundaries` | no (`crates/lushtext-core/src`, the matrix, `kani_proofs.rs`) | no | no |
| `check-accessibility-policy`, `check-visual-proof-policy` | no (UI prefixes) | no | no |
| `check-gtk-lush-policy`, `gtk-lush-adoption-matrix` | no; the first reads the root `Cargo.toml` and accepts the new `exclude` entries | no | no |
| terminology guard (`check-terminology`) | no | **yes** (`docs`); the report follows the folder-set terminology | no |
| `scripts/check-agent-docs.sh` | no | no | no |
| `sonar-project.properties` | no (`formal` is not in `sonar.sources`) | no | **yes** (`scripts` is in scope), like every repository script |
| `make mutants-*` (`.cargo/mutants.toml` `examine_globs`) | no (crates only) | no | no |

The design needed no adjustment. `make check` and `make check-policy` pass
on the final tree (§12).

## 11. Exploration log

Failures included, in order.

1. A probe package inside the repository was taken for a workspace member;
   an empty `[workspace]` table fixed it.
2. Constants and helpers outside `verus!` are external, so a verified body
   could not read them; they moved inside, with `allow_in_spec` for the
   associated helpers.
3. "return is not allowed in pure context" in `orphan_body_decision`; the
   `if` chain replaced the early returns.
4. Partial verification refused `cargo verus verify`; `cargo verus focus` is
   required, with `--lib` after `--manifest-path`.
5. Cargo skipped an unchanged crate, so a re-run verified nothing, which
   would have read as a pass. Neither touching the sources nor a fresh copy
   at the same path changed that; `verus_check` now appends a unique comment
   to the copy's `lib.rs`, and a Verus run without a
   `verification results::` line classifies as `error`, never as `pass`.
6. The `measure` subcommand died under `pipefail` when `ps` raced a process
   that had just exited (a pre-existing bug); fixed (`|| rss=0`).
7. Verus prints the resource stop in lower case ("resource limit (rlimit)
   exceeded"); the classifier matches it case-insensitively. A tiny global
   `--rlimit` still passed V1's trivial obligations, so the rlimit sample is
   a nonlinear lemma with `#[verifier::rlimit(0.1)]`.
8. `awk -v` consumed the regex escapes of the extraction pattern; the
   pattern is now passed through the environment.
9. The drift production build failed on a fresh dependency resolution; it
   now uses the compared commit's `Cargo.lock`.
10. The classifier was tested on one captured output of each kind: pass (V1),
    a caught mutant on the named obligation (`v1-bound-div-to-mod`, with
    `FORMAL_EVAL_OBLIGATION`), `rlimit` (`classify-rlimit`), and a build
    error (`classify-build-error`).
11. Kani: the first harness name was not found because the run started in
    the results directory; `--manifest-path` fixed it. `kani::cover!` needs
    literal messages, so the diagnosis harnesses list each pair explicitly.
12. Kani's 96-cover diagnosis hit the 7,266 MiB ceiling at the 8G cap and
    passed at 16G; the per-action split (16 harnesses) brought the peak down.
13. The private `RUSTUP_HOME` also gained a 1.5 GB `stable` toolchain at
    some point during the exploration (not traced to a script command); it
    was removed, and the re-runs of §8 and the final drift check did not
    bring it back.
14. Kani's inductive-step counterexamples pointed at a trace the maintained
    harness cannot reach (§4.2). Replaying it showed a model
    over-approximation, not a production defect; it is fixed in scratch only
    (F1).
15. V1's mutant was first confirmed in Kani on production, as the rule
    requires before it counts: with `/` → `%` in the bound,
    `preview_width_is_at_most_a_third_above_three_sp` fails
    (`3 * i64::from(width) <= counted(available)`, 14.5 s), and the scratch
    worktree's `policy.rs` was restored afterwards.
16. The rolling release refused the crates.io `vstd` of the non-rolling one
    (§8).

## 12. Reproduction

```sh
make formal-evaluation FORMAL_EVAL_TARGET=verus-install   # Verus, z3, Verus's rustc
make formal-evaluation FORMAL_EVAL_TARGET=verus-versions
make formal-evaluation FORMAL_EVAL_TARGET=verus-drift     # the table exchange at the copy commit
VERUS_DRIFT_REV=origin/main make formal-evaluation FORMAL_EVAL_TARGET=verus-drift
make formal-evaluation FORMAL_EVAL_TARGET=verus-v1        # V1 and its mutant
make formal-evaluation FORMAL_EVAL_TARGET=verus-classify  # one output of each kind
VERUS_RELEASE=0.2026.11.05.31e44e6 make formal-evaluation FORMAL_EVAL_TARGET=verus-install
VERUS_RELEASE=0.2026.11.05.31e44e6 make formal-evaluation FORMAL_EVAL_TARGET=verus-v1
make formal-evaluation FORMAL_EVAL_TARGET=report-data
```

The targets wrap each check in its own `scripts/heavy.sh` call, so they run
directly, never inside one `heavy.sh`. Without the tools, `verus-v1` exits 2
with the install message. `verus-v2` and `verus-v3` say that their modules do
not exist and verify nothing.

The Kani attempt and the in-place probe are patches against `85758eaf`:

```sh
git worktree add --detach build/formal-evaluation/scratch/kani-step 85758eaf
git -C build/formal-evaluation/scratch/kani-step apply "$PWD/formal/evaluation/verus/kani-step.patch"
# then, per harness, from the scratch worktree:
scripts/heavy.sh --mem 16G -- scripts/formal-evaluation.sh verus-measure NAME pass -- \
  cargo kani -p lushtext-core --target-dir build/formal-evaluation/kani-target \
  --harness services::draft_service::kani_proofs::verus_eval_inductive_step
git worktree remove build/formal-evaluation/scratch/kani-step
```

`in-place-probe.patch` applies the same way, and is verified with
`cargo verus focus -p lushtext-core --lib -- --verify-module
services::draft_service::journal_core` under Verus's toolchain
(`RUSTUP_HOME=build/formal-evaluation/tools/verus-rustup
RUSTUP_TOOLCHAIN=1.99.1`, with the Verus directory on `PATH`).

## 13. Decision

**Maintainer decision (2026-11-15): recommendation (a) is accepted.** Kani
stays the programme's only maintained tool. The evaluation is closed
**without V2** (tasks 4.1–4.4 and 4.5's metrics, and V3 and task 5.2 with
them, are closed unattempted) and **without a Kani follow-up**: option (a′)
was not chosen, so no change carries the inductive-step harness or F1 into
the lane. The revisit triggers below stand, and the round-3 patch, F1, and
the open pairs are in the programme record's deferral inventory. Nothing in
this record is a V2 or V3 result.

**Recommended, and accepted: (a) keep Kani alone.** Nothing here supports a
second tool:

- **No claim qualifies for (b).** Adoption needs a claim whose two Kani
  attempts both failed **and** that a trigger needs. For the journal, the
  inductive step did not close within budget and no compositional argument
  applies, so both attempts failed; but no trigger needs the unbounded
  claim, and V2, the measurement that would show Verus can carry it, was not
  completed. For V1, Kani's proof is already complete over every `i32`.
- **Not (a′) yet.** (a′) carries a proved inductive-step harness. The
  harness is not proved, so there is nothing to carry. The round-3 patch,
  F1, and the open pairs are recorded in the programme record's deferral
  inventory as the starting point if a trigger comes.
- **(c) is the revisit rule,** with these triggers: the two-window journal
  needing more than its lane's 7 actions, a third window or a second
  process kind, or GTK Lush publication needing a claim outside the per-bin
  domain. On any of them, first finish the Kani inductive step (a fourth
  round from the patch, about 14 minutes of solver time a round), and only
  if that fails, reopen Verus with V2.

**The losing options' evidence.**

- *For Verus:* V1 reproduced every known answer, the copy and drift
  approach worked and caught both deliberate changes, and the in-place probe
  verified the real `journal_core` decisions in 15.7 s with no change to
  their logic. The tool is usable on this codebase.
- *Against it:* a second rustc that never moves with the workspace's, a
  `vstd` tied to each weekly release (§8), a rewrite footprint that reaches
  into `model/draft.rs` and `cleanup_types.rs` (§7.1), eight new crates and
  regenerated hakari and Flatpak vendoring for an in-place adoption, and no
  measured evidence on the one claim that matters (V2).

**Fate of `formal/evaluation/verus/`:** it stays as disposable evidence,
like the Quint and TLA+ models beside it: outside every gate, in the root
`Cargo.toml` `exclude`, rerun only through the local-only
`make formal-evaluation`. Deleting it, the `verus-*` script targets, and the
two `exclude` entries is the rollback, and needs no other change.
