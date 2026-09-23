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

## 2. Ranked candidates, in order of value

### N1. Measure the Kani lane on real runners, then promote a fast shard to the PR gate

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

The breakpoint loop runs: allocated width → layout → breakpoint → allocated
width. It is the second closed feedback loop in the shell, and the programme
record already names it as the natural follow-on to K4. The pattern is the
K4 one: the real pure policy, Adwaita as a ledger-cited envelope, and three
properties to prove (fixed point, no flapping, requested visibility
preserved). It may need new ledger axioms for `AdwBreakpoint` and
`AdwOverlaySplitView` allocation behaviour, each pinned by a probe.

### N4. Close the two remaining single-process gaps inside one process

K8 is about two processes. Two narrower cases need no inter-process lock:

- **Multi-window.** Cleanup gating is per window. Widget tests create several
  windows, and a future "new window" action would make this reachable. Add a
  second window to the journal machine, in the same process with a shared
  target guard, and fix whatever it finds.
- **Simultaneous requests from two bins.** Every bin computes its request
  against the same pre-move outer value, so two requests in one frame add up.
  The K4 model can show this directly.

### N5. Make the draft-registration token airtight

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

- Put a size bound or retention policy on `drafts/set-aside/`. It is never
  pruned today, and the Data page is the user's only control.
- Add a sidebar and slice-bin scenario to `cargo-gtk-proof`. It is blocked on
  a `reveal-workspace-path` automation action.
- Freshness precision for drafts (a nanosecond or file-identity token) needs
  a manifest format bump, so wait for one.

### Dormant: Lean

Lean returns only if a claim genuinely needs to be unbounded. The likeliest
case is "any number of bins" as evidence if GTK Lush publication reopens
(`docs/next/gtk-lush.md`).

## 3. Suggested order

```
N1 measure + PR-gate a shard ─► N2 more pure policies ─► N3 breakpoint loop
N5 token airtight (small) ─► N4 multi-window + two-bin requests
N9 mutation × proofs (any time after N1)
N6, N7 only on their triggers; N8, N10 opportunistic
```
