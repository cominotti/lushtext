## Context

K4 (`consolidate-formal-verification-on-kani`) models the `ViewportSliceBin`
loop in `crates/gtk-lush/widgets/src/kani_proofs.rs`, in `mod slice_loop`.
Each allocation calls the real `viewport_slice`, `whole_pixel_band` and
`classify_child_scroll`, and mirrors the bin's publish, learn-inset,
write-back, defer and request bookkeeping. The child is `kani::any()`,
restricted by `kani::assume` clauses that cite ledger axioms A4–A13. The model
proves rest for N ≤ 3 and single-request fidelity for N ≤ 2, and keeps two
`should_panic` residuals.

Three things K4 left open, per the programme record (phase 3 "Still open") and
candidates N3/N4 in `docs/next/formal-verification-next.md`:

1. **Two bins requesting in one frame.** Two code paths matter.
   - `imp.rs::size_allocate` computes `delta = child_value − viewport_top`,
     where `viewport_top` comes from `compute_point` at allocation time, so
     every bin in the frame measures against the same pre-move outer value.
   - `follow_child_request` accumulates the delta and schedules an idle that
     runs `adjustment.set_value(adjustment.value() + delta)`.

   GLib runs same-priority idles in FIFO order. The second bin's idle therefore
   adds its delta on top of the first bin's move, and the outer lands at
   `outer₀ + d_A + d_B`, where neither request is honoured. The model's
   `run_idles` already mirrors this, but no harness issues two requests in one
   frame. Real triggers exist. `scan_execution.rs` calls
   `select_and_scroll_to` from a per-section timeout after a scan, so two
   sections refreshed in the same turn (a workspace refresh, a rename or
   create in overlapping folders) can each `scroll_to`. Any GTK Lush consumer
   can also call `scroll_to` on two hosted lists in one turn.
2. **The adaptive-shell breakpoint loop.**
   - `derive_adaptive_shell_layout` (in `ui/window/geometry/policy.rs`) maps
     `AdaptiveShellInputs` to a threshold `T(w)` and a presentation
     (`Sheet` iff `w ≤ T(w)`).
   - `execution.rs::sync_split_view_widths` runs from `size_allocate` when the
     width changes. Unless a sidebar transition settle is pending, it calls
     `sync_properties_breakpoint` (`set_condition` only when the integer
     threshold changes), `sync_properties_split_view` and
     `sync_secondary_surfaces` (layout name, compact slot, `show-sidebar`,
     sheet `open`).
   - Independently, the installed `AdwBreakpoint` sets `layout-name = "sheet"`
     when its own `max-width: Tsp` condition holds.

   Two writers of `layout-name` with possibly different predicates are exactly
   the shape that flaps. One predicate compares pixels with an `sp` threshold;
   the other is Adwaita's own `sp` evaluation, which scales with the text scale
   factor. Nothing models the interaction.
3. **Everything is bounded.** "Dormant: Lean" names unbounded claims (any N)
   as Lean's only reason to return. Kani 0.68 offers experimental loop
   contracts: `#[kani::loop_invariant(..)]`, `#[kani::loop_decreases(..)]`,
   `on_entry`/`prev`, `-Z loop-contracts`, with support for `while`, `loop`
   and `for` loops over ranges and arrays. Loop contracts turn a loop into an
   inductive step. Nobody has tried them here, nor a compositional argument.

Constraints:

- Kani is the only formal tool (`formal-verification-kani`).
- Harnesses live behind `cfg(kani)` beside GTK-free checked code.
- Envelopes cite the ledger.
- Every CI shard stays under 30 minutes.
- The pre-existing-blockers rule applies.
- The `WFR-SHELL-GEOMETRY` role home declares every file.
- GTK Lush widget changes carry governance (CHANGELOG, policy gates).
- The memory from the sidebar realized-cap work: replacing within-bin
  accumulation with a "latest requested offset" regressed keyboard traversal
  under the headless frame clock. Within-bin accumulation must stay.

## Goals / Non-Goals

**Goals:**

- Decide the two-bin double count in the model. If the model finds it, fix it
  failing-first in real GTK, with the fix model-checked before it is written in
  `imp.rs`.
- Give the breakpoint loop the K4 treatment: real policy, a real
  reconciliation decision, a ledger-cited Adwaita envelope, and proofs of fixed
  point, no flapping, preserved intent, agreement and no allocation-path
  persistence.
- Pin every new Adwaita axiom with an isolated probe.
- Make both unbounded attempts and record their outcomes honestly. Rewrite the
  Lean gate so it depends on them.
- Keep the lane sharded within the CI caps.

**Non-Goals:**

- Fixing the two recorded slice-bin residuals (N7). Their trigger is still a
  variable-height consumer.
- Kani coverage of the other pure policies in N2 (editor memory, minimap fit,
  `clamped_preview_width`), apart from the parts of the geometry policy the
  loop calls.
- The multi-window journal half of N4.
- Promoting a shard to the PR gate (N1).
- Any Lean or other-tool work.
- Visual smoke of the sidebar. That is the separate
  `add-sidebar-visual-proof-scenario` change.

## Decisions

### D1. Two-bin harness first; expected counterexample; semantics "last applied wins"

Add `slice_loop_two_simultaneous_requests_do_not_add_up` for N=2:

1. Rest.
2. Choose targets for both bins under A5 (`page > 0`), each at least epsilon
   from its child.
3. Mark both for allocation and run one frame in which both reactions request.
4. Run two resting frames.

The property: the outer equals the anchor-relative target of the bin whose
idle ran last (FIFO means the bin allocated last in the frame), or is clamped.
Also, the loop is at rest and the child holds its band offset.

The expected result on current code is a counterexample, which is kept first
as a `should_panic` pin.

Why "last wins", and not "both visible" or "first wins":

- Two requests may conflict (rows further apart than a viewport), so "both"
  is not always satisfiable.
- "First wins" would need the later bin to know about the earlier one.
- "Last wins" is what GTK itself does for successive `scroll_to` calls on one
  list.
- It falls out of anchoring without any cross-bin state.

*Alternative considered:* model "honour at least one" as a disjunction. That
is rejected because it is weaker than what the fix delivers, and it would
leave the choice unspecified.

### D2. The fix: anchor each bin's accumulated delta to the outer value it was measured against

In `follow_child_request`, when a bin's pending batch is empty, record
`request_anchor = outer.vadjustment().value()` (the value its `viewport_top`
derived from). Keep accumulating `pending_outer_delta` exactly as today. The
idle then calls `set_value(anchor + pending_delta)` instead of
`set_value(value() + pending_delta)`.

- When nothing else moved the outer between the request and the idle, the
  two are identical. Single-bin traversal therefore keeps its current
  semantics, which avoids the regression the realized-cap work recorded.
- When another bin's idle already moved the outer from the same anchor, the
  second set is absolute, so the last one wins and nothing adds up.

`child_adjustment_moved` (requests made outside allocation) records its anchor
the same way.

The model is changed **first**. `run_idles` gains a per-bin anchor, and the
two-bin harness flips from `should_panic` to a proof. All existing slice-loop
harnesses must stay proved, and the rest and request harnesses re-run at
N = 1, 2, 3. The fix is then written in `imp.rs` against a real-GTK test that
has already failed.

*Alternatives considered:*

- **A shared per-outer-scroller coordinator** (qdata on the outer adjustment
  holding one pending target). It is correct, but it adds cross-widget state
  and a lifetime to manage. It is kept as the fallback if anchoring fails in
  the model.
- **Dropping any request while another bin's idle is pending.** It loses
  requests silently, which violates the existing "never silently dropped"
  requirement.
- **Replacing accumulation with the latest target.** Already known to regress
  traversal.

One open edge is recorded in Risks: a user scroll between a request and its
idle is now overridden rather than added to.

### D3. Real-GTK reproduction before the fix

The primary fixture extends `gtk_lush_adoption::test_adoption_two_slice_bins_in_one_scroller_do_not_oscillate`:

- two `ViewportSliceBin`s with `GtkListView` children in one outer
  `GtkScrolledWindow`, at rest;
- then, in one main-loop turn, `scroll_to(row_a, FOCUS)` on list A and
  `scroll_to(row_b, FOCUS)` on list B;
- then wait for rest.

It asserts that the outer lands where B's request is honoured (B's row is
fully inside the outer viewport, with its rendered bounds read through
`mapped_list_rows`), or is clamped. It also asserts that the outer is not at
`outer₀ + d_A + d_B`, and that both bins' allocation and correction counts
stop growing.

A second test does the same in LushText: two workspace sections in
`workspace_tree_virtualization.rs`, triggering `select_and_scroll_to` on both
in one turn through the existing test-utils surface, or through the real path
(a refresh completing in both sections). If no such path exists without a new
seam, the task records why, and the adoption-lab fixture stands as the
reproduction. It is a real GTK Lush consumer, so the viewport-slice spec's
"any consumer can reach it" is met. Both tests must fail on current code
before the fix is applied.

### D4. The breakpoint model drives a pure reconciliation plan

`sync_secondary_surfaces` and `sync_properties_breakpoint` currently decide and
apply in the same code. Extract one pure function into
`ui/window/geometry/policy.rs`:

```
plan_shell_reconciliation(
    rendered: RenderedShellState,
    layout: AdaptiveShellLayout,
    installed_threshold: i32,
) -> ShellReconciliation
```

- `RenderedShellState` holds the current layout name, compact slot, workspace
  `show-sidebar`, properties `show-sidebar` and sheet `open`.
- `ShellReconciliation` holds `Option` writes for each of those, plus
  `reinstall_threshold: Option<i32>`.

`execution.rs` reads the rendered state, calls the plan and applies the writes
in the current order. Focus restoration stays in execution, because it reads
the GTK focus chain.

The extraction follows the workflow convention: pure policy goes in
`policy.rs`, and the plan is a seam value object only inside this row. It is
behaviour-preserving, which is shown by characterization unit tests written
before the move plus the existing shell-geometry widget tests.

This is what makes the model check production logic, as
`formal-verification-kani` requires. The model mirrors only the GTK
sequencing, the same way K4 mirrors `size_allocate`.

*Alternative considered:* mirror `sync_secondary_surfaces` inside the harness
as K4 mirrors bookkeeping. That is rejected because the reconciliation *is*
decision logic, and a mirror would drift silently.

### D5. Breakpoint-loop state, steps and envelope

The model lives in `ui/window/geometry/kani_proofs.rs` (`#[cfg(kani)]`,
declared in the facade).

**State:**

- inputs: width `w` in a bounded range, for example 360..=2560 in steps the
  harness fixes; preset; the two requested flags; the compact slot; Focus
  Mode; text scale `s` from a small ledger-cited set;
- the cached threshold;
- the installed condition threshold;
- `applied` (the breakpoint's state);
- the rendered state from D4;
- the `synced_for_width` guard;
- ghost counters: settings writes on the allocation path, and presentation
  changes.

**Step `allocate(w)`:**

1. Mirror `sync_split_view_widths_for_allocation`: guard on `w`, and skip the
   breakpoint and surface work while a settle is pending (a nondeterministic
   pending flag that clears within the bound).
2. Call the real `derive_adaptive_shell_layout` and the real
   `plan_shell_reconciliation`, and apply the writes.
3. Adwaita step, restricted only by ledger clauses:
   - breakpoint re-evaluation of `applied := w ≤ threshold·s`, either at once
     or at the next allocation, as the timing axiom pins;
   - on apply, the setter writes `sheet`, capturing the prior value;
   - on unapply, it restores the captured value, as the setter-restore axiom
     pins;
   - the allocated width may be raised to the minimum width of the current
     presentation, as the minimum-width and split-view allocation axioms pin.

**Harnesses** (one per property group, small unwind):

- `shell_loop_settles_at_a_stable_width`: fixed point within k allocations,
  plus idempotence.
- `shell_loop_sweep_does_not_flap`: two or three monotone widths across `T`.
- `shell_loop_preserves_requested_visibility`: compact width, then wide width.
- `shell_loop_layout_agrees_with_policy_at_rest`.
- `shell_loop_allocation_never_persists`: the ghost counter stays at 0.

If the `sp`/`px` mismatch (`s ≠ 1`) or setter restore yields a
counterexample, D7 applies. The f64 arithmetic in the policy is used as is;
widths are whole `i32` sp. If CBMC time is too high, the harness narrows the
width domain to the neighbourhood of the thresholds and records that domain,
per "Proved properties state their input domain".

### D6. New ledger axioms, written from probes

Candidate entries, with ids continuing after A13. Each statement is finalized
from what its probe observes:

- **A14:** how the `max-width: Nsp` condition compares with the window width,
  and its dependence on `gtk-xft-dpi` / text scale. The probe sets the
  settings DPI and bisects the apply width.
- **A15:** when a `set_condition` change is re-evaluated. The probe changes the
  condition at a fixed size and reads `current-breakpoint` synchronously and
  after one allocation.
- **A16:** setter apply and unapply ordering relative to child allocation, and
  what is restored after an application write made while the breakpoint was
  applied.
- **A17:** `AdwOverlaySplitView` sidebar width from the fraction, minimum and
  maximum; `show-sidebar` and `collapsed` do not change the toplevel
  allocation.
- **A18:** how breakpoints lower the window's minimum width, and what happens
  below the smallest breakpoint's minimum.

Probes go in `crates/lushtext/tests/widget/gtk_axioms.rs`, using pure Adwaita
fixtures only. If a behaviour cannot be isolated, for example because it
depends on the compositor, the row says why.

### D7. Counterexamples: fix when reachable, else record

The rule is the one K4 used. A breakpoint-loop or two-bin counterexample that
real GTK reaches:

1. gets a failing widget test (for the shell, `LUSHTEXT_WIDGET_CHILD` windows
   created at the target width, per the GTK4 resize caveat);
2. is then fixed;
3. and its harness becomes a proof.

One that real GTK does not reach is kept as a named `should_panic` residual,
with reachability evidence and a candidate fix in the programme record.

The likely suspect is disagreement between the policy's `px ≤ Tsp` and
Adwaita's scaled evaluation under text scaling. Its candidate fix, to be
model-checked first, is to make the policy compare in the unit A14 pins, for
example by converting the allocated width to sp through the same factor.

### D8. Unbounded attempt 1: loop contracts

The target is a rest-set invariant `R(model)`: `at_rest`, every child equals
`band_top`, and `outer ∈ [0, outer_max]`. Two harnesses use it.

- `slice_loop_rest_persists_for_any_number_of_frames`:
  `while kani::any() { #[kani::loop_invariant(R(&model) && model.outer == outer0)] model.frame(resting_child) }`.
  This is the inductive form. It is not new, because the one-step fixed-point
  check already implies it, and the harness states that openly.
- `slice_loop_spaced_requests_stay_honoured_for_any_number_of_requests`:
  - loop body: one admissible request, then two resting frames;
  - invariant: `R`;
  - postcondition per iteration: the request is honoured or clamped;
  - an optional `loop_decreases` on a request budget, to claim termination.

These are the genuinely unbounded claims: any number of frames and any number
of spaced requests.

If loop contracts reject the model, record the Kani limitation verbatim. Known
risks include:

- `loop_modifies` inference over `[Bin; N]` with closures;
- the struct-field projection limitation of `loop_decreases`;
- unwinding inside `frame`, whose inner `for` loops over N need their own
  contracts or a fixed unwind.

Loop contracts need `-Z loop-contracts`, so the shard table gains a per-shard
`kani_flags` field (D10).

### D9. Unbounded attempt 2: bin independence and non-interference

The argument, written as a proof note in the module rustdoc and in the
programme record. Content heights are fixed; the model's A12 envelope already
makes content independent of the outer value.

- **L1, frame locality.** For every state and every `j ≠ i`, `allocate(j)`
  leaves `bins[i]` and `outer` unchanged. (Harness: `slice_bin_allocation_touches_only_its_own_bin`.)
- **L2, single writer.** Only `run_idles` writes `outer`. By construction;
  checked with a ghost write counter.
- **L3, locality of the decision.** `allocate(i)`'s effect on `bins[i]` is a
  function of `(bins[i], outer − origin(i), viewport)` alone. (Harness: two
  models that differ everywhere except those inputs produce the same
  `bins[i]`.)
- **L4, one bin under any origin and any outer jump.** A `Loop<1>`
  generalized to an arbitrary origin in `[0, MAX_ORIGIN]` and arbitrary
  `outer_max`:
  - it rests within two frames from any start;
  - it rests within two frames after an arbitrary outer jump;
  - it honours a request.
- **L5, pairwise.** The D1/D2 two-bin harness. After anchoring, a sequence of
  absolute `set_value(anchor + d)` calls from one shared anchor equals its
  last call, so the pairwise result composes to any number of simultaneous
  requesters.

**Composition.** With no request, L2 keeps `outer` constant. By L1 and L3,
each bin then evolves as a one-bin loop with a constant `viewport_top`, which
L4 covers, so any N rests within two frames. After a request, other bins see
only an outer jump (L1–L3), which L4 covers. Simultaneous requests reduce to
the last one by L5.

The proof note must name every harness and state the domain, including the
MAX_* bounds per bin. The claim is "any N within the per-bin bounds". It is
not "any geometry".

### D10. Shards and CI caps

`scripts/kani-shards.py`: `SHARDS` values gain an optional third element,
`kani_flags`. `run` appends those flags, `github-outputs` exports them, and
`check` still enforces one shard per harness. The self-test covers flag
scoping.

New shards, sized from local times (the rule: under about 20 minutes locally,
to leave headroom on runners):

- `widgets-slice-loop-pairs`: the two-bin simultaneous and L1/L3 harnesses;
- `widgets-slice-loop-unbounded`: loop contracts, with
  `-Z loop-contracts`, plus L4;
- `core-shell-geometry`: the breakpoint loop.

If a harness exceeds the budget, first narrow its domain and record the
domain; only then split a shard. The phase-3 table in the programme record
gains each harness's result and time.

## Risks / Trade-offs

- **[Risk] Loop contracts are experimental and may not accept the model**
  (closures, const-generic arrays, nested loops). → This is an expected
  outcome. The limitation is recorded verbatim and attempt 2 still stands.
  The gate needs both to fail before Lean is discussed.
- **[Risk] Anchoring overrides a user scroll made between a request and its
  idle.** Today the user's scroll is added to. → The window is one idle long
  (below one frame), and "the requested row ends visible" is the contract.
  This is recorded in the widget-wiring rule and in the CHANGELOG.
- **[Risk] A sidebar two-section reproduction may need a new test seam.** →
  Prefer the real refresh path. Otherwise the adoption-lab fixture is the
  reproduction and the reason is recorded. No new `*_for_test` is added
  without updating the matrix seam count.
- **[Risk] CBMC on the f64 geometry policy may be slow.** → Narrow the width
  domain around the thresholds and state it.
- **[Risk] A new file in a migrated role home trips
  `make check-workflow-boundaries`.** → Declare `kani_proofs.rs` in the
  `WFR-SHELL-GEOMETRY` row and re-derive its measured cells in the same change.
- **[Trade-off] The pure reconciliation plan adds a small type pair to
  `policy.rs`.** It is justified because it turns the loop from mirrored logic
  into a production-driven proof.
- **[Risk] Probes for A14/A18 may depend on compositor or settings behaviour
  under headless Mutter.** → Use `GtkSettings` DPI overrides inside the
  harness process. If the dependence is real, record the axiom as not
  isolable and give the reason.

## Migration Plan

This change does not migrate anything. The two-bin fix is private to
`gtk-lush-widgets`, and the public API snapshot is unchanged. Rollback means
reverting the idle's `set_value` anchor, which returns the two-bin harness to
its `should_panic` pin.

## Open Questions

- Does A15 make re-evaluation synchronous in `set_condition`? The answer
  decides whether the model needs the one-allocation lag.
- Is the text-scale mismatch reachable with the Fedora default
  `text-scaling-factor` values (1.0, 1.25, 1.5)? The probe decides the
  statement, and D7 the outcome.
