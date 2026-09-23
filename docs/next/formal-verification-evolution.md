# Formal Verification — Evolution Candidates

Status: **candidate list**, written on 2026-09-23 when phase 0 of
[`formal-verification.md`](./formal-verification.md) closed. The programme
record owns phases, axioms, and deferrals. This file ranks the next moves and
records what the tool feasibility spike measured, so the ordering rests on
evidence instead of assumptions. Nothing here is committed work until an
OpenSpec change adopts it.

## 1. What phase 0 taught

- **Writing a protocol down as a state machine is the cheapest verification
  step, and the most productive one so far.** No model or proof existed yet
  when the reading surfaced six defects. The follow-up data-safety audit then
  found one more pre-existing bug (autosave overwriting a body whose lazy
  restore was still pending) and one HIGH (Save or Discard during a pending
  restore deleted unseen edits).
- **An accepted trade-off can turn out to be a real loss path.** The design
  accepted "stale drafts in local history obey retention". During
  implementation it turned out that retention prunes a back-dated snapshot
  first, so a set-aside copy that is never pruned became mandatory. Future
  models should include retention and pruning as adversarial actions.
- **Names that lie are defects waiting to happen.** `copy_file_durable`
  removes its source. A "copy" that was really a move shipped inside phase 0
  and was caught only by the audit.
- **Environment axioms bite even outside GTK.** Inside a Flatpak, a pid
  repeats across launches. A pid alone is not a process identity, so leftover
  temp names now carry a launch nonce.
- **The slice-bin probe settled axiom A8 empirically.** Across 48
  instrumented widget tests, real consumers produced 39 requests, 0 settles,
  and no `Defer`. Only a synthetic `GtkScrollable` reached the swallowed-request
  case.

## 2. Tool feasibility spike (measured 2026-09-23, this toolbox)

| Tool | Version | Setup | Result |
|---|---|---|---|
| Kani | 0.68.0 (CBMC 6.11.0) | `cargo install --locked kani-verifier` 2.4 s, then `cargo kani setup` 13 s | 7 harnesses over the **real** `slice_geometry.rs` and `scroll_request.rs` (see appendix A): 5 proved, 2 counterexamples; each harness took 0.1–28 s |
| Kani, in-tree | same | none | `cargo kani -p gtk-lush-widgets --only-codegen` compiled the crate and its whole GTK dependency tree in **31 s**, next to the workspace's 1.96 toolchain pin |
| Lean 4 | 4.34.0 stable (elan) | about 1 min | micro-model of `atomic_replace` with two theorems plus a `Float` fact (see appendix B) checked in **0.37 s** |
| Quint | — | not installed | not spiked. It needs a Node or npm toolchain, so evaluate it inside phase 4 |

The two Kani counterexamples matter more than the proofs:

1. **Rustdoc promise "the slice always lies inside the content".** It is false
   for finite `f64` near 10²⁶⁰, where rounding makes `top + height` exceed
   `content`. It is **proved** for every `i32` pixel input with overscan up to
   4096. The promise is true on the domain GTK actually produces, and the
   documentation should say so.
2. **Exact landing, `viewport_top + delta == child_value`**, which axiom A9
   needs because GTK compares adjustment values exactly. It fails for general
   `f64`, and is **proved** for `i32` pixel inputs with a `u16` shift. This is
   the same shape: correct on integer allocations, unstated elsewhere.

Proved outright: no panic for any `f64` (NaN, ∞, negatives); a resting bin
never requests a scroll (the v0.7.0 regression as a theorem); every forwarded
request is at least `ADJUSTMENT_EPSILON`.

## 3. Ranked candidates

### C1. Land the Kani lane now (was phase 2) — highest value per hour

The spike removed every adoption risk the plan listed: toolchain coexistence,
GTK crates compiling, and run time. Two real specification gaps came out in
under a minute.

Scope:
- in-tree `#[cfg(kani)]` harnesses in `gtk-lush-widgets`;
- a `make kani` target;
- an optional CI job using `model-checking/kani-github-action`.

Resolve the two counterexamples by stating each function's domain in its
rustdoc and spec (the `gtk-lush-viewport-slice` "pure, property-tested policy"
requirement), or by clamping inputs to the whole-pixel range. Then extend to
the pure-geometry candidates the phase-0 reading listed: `minimap/policy.rs`
fit functions, `window/geometry/policy.rs`, width presets, and
`clamped_preview_width`, which is known to violate its "≤ 1/3" rule below
3·MIN.

### C2. GTK axiom ledger (phase 1), then the closed-loop model (phase 3)

These stay in order, because the model is only as good as its envelope. The
ledger must add isolated probes for A5, A9, A11, and A13, and record A8's
phase-0 evidence.

The phase-3 model now has a concrete first target: the **learning-frame
residual**. The first allocation after inset learning still erases a
same-frame request, and phase 0 could not decide whether that case is
reachable.

### C3. Close the draft journal's invariants inside the service, before modelling it (pre-phase 4)

The `/simplify` altitude review proposed redesigns that were out of scope for
phase 0. Each one removes state the phase-4 Quint model would otherwise have to
carry:

- enforce "no body without an entry" inside `draft_service`'s write, not in
  its callers;
- let insert-only commits pass through `update_manifest` instead of a separate
  additive path;
- replace the restore-hold counter, the six preserve call sites, and the
  overwrite copy with **one body-ownership check**;
- give stale bodies **one owner** (set-aside, with local history as a view)
  instead of two parallel stores.

Do this first. The model then shrinks from about 18 actions toward about 12,
and each invariant becomes a property of one function.

### C4. Split the durable copy and move primitives

Provide `copy_durable`, which keeps its source, and `move_durable`, which
removes it, and retire `copy_file_durable`. This closes the naming trap behind
one phase-0 defect, and gives the phase-5 Lean model primitives whose names
match their semantics. It also belongs with the altitude items below:

- **one owner** for the temp-name format, which today is shared by the
  builder and the leftover predicate;
- **closed** `WriteLabel` construction, so the predicate's known-tag set cannot
  drift.

### C5. Leftover sweep coverage and a set-aside recovery surface

The startup sweep does not cover `style-schemes/`, `format-upgrade-backups/`,
or `drafts/set-aside/`, and the set-aside area has no size bound and no UI. A
Preferences > Data row that lists set-aside drafts with Open and Delete
actions would turn a hidden directory into a user-owned recovery surface. It
would also resolve the "no size bound" deferral by making the user the
decision-maker.

### C6. Deterministic crash injection in the real-process smoke

`make crash-recovery-smoke` could not kill inside the body-write-to-commit
window, because the smoke binary has no delay hooks. Two routes:

- **feature-gated delay hooks** in the smoke build, so that a kill point
  inside any protocol window becomes deterministic;
- **LazyFS or dm-flakey** power-loss simulation on top of that.

Crash injection is the empirical counterpart of phases 4 and 5: the models say
which windows exist, and injection proves the real process survives them.

### C7. Grow the Lean spike into phase 5

The micro-model in appendix B already proves crash atomicity for every prefix
of the protocol. It also proves that dropping the temp `fsync` is unsafe,
which is the regression the ordering exists to prevent.

Next steps:
- add the copy fallback and `rename_durable`;
- add mode non-widening;
- replace the prefix enumeration with an inductive proof over arbitrary crash
  points;
- keep the model in-repo as a Lake project with an elan-pinned toolchain.

The phase-0 NFS and inode-ABA deferrals become explicit axioms that the proofs
name.

### C8. Multi-window and multi-process safety — the largest unmodelled axiom

Everything above assumes one process and one window per data directory
(axiom A6). Production reuses the active window, but widget tests create
several, and a second instance is one `flatpak run` away. In the phase-4 model
this is the first axiom worth *dropping*, to find out what breaks, before
deciding whether an inter-process lock on the data directory is warranted.

### C9. Draft freshness precision

`original_mtime_secs` has second granularity. Phase 0 made false positives
non-destructive, but every one still hides a restore behind a detour. A
nanosecond or file-identity freshness token would need a draft-manifest format
change, so it waits until a format bump is justified on other grounds.

### C10. GTK Lush publication evidence (dormant track)

If publication reopens (`docs/next/gtk-lush.md`), machine-checked invariants
from C1–C2 are a differentiator no comparable GTK helper crate offers. Record
them in the crate README as verified properties with their stated domains.

## 4. Suggested order

```
C1 Kani lane ─┬─► C2 axiom ledger ─► phase 3 closed-loop model
              │
C4 primitives ┴─► C7 Lean phase 5
C3 journal invariants ─► phase 4 Quint model ─► C8 drop axiom A6
C5, C6 in parallel whenever convenient; C9 and C10 are opportunistic
```

## Appendix A — Kani spike harness

The scratch crate includes the real source files with `#[path]`, so what Kani
checks is exactly what ships:

```rust
#[path = "<repo>/crates/gtk-lush/widgets/src/slice_geometry.rs"] pub mod slice_geometry;
#[path = "<repo>/crates/gtk-lush/widgets/src/scroll_request.rs"] pub mod scroll_request;

#[cfg(kani)]
mod proofs {
    use super::{scroll_request::*, slice_geometry::*};

    #[kani::proof] // PROVED
    fn slice_never_panics() {
        let _ = viewport_slice(kani::any(), kani::any(), kani::any(), kani::any());
    }

    #[kani::proof] // COUNTEREXAMPLE near 1e260; PROVED on i32 pixels (28 s)
    fn slice_lies_inside_content() {
        let (t, h, c, o): (f64, f64, f64, f64) = (kani::any(), kani::any(), kani::any(), kani::any());
        kani::assume(t.is_finite() && h.is_finite() && c.is_finite() && o.is_finite());
        let s = viewport_slice(t, h, c, o);
        assert!(s.top >= 0.0 && s.height >= 0.0 && s.top + s.height <= c.max(0.0));
    }

    #[kani::proof] // PROVED
    fn resting_bin_never_requests() {
        let (p, v, s): (f64, f64, f64) = (kani::any(), kani::any(), kani::any());
        assert!(outer_scroll_request(p, p, v, s).is_none());
    }

    #[kani::proof] // PROVED
    fn requests_exceed_epsilon() {
        let (p, c, v, s): (f64, f64, f64, f64) = (kani::any(), kani::any(), kani::any(), kani::any());
        if let Some(d) = outer_scroll_request(p, c, v, s) { assert!(d.abs() >= ADJUSTMENT_EPSILON); }
    }

    #[kani::proof] // COUNTEREXAMPLE on general f64; PROVED on i32 pixels + u16 shift (4.9 s)
    fn request_lands_exactly_on_child_value() {
        let (p, c, v, s): (f64, f64, f64, f64) = (kani::any(), kani::any(), kani::any(), kani::any());
        if let Some(d) = outer_scroll_request(p, c, v, s) { assert!(v + d == c); }
    }
}
```

## Appendix B — Lean spike micro-model (Lean 4.34.0)

```lean
inductive Content | oldBytes | newBytes | torn deriving DecidableEq, Repr
inductive Step | writeTemp | syncTemp | rename | syncDir deriving DecidableEq, Repr

structure Disk where
  target : Content
  durableTarget : Content
  tempData : Option Content
  tempDurable : Bool
  deriving Repr

def init : Disk := ⟨.oldBytes, .oldBytes, none, false⟩

def step (d : Disk) : Step → Disk
  | .writeTemp => { d with tempData := some .newBytes, tempDurable := false }
  | .syncTemp  => { d with tempDurable := true }
  | .rename    => { d with target := .newBytes, tempData := none,
                           durableTarget := if d.tempDurable then d.durableTarget else .torn }
  | .syncDir   => { d with durableTarget := d.target }

def crash (d : Disk) : Content := d.durableTarget

def run (d : Disk) : List Step → Disk
  | [] => d
  | s :: ss => run (step d s) ss

def protocol : List Step := [.writeTemp, .syncTemp, .rename, .syncDir]

-- A crash after any prefix of the protocol never exposes a torn target.
theorem crash_atomic :
    (List.range (protocol.length + 1)).all
      (fun n => crash (run init (protocol.take n)) != Content.torn) = true := by decide

-- Dropping the temp fsync is caught.
theorem skipping_fsync_is_unsafe :
    crash (run init [.writeTemp, .rename]) = Content.torn := by decide

-- Float is IEEE-754 binary64 in the logic.
example : (0.1 + 0.2 : Float) ≠ 0.3 := by native_decide
```

This is a toy: it folds axioms A2 and A3 into one `durableTarget` field and
enumerates prefixes rather than proving over arbitrary crash points. Its value
is the measurement, not the theorem. Lean 4.34 with core tactics alone
(no Mathlib) checks a finite protocol model interactively.
