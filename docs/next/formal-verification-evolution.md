# Formal Verification — Evolution Candidates

Status: **candidate list**, written on 2026-09-23 when phase 0 of
[`formal-verification.md`](./formal-verification.md) closed. The programme
record owns phases, axioms, and deferrals. This file ranks the next moves and
records what the tool feasibility spike measured, so the ordering rests on
evidence instead of assumptions. Nothing here is committed work until an
OpenSpec change adopts it.

**Revised the same day.** The programme consolidated on **Kani as its only
formal tool** (see §2 of the programme record). The ranking below is the
Kani-only one. Candidates that assumed Quint or Lean were adapted, not
dropped.

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
| Lean 4 | 4.34.0 stable (elan) | about 1 min | micro-model of `atomic_replace` with two theorems plus a `Float` fact, checked in **0.37 s**. Superseded: it was ported to Kani (appendix B) |
| Kani, protocol port | same | none | the Lean micro-model ported to Rust (see appendix B). Both Lean theorems were reproduced, and a stronger third harness (any sequence of up to 6 steps, in any order) was proved. 3 harnesses in **1.2 s** |
| Quint | — | not installed | dropped by the consolidation |

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

## 3. Ranked candidates (Kani-only, in order of value)

Each candidate either adopts Kani or strengthens what Kani checks. The one
OpenSpec change that carries them all is `consolidate-formal-verification-on-kani`.

### K1. Land the Kani lane — highest value per hour

Scope:
- in-tree `#[cfg(kani)]` harnesses in `gtk-lush-widgets`, covering the seven
  spike harnesses over the real `slice_geometry.rs` and `scroll_request.rs`;
- `make kani`;
- an optional or scheduled CI job using `model-checking/kani-github-action`;
- `#[kani::should_panic]` for the harnesses that must find a counterexample.

Resolve the two domain gaps. The rustdoc promises "the slice lies inside the
content" and "the request lands exactly on the child value" hold on
whole-pixel inputs, not on arbitrary `f64`. State that domain in the rustdoc
and in `gtk-lush-viewport-slice`, and prove it.

### K2. GTK axiom ledger (phase 1)

This needs no new tool. It adds isolated widget probes for A5, A9, A11 and
A13, and records the phase-0 evidence for A8. The ledger defines the
adversarial envelope that K4 feeds to Kani, so it comes first.

### K3. Draft journal core, pure and Kani-checked (was C3 plus phase 4)

The `/simplify` altitude findings and the phase-4 model merge into one step:
extract the journal's decisions into a pure state machine that the service and
the GTK coordination both use. The work is:

- **one body-ownership check**, replacing the restore hold, the six preserve
  call sites, and the overwrite copy;
- "no body without an entry" enforced inside the service's write;
- insert-only commits routed through `update_manifest`;
- **one owner** for stale bodies.

Kani then checks S1–S4 and a bounded L1 over action sequences that include
`Crash`, with fixed-size state (up to 3 ids and 3 generations). This replaces
the Quint model and its differential harness, because the checked machine is
production code.

### K4. ViewportSliceBin closed loop in Kani (phase 3)

Write a Rust step model that calls the real `viewport_slice` and
`classify_child_scroll`. The child is `kani::any()` restricted by the ledger's
envelope, with N ∈ {1, 2, 3} bins and k allocations. Properties:

- fixed point at rest;
- no oscillation;
- request fidelity;
- bounded liveness;
- render fidelity.

The first target is the **learning-frame residual**.

### K5. Deterministic crash injection in the real-process smoke (was C6)

Add feature-gated kill points to the smoke build at the windows K3 and K6
name, such as body-written-before-commit and renamed-before-directory-sync.
This is the empirical counterpart of the Kani proofs: Kani shows the protocol
is safe, and injection shows the real process follows it.

### K6. durable_write I/O-free core (was C7 / phase 5, adapted)

Extract `WriteProtocol` (state plus event gives the next action) and keep a
thin `sys::` shell. The ported protocol harnesses (appendix B) then check the
**real** core instead of a model, including the copy fallback,
`rename_durable`, and mode non-widening. Crash atomicity becomes a complete
proof, because the protocol is finite.

### K7. Base cleanups (was C4 plus C5)

- **Split** `copy_durable`, which keeps the source, from `move_durable`, which
  removes it, and retire the misleading `copy_file_durable`. K6 builds on this.
- **One owner** for the temp-name format, and closed `WriteLabel`
  construction.
- **Extend the leftover sweep** to `style-schemes/`, `format-upgrade-backups/`
  and `drafts/set-aside/`.
- **A set-aside recovery surface** on the Preferences Data page, listing the
  preserved drafts with Open and Delete actions.

### K8. Drop axiom A6 in the K3 machine (was C8)

Add a second writer (window or process) to the Kani journal machine and see
what breaks. Only then decide whether the data directory needs an
inter-process lock.

### Opportunistic

- **Draft freshness precision (was C9).** Waits for a draft-manifest format
  bump.
- **GTK Lush publication evidence (was C10).** Would come from K1 and K4.

### Retired by the consolidation

- **Lean phase 5.** Its content moved into K6.
- **The Quint draft model.** It became K3.
- **The Lean/Rust differential corpus and Aeneas extraction.** They are no
  longer needed, because the checked code is the production code.

## 4. Suggested order

```
K1 Kani lane ─► K2 axiom ledger ─► K4 closed-loop in Kani
K7 base cleanups ─► K6 durable_write core ─► (K5 kill points at K6 windows)
K3 journal core ─► (K5 kill points at K3 windows) ─► K8 drop axiom A6
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

## Appendix B — The Lean spike, ported to Kani

The Lean micro-model of `atomic_replace` was ported line for line:
`inductive` became `enum`, `structure` became `struct`, and each `theorem`
became a `#[kani::proof]`. Kani checked all three harnesses in 1.2 s.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Content { OldBytes, NewBytes, Torn }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Step { WriteTemp, SyncTemp, Rename, SyncDir }

#[derive(Clone, Copy, Debug)]
pub struct Disk { pub target: Content, pub durable_target: Content,
                  pub temp_data: Option<Content>, pub temp_durable: bool }

pub const INIT: Disk = Disk { target: Content::OldBytes, durable_target: Content::OldBytes,
                              temp_data: None, temp_durable: false };

pub fn step(d: Disk, s: Step) -> Disk {
    match s {
        Step::WriteTemp => Disk { temp_data: Some(Content::NewBytes), temp_durable: false, ..d },
        Step::SyncTemp => Disk { temp_durable: true, ..d },
        Step::Rename => Disk { target: Content::NewBytes, temp_data: None,
            durable_target: if d.temp_durable { d.durable_target } else { Content::Torn }, ..d },
        Step::SyncDir => Disk { durable_target: d.target, ..d },
    }
}

pub fn crash(d: Disk) -> Content { d.durable_target }
pub const PROTOCOL: [Step; 4] = [Step::WriteTemp, Step::SyncTemp, Step::Rename, Step::SyncDir];

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof] #[kani::unwind(5)] // PROVED (Lean: crash_atomic)
    fn crash_atomic() {
        let n: usize = kani::any();
        kani::assume(n <= PROTOCOL.len());
        let mut d = INIT;
        for s in &PROTOCOL[..n] { d = step(d, *s); }
        assert!(crash(d) != Content::Torn);
    }

    #[kani::proof] // COUNTEREXAMPLE, as intended (Lean: skipping_fsync_is_unsafe);
                   // in-tree it becomes #[kani::should_panic]
    fn skipping_fsync_is_unsafe() {
        let d = step(step(INIT, Step::WriteTemp), Step::Rename);
        assert!(crash(d) != Content::Torn);
    }

    #[kani::proof] #[kani::unwind(7)] // PROVED — stronger than the Lean spike
    fn torn_only_by_unsynced_rename() {
        let mut d = INIT;
        let mut unsynced_rename = false;
        for _ in 0..6 {
            let s: Step = kani::any();
            if s == Step::Rename && !d.temp_durable { unsynced_rename = true; }
            d = step(d, s);
        }
        assert!(crash(d) != Content::Torn || unsynced_rename);
    }
}
```

The third harness takes **any** sequence of up to 6 steps, in any order and
with repetitions. It proves that a torn target arises only from a rename of an
unsynced temp: the code's ordering rule is exactly the safety condition. The
Lean spike only enumerated prefixes of the correct order.

This is still a model of `durable_write.rs`, not the file itself. K6 closes
that gap: once the I/O-free core exists, these harnesses import the real
`WriteProtocol` instead of `step`.

The original Lean 4.34 spike took 0.37 s with core tactics and no Mathlib. It
is superseded; its source is in git history (commit `b2298918`).
