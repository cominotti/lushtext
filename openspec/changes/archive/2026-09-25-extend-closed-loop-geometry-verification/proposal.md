## Why

The Kani consolidation proved the `ViewportSliceBin` feedback loop (K4). It
left three gaps that the programme record names as open:

- **Simultaneous requests from two bins.** Every bin computes its forwarded
  delta against the same pre-move outer value, and each idle then applies
  `outer.value() + delta`. Two requests in one frame therefore add up. The K4
  harnesses issue only one request at a time, so this was never checked.
- **The adaptive-shell breakpoint loop.** allocated width → layout →
  breakpoint threshold → allocated width is the shell's second closed feedback
  loop. It has the history of visual regressions that K4 addressed for the
  slice bin, and it has no model at all.
- **Everything proved is bounded.** The results hold for 1–3 bins and a
  handful of allocations. The "Dormant: Lean" note says Lean would be the tool
  for any unbounded claim, but Kani itself has not yet been tried for one:
  neither through its loop contracts nor through a compositional argument.

These are items N3, N4 (the two-bin half) and the pre-Lean half of the
"Dormant: Lean" note in `docs/next/formal-verification-next.md`. They are the
natural next step after K4, and each one either finds a defect or narrows what
still needs a heavier tool.

## What Changes

- **Two-bin simultaneous requests.** Add a Kani harness in which two bins
  request in the same frame. Code reading predicts a counterexample: the outer
  lands at the sum of both deltas, so neither request is honoured. If the
  model confirms it:
  - write a failing-first real-GTK widget test (the adoption-lab two-bin
    fixture, plus two sidebar workspace sections where a path exists);
  - fix `ViewportSliceBin::follow_child_request` so that requests computed
    against the same outer value do not add up;
  - model-check the fix first, and keep every existing rendered-bounds,
    allocation-count and correction-count test green.

  If the model proves no double count, record the proof instead.
- **Breakpoint-loop model.** Add a Kani step model of the adaptive-shell
  breakpoint loop. It calls the real pure policy in
  `ui/window/geometry/policy.rs`. It extracts the reconciliation decision that
  `execution.rs` applies (layout name, compact slot, `show-sidebar`, sheet
  `open`, cached threshold) into a pure, production-driven function, so the
  model checks shipped logic and does not re-implement it. Adwaita is modelled
  as an envelope cited from the axiom ledger. The model proves:
  - a fixed point for stable inputs;
  - no presentation flapping, including across a monotone resize sweep;
  - requested visibility is preserved;
  - the allocation path never persists.

  A counterexample is fixed failing-first, or recorded with reachability
  evidence.
- **New ledger axioms.** Add entries for the `AdwBreakpoint` and
  `AdwOverlaySplitView` behaviours the envelope needs:
  - condition units (sp against the window width);
  - when a condition change is re-evaluated;
  - setter apply and restore semantics;
  - split-view allocation, and how breakpoints affect the window's minimum
    width.

  Each entry is pinned by an isolated headless probe in the GTK Lush family
  crate `gtk-lush-axioms` (a catalogue entry, `src/probes/aNN.rs`, and an
  interactive/`--check` sample `examples/aNN_<slug>.rs`, run by
  `make gtk-axioms`), or records why it cannot be.
- **Unbounded slice-bin results, attempted in Kani.** Two attempts:
  1. Kani loop contracts (`#[kani::loop_invariant]`, `#[kani::loop_decreases]`,
     `-Z loop-contracts`, experimental). They target "rest persists and spaced
     requests stay honoured" and "no oscillation" for an arbitrary number of
     allocations.
  2. A bin-independence / non-interference argument: harnesses plus a short
     proof note showing that the one-bin proofs, quantified over any origin
     and any outer jump, plus a pairwise non-interference harness, cover any
     number of bins.
- **Outcome gate.** Record both outcomes in the programme record. Revise the
  "Lean is dormant" note in both `docs/next` records: Lean is re-discussed only
  if **both** attempts fail **and** an unbounded claim is actually needed (for
  example GTK Lush publication), and only by a recorded maintainer decision.
- **Kani lane.** Add the new harnesses to `scripts/kani-shards.py`, with
  per-shard extra Kani flags for loop contracts. Every shard stays within the
  30-minute CI cap.

No user-visible behaviour changes, except the two-bin fix if the model
confirms the double count.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-lush-viewport-slice`: adds a requirement that simultaneous requests
  from several bins sharing one outer scroller do not add up, and a
  requirement that the loop's claims are extended beyond the bounded scope by
  the two Kani attempts, with results recorded.
- `adaptive-editor-geometry`: adds a requirement that the breakpoint feedback
  loop is Kani-verified against the ledger (fixed point, no flapping,
  requested visibility preserved, no persistence on the allocation path),
  driven through a pure reconciliation decision.
- `gtk-axiom-ledger`: adds a requirement that the Adwaita breakpoint and
  overlay-split-view behaviours the shell envelope relies on are ledger
  entries, each pinned by an isolated probe.
- `formal-verification-kani`: adds a requirement that unbounded claims are
  attempted in Kani first (loop contracts, then a compositional argument)
  before any other tool is discussed. Also adds a requirement that the shard
  table carries per-shard Kani flags for experimental features.

## Impact

- **Code:**
  - `crates/gtk-lush/widgets/src/kani_proofs.rs`: new two-bin, independence and
    loop-contract harnesses.
  - `crates/gtk-lush/widgets/src/viewport_slice_bin/imp.rs`: the request
    forwarding fix, if the model confirms the double count. A private change;
    the public API snapshot does not change.
  - `crates/lushtext-core/src/ui/window/geometry/`: `policy.rs` gains the pure
    reconciliation plan; `execution.rs` applies it; a new `#[cfg(kani)]`
    `kani_proofs.rs`.
  - `crates/gtk-lush/axioms/` (catalogue entries, probes, and samples for
    A14–A18); `crates/lushtext/tests/widget/{gtk_lush_adoption,workspace_tree_virtualization}.rs`
    and the shell-geometry widget tests.
- **Tooling:** `scripts/kani-shards.py` (new shards and per-shard flags),
  `.github/workflows/kani.yml` (reads the table), `Makefile` `kani`.
- **Docs:**
  - `docs/next/formal-verification.md` (phase 3 results, a new breakpoint-loop
    section, the deferral inventory, the Lean note);
  - `docs/next/formal-verification-next.md` (N3, N4, "Dormant: Lean");
  - the axiom ledger;
  - `docs/workflow-readability-matrix.md` (the `WFR-SHELL-GEOMETRY` file set and
    re-derived cells);
  - the GTK Lush widgets CHANGELOG and README, if the fix lands;
  - `AGENTS.md` / `.claude/CLAUDE.md` design notes;
  - `.agents/rules/widget-wiring.md`.
- **No new dependencies.** Kani stays pinned at `KANI_VERSION`. Loop contracts
  need only an unstable Kani flag, not a new tool.
