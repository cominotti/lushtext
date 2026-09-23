# Formal Verification — Evolution Candidates

Status: **done**. Written on 2026-09-23 when phase 0 of
[`formal-verification.md`](./formal-verification.md) closed, to rank the next
moves on the evidence of a measured tool spike. Every candidate K1–K8 was
adopted and completed by the OpenSpec change
`consolidate-formal-verification-on-kani`; the programme record owns the
results, phases, axioms, and deferrals, and this file is kept as the ranking's
rationale. The post-consolidation ranking lives in
[`formal-verification-next.md`](./formal-verification-next.md).

The programme consolidated on **Kani as its only formal tool** (see §2 of the
programme record). The ranking below is the Kani-only one.

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
| Kani, protocol micro-model | same | none | a micro-model of `atomic_replace` (see appendix B): crash atomicity, the skipped-fsync counterexample, and a stronger any-order harness over up to 6 steps. 3 harnesses in **1.2 s** |

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

**Done:** `make kani`, the sharded `.github/workflows/kani.yml`, and the geometry harnesses; results in phase 2 of the programme record.


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

**Done:** the ledger and the A5, A9, A11, A13 probes; phase 1 of the programme record.


This needs no new tool. It adds isolated widget probes for A5, A9, A11 and
A13, and records the phase-0 evidence for A8. The ledger defines the
adversarial envelope that K4 feeds to Kani, so it comes first.

### K3. Draft journal core, pure and Kani-checked (was C3 plus phase 4)

**Done:** `services/draft_service/journal_core.rs` and its harness; phase 4 of the programme record.


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
a separately written model and its differential harness, because the checked
machine is production code.

### K4. ViewportSliceBin closed loop in Kani (phase 3)

**Done:** the `slice_loop` model; both residuals decided and recorded; phase 3 of the programme record.


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

**Done:** three kill points, all passing in `make crash-recovery-smoke`; K5 in the programme record.


Add feature-gated kill points to the smoke build at the windows K3 and K6
name, such as body-written-before-commit and renamed-before-directory-sync.
This is the empirical counterpart of the Kani proofs: Kani shows the protocol
is safe, and injection shows the real process follows it.

### K6. durable_write I/O-free core (was C7 / phase 5, adapted)

**Done:** `services/filesystem/write_protocol.rs` and its harnesses; phase 5 of the programme record.


Extract `WriteProtocol` (state plus event gives the next action) and keep a
thin `sys::` shell. The ported protocol harnesses (appendix B) then check the
**real** core instead of a model, including the copy fallback,
`rename_durable`, and mode non-widening. Crash atomicity becomes a complete
proof, because the protocol is finite.

### K7. Base cleanups (was C4 plus C5)

**Done:** all four items.


- **Split** `copy_durable`, which keeps the source, from `move_durable`, which
  removes it, and retire the misleading `copy_file_durable`. K6 builds on this.
- **One owner** for the temp-name format, and closed `WriteLabel`
  construction.
- **Extend the leftover sweep** to `style-schemes/`, `format-upgrade-backups/`
  and `drafts/set-aside/`.
- **A set-aside recovery surface** on the Preferences Data page, listing the
  preserved drafts with Open and Delete actions.

### K8. Drop axiom A6 in the K3 machine (was C8)

**Done:** see K8 in the programme record.


Add a second writer (window or process) to the Kani journal machine and see
what breaks. Only then decide whether the data directory needs an
inter-process lock.

### Opportunistic

- **Draft freshness precision (was C9).** Waits for a draft-manifest format
  bump.
- **GTK Lush publication evidence (was C10).** Would come from K1 and K4.

### Retired by the consolidation

Candidates that checked a separately written model — of the draft journal, of
the durable write, or a differential corpus between a model and the code — are
retired: K3 and K6 check the production code instead.

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

## Appendix B — The protocol micro-model (superseded)

The spike's second Kani result was a hand-written micro-model of
`atomic_replace` (a four-step `enum` and a volatile/durable disk), which proved
crash atomicity, found the skipped-fsync counterexample, and proved that a torn
target arises only from renaming an unsynced temp, over any order of up to six
steps. K6 superseded it: the same properties are now proved over the
**production** `WriteProtocol` core in
`crates/lushtext-core/src/services/filesystem/write_protocol/kani_proofs.rs`,
and the micro-model's source survives only in git history (commit
`5398acde`).
