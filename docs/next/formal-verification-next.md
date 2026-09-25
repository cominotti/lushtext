# Formal Verification — Next Candidates (after the Kani consolidation)

Status: **candidate list**, written on 2026-09-23 when
`consolidate-formal-verification-on-kani` closed. The programme record
[`formal-verification.md`](./formal-verification.md) owns phases, results,
axioms, and deferrals. The earlier ranking lives in
[`formal-verification-evolution.md`](./formal-verification-evolution.md); all
of its K1–K8 items are done. Nothing here is committed work until an OpenSpec
change adopts it.

## 1. What the consolidation established

- **One tool, and it checks the shipped code.** 27 Kani harnesses cover:
  - the viewport-slice geometry and request decisions (on whole pixels);
  - the `ViewportSliceBin` feedback loop (1–3 bins, envelope taken from the
    GTK axiom ledger);
  - the draft-journal decision core (S1–S4, and liveness L1 with a tight
    bound of k = 7);
  - the I/O-free `WriteProtocol`, `MoveProtocol` and `RenameProtocol` cores
    (crash atomicity and classification soundness).

  The lane takes about 40 minutes and runs in five CI shards from one table.
  (Since then: 74 harnesses in seven shards after N1/N2, and 92 in ten after
  `extend-closed-loop-geometry-verification`.)
- **Kani found real defects, not only confirmed designs.**
  - Model checking the journal found a draft-loss path: a lazy restore whose
    file vanished released the autosave hold, and the next autosave or Discard
    destroyed an unseen body.
  - The follow-up audit found two more: a failed set-aside copy released the
    hold, and a draft opened from the Data page was never marked dirty.
  - All three are fixed, each behind a failing-first widget test.
- **Dropping an axiom is a cheap way to learn what it buys.** Adding a second
  process to the journal machine (K8) produced two concrete loss traces within
  eight actions. That is how we know exactly what the single-instance
  assumption protects.
- **Residuals can be recorded honestly.** Two slice-bin counterexamples are
  outside the envelope real consumers reach. They are pinned as
  `should_panic` harnesses, with reachability evidence and a model-checked
  candidate fix, instead of being fixed blind or hidden.

## 2. OpenSpec changes carrying these candidates (proposed 2026-09-23)

| Order | Change | Carries | Depends on |
|---|---|---|---|
| 1 | `harden-kani-lane-and-draft-token` (**implemented 2026-09-23**) | N1 (measure the shards on runners, geometry shard in the PR gate) and N5 (both fixture modules gated). **Done**; figures in the programme record, phase 2 | — |
| 2 | `extend-kani-to-pure-policies` (**implemented 2026-09-24**) | N2. `clamped_preview_width` keeps its floor, and the exception is stated in the spec. The sidebar-width `NaN` bug is fixed failing-first. **Done**; results, the second defect it found (an infinite native-slider height), and the three new shards are in the programme record, phase 2 | 1 |
| 3 | `verify-multi-window-draft-journal` (**implemented 2026-09-25**) | N4, first half: window actors against process actors in the journal machine. **Done**; the two-window harness, its counterexamples, and the per-application journal coordinator that fixes them are in the programme record, phase 4 | 1 |
| 4 | `extend-closed-loop-geometry-verification` (**implemented 2026-09-25**) | **Done**; results in the programme record, phase 3. N4, second half (two bins requesting in one frame), N3 (breakpoint loop, axioms A14–A18), and the two in-Kani attempts at unbounded claims: loop contracts and bin independence | 1 |
| 5 | `measure-proof-strength-with-mutation` | N9. The harness-file mutant exclusion is owned by change 1 | 1 |
| 6 | `bound-draft-set-aside-retention` | Fix first: the E1 set-aside naming defect (a body counts as kept only under a byte-identical copy; Kani K9). Then N10 set-aside retention: no automatic deletion, a soft bound that asks for review, and the 256-entry listing bug fixed | — |
| 7 | `add-sidebar-visual-proof-scenario` | N10 sidebar proof: the `reveal-workspace-path` action and the rendered-row pixel check | — |
| 8 | `apply-verification-altitude-redesigns` | N8, as six groups that can be applied one at a time | 1, 6 |
| 9 | `verify-shell-conformance-against-proven-cores` | N11: the imperative shells checked against the Kani-proven cores used as oracles | 1, 3 |
| — | `evaluate-quint-and-tlaplus-empirically` (**implemented 2026-09-24**) | An empirical Quint vs TLA+ comparison, with open exploration. Evaluation only. **Done**: [`formal-verification-quint-vs-tlaplus.md`](./formal-verification-quint-vs-tlaplus.md) decided Kani only for maintained properties, TLA+ with TLC for the disposable N6 sketch, and no Quint; its E1 run found a set-aside defect, fixed first in change 6 | — |

Only two candidates wait for a trigger, and neither has a proposal. N6
(inter-process lock) waits for axiom A6 to be breached. N7 (slice-bin
residuals) waits for a variable-height consumer.

Lean is re-discussed only if **both** in-Kani attempts in change 4 fail **and**
an unbounded claim is actually needed, by a recorded maintainer decision. Change
4 made both: the compositional attempt succeeded within stated bounds, so
Lean stays dormant.

## 3. Ranked candidates, in order of value

### N1. Measure the Kani lane on real runners, then promote a fast shard to the PR gate

**Done** by `harden-kani-lane-and-draft-token` (2026-09-23). Every shard is
measured on `ubuntu-latest` and fits its margins without a split or a bound
change; `widgets-geometry` runs on every pull request and push to `main`. The
per-shard table, the run ids, and both cache verdicts are in
[`formal-verification.md`](./formal-verification.md), phase 2. What follows is
the candidate as it was ranked.

Nobody has timed the shards on GitHub runners yet. `core-second-writer` and
`core-journal-and-write` peak around 9 GB, which is close to a standard
runner's memory.

1. Dispatch `kani.yml` and record wall time and peak memory for each shard.
2. Tune the bounds or split a shard if one is near a limit.
3. Consider caching `~/.kani` and the Kani target directory.
4. Promote `widgets-geometry` (about 4 minutes locally) to the pull-request
   gate, so the geometry proofs block regressions instead of trailing them
   weekly.

This is cheap, and it turns proofs from periodic evidence into a guard.

### N2. Extend Kani to the other pure geometry and budget policies

**Done (2026-09-24, `extend-kani-to-pure-policies`).** Results in the programme record, phase 2.

These were listed in phase 2 but were outside this change:

- the `editor_memory` budget and hysteresis;
- the fit functions in `minimap/policy.rs`;
- `window/geometry/policy.rs`;
- the width-preset clamps;
- `clamped_preview_width`, which is known to violate its "≤ 1/3" rule below
  3·MIN, so that rule needs to be either stated or fixed.

These functions have the same shape as the proven slice geometry (pure, and
arithmetic on whole pixels), so harness cost is low. The minimap and shell
geometry had their own history of visual bugs.

### N3. Model-check the adaptive-shell breakpoint loop

**Done** (`extend-closed-loop-geometry-verification`, 2026-09-25): the pure
`plan_shell_reconciliation`, a Kani step model citing A14–A18, and two
failing-first fixes (the properties layout setter flapped; the Open-button
breakpoint uncollapsed the workspace at large text scales). Programme record,
phase 3.

The breakpoint loop runs: allocated width → layout → breakpoint → allocated
width. It is the second closed feedback loop in the shell, and the programme
record already names it as the natural follow-on to K4. The pattern is the
K4 one: the real pure policy, Adwaita as a ledger-cited envelope, and three
properties to prove (fixed point, no flapping, requested visibility
preserved). It may need new ledger axioms for `AdwBreakpoint` and
`AdwOverlaySplitView` allocation behaviour, each pinned by a probe.

### N4. Close the two remaining single-process gaps inside one process

K8 is about two processes. Two narrower cases need no inter-process lock:

- **Multi-window.** **Done** (`verify-multi-window-draft-journal`): Kani
  found the per-window journal lane, per-window startup restore and cleanup,
  and per-window duplicate-path detection unsafe with two windows of one
  process; failing-first multi-window widget tests reproduced each, and one
  journal coordinator per application fixes them (proved). What follows was
  the candidate as ranked: cleanup gating is per window. Widget tests create
  several windows, and a future "new window" action would make this
  reachable. Add a second window to the journal machine, in the same process
  with a shared target guard, and fix whatever it finds.
- **Simultaneous requests from two bins.** **Done**
  (`extend-closed-loop-geometry-verification`): Kani confirmed the double
  count, a failing-first adoption-lab test reproduced it, and anchored
  forwarding fixed it (proved).

### N5. Make the draft-registration token airtight

**Done** by `harden-kani-lane-and-draft-token` (2026-09-23): both
`draft_service::fixture` and `filesystem::fixture` compile only under
`cfg(test)` or `test-utils`, `make check-filesystem-boundary` keeps them gated
and `test-utils` out of every shipping build, and CI's lint job checks the
shipped binary with default features. What follows is the candidate as it was
ranked.

`draft_service::fixture::write_body` bypasses `RegisteredDraft` and is
compiled into every build. Gate it behind a `test-utils` or bench feature, as
the rest of the test surface is. After that, "no body without an entry" is
enforced by the compiler everywhere, not only by convention.

### N6. An inter-process data-directory lock (only if A6 is ever breached)

This is the follow-up K8 named. Take a `flock` on a lock file in the data
directory, and give a second instance read-only mode or refuse to start it.
Keep it deferred unless someone reports two graphical sessions of one user
running LushText, or the product adds a supported multi-instance mode. The
day it lands, the K8 `should_panic` harness becomes a proof.

### N7. Fix the slice-bin residuals when a variable-height consumer arrives

The candidate fix for the two-reconfigure residual is already model-checked:
bound a settle by `shift + |held − published|` while a divergence is held. The
trigger is the first `ViewportSliceBin` adopter with variable-height rows.
Its first step is a failing real-GTK test, then the fix, and then the residual
harness becomes a proof. Until then there is nothing to do, which is correct.

### N8. The remaining altitude proposals from the two `/simplify` passes

Each is a redesign that this change left out on purpose:

- a common `Protocol` trait for the write, move and rename loops;
- one owner for the app-data directory layout;
- backoff for retries of the unrestored copy;
- passing `WriteLabel` into `temp_name::format`;
- separate types for registered and unregistered candidates;
- reusing the kill-point binary across smoke roles.

Take them up when a change touches that area anyway, not as a campaign.

### N9. Use proofs as mutation killers

Run `cargo-mutants` over the Kani-covered modules (`slice_geometry`,
`scroll_request`, `journal_core`, `write_protocol`) with the harnesses as the
test oracle. The result measures how much of each module the proofs pin: a
surviving mutant marks a behaviour no harness constrains. It also connects the
existing mutation lane to the new one.

### N10. Smaller hygiene items

- ~~Put a size bound or retention policy on `drafts/set-aside/`.~~ **Done
  2026-09-24 (`bound-draft-set-aside-retention`, step 8a):** a soft bound
  surfaces a review notice and `Preferences > Data` gained a summary row and a
  confirmed "Delete All Preserved Drafts…"; nothing is deleted automatically,
  and Kani R1–R4 check the bulk-deletion decision.
- Add a sidebar and slice-bin scenario to `cargo-gtk-proof`. It is blocked on
  a `reveal-workspace-path` automation action.
- Freshness precision for drafts (a nanosecond or file-identity token) needs
  a manifest format bump, so wait for one.

### N11. Check the shells against the proven cores

Kani proves the pure cores: `journal_core`, `WriteProtocol`, `MoveProtocol`,
`RenameProtocol`, and the slice-bin decisions. It does not prove the
imperative shells that drive them:

- the GTK drafts coordination (`ui/window/drafts/journal.rs`,
  `cleanup_journal.rs`);
- the durable-write shell loop running against the real filesystem;
- the seam between the two.

A model-based test closes the chain. Random operation sequences, including
"crash means stop and reload", drive the real `draft_service` and write
shell over a tempdir. The **proven core is the oracle**, so no parallel spec
is needed. Kani shows the core is safe, and conformance shows the shell obeys
the core.

This came out of evaluating Quint Connect: this project gets the same
spec-to-implementation conformance with no second language, because its
oracle is production code that is already proved.

The 2026-09-24 evaluation (E1) sharpened this. Driving the real
`draft_service` over a tempdir, against the Rust abstract disk model in
`formal/evaluation/stateright/src/env.rs` (which calls the real
`journal_core`), found a real set-aside defect that the proven core alone
could not show: the core correctly answered "preserve", and the service
effect was wrong. An oracle made only of the core's decisions would miss that
class of defect. N11 should adopt the abstract disk model as its oracle.

### Tool criteria recorded on 2026-09-23, measured on 2026-09-24

The 2026-09-23 judgements were opinions. The 2026-09-24 evaluation
([`formal-verification-quint-vs-tlaplus.md`](./formal-verification-quint-vs-tlaplus.md))
measured them on T1 (the durable write), T2 (the two-process journal), and T3
(an N6 lock sketch).

- **TLA+ (TLC): confirmed, as the disposable N6 design-sketch tool only.**
  - It checked every T3 liveness verdict in about 1 s and every safety
    verdict in 2–17 s, and distinguished weak from strong fairness.
  - TLAPS proved the lock's at-most-one-writer invariant unbounded in 0.4 s.
  - On T2 it was the only external route that reproduced K8 faster than Kani:
    about 252 s at 6 actions, but with 24 cores, about 12.3 GiB, and a
    hand-written `VIEW`. With one worker it did not finish in 30 minutes.
  - The final core stays Kani-checked Rust.
- **Quint: rejected, including as a Kani replacement.**
  - It matched TLA+ on T1 and T3 and read better (E7).
  - Its toolchain gave a silent false green: exit 0 with the latest Apalache.
  - Its simulator silently keeps unassigned variables.
  - Its TLC translation has no `VIEW` and timed out on T2 at 6 actions
    (131.6 M states). Its Apalache path ran out of heap while inlining T2.
  - Quint Connect worked, but the defect it surfaced came from the Rust
    abstract disk model it drove, which N11 can use without Quint.
- **Scale: `stateright` — confirmed, and it beat Kani.**
  - It ran on the real `journal_core`, with no second language.
  - It explored the one-process journal at 8 actions in 49.5 s / 1.1 GiB
    (Kani: 487 s / about 8 GB), and K8 at 6 in 73 s / 2.3 GiB (Kani:
    500.8 s / about 9 GB).
  - Adopting it as a maintained lane needs its own change.
- **Bend 2** (HigherOrderCO, September 2026): evaluated, and **not
  applicable**.
  - It verifies only Bend programs and cannot check Rust.
  - Its README says it is young and that its compiler is "99% AI-written and
    not fully audited".
  - The one transferable idea, `LAWS.bend` (declared laws that the compiler
    demands a proof of on every edit), is what Kani harnesses in the PR gate
    already provide.

### Dormant: Lean

Lean is re-discussed only if **both** in-Kani attempts at an unbounded claim
fail — Kani loop contracts, and a compositional argument (small harnesses plus
a written proof note) — **and** an unbounded claim is actually needed, for
example "any number of bins" as evidence if GTK Lush publication reopens
(`docs/next/gtk-lush.md`), and then only by a recorded maintainer decision.
For the slice loop, `extend-closed-loop-geometry-verification` made both
attempts: loop contracts failed on resources (programme record, phase 3), and
the bin-independence argument carried "any number of bins rests and does not
oscillate" within stated per-bin bounds. Lean therefore stays dormant.

## 4. Suggested order

```
N1 ✓ measured, geometry shard PR-gated ─► N2 ✓ more pure policies ─► N3 breakpoint loop
N5 ✓ token airtight ─► N4 ✓ multi-window + ✓ two-bin requests
N9 mutation × proofs (N1 is done, so any time now)
N11 after N4 (multi-window; N5 is done); N6, N7 only on their triggers; N8, N10 opportunistic
```

N1 and N5 landed together in `harden-kani-lane-and-draft-token`, and N2
landed in `extend-kani-to-pure-policies`, so the next step on this line is N3
(the adaptive-shell breakpoint loop, carried by
`extend-closed-loop-geometry-verification`), which builds on the shell-layout
harnesses N2 added in `ui/window/geometry/policy/kani_proofs.rs`.
