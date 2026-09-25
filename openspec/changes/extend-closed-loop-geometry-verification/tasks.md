## 1. Baseline and shard plumbing

- [x] 1.1 Run `make kani` (all five shards), `make check-kani-shards`, `make test-widget` and `make check` on the current tree, and record the per-shard times as the baseline. Any failure is a pre-existing blocker and is fixed first.
- [x] 1.2 Extend `scripts/kani-shards.py` with an optional per-shard `kani_flags` element. `run` appends it, `github-outputs` exports it, and `.github/workflows/kani.yml` reads it. Add a `--self-test` case proving the flags reach only their own shard. Done when `scripts/kani-shards.py --self-test` and `make check-kani-shards` pass.
- [x] 1.3 Confirm against the current Kani documentation and the pinned `KANI_VERSION` that `-Z loop-contracts`, `#[kani::loop_invariant]` and `#[kani::loop_decreases]` are available. Record the version facts in `docs/next/formal-verification.md` §2 "Tool facts".

## 2. Two bins requesting in the same frame (model first)

- [x] 2.1 Add `slice_loop_two_simultaneous_requests_do_not_add_up` (N=2) to `crates/gtk-lush/widgets/src/kani_proofs.rs`:
  - rest;
  - both bins request in one frame (A4, with A5 `page > 0`);
  - two resting frames;
  - assert "last-applied request honoured or clamped, never the sum; at rest; child holds band offset".
  Add it to a new `widgets-slice-loop-pairs` shard.
- [x] 2.2 Run the harness on the current model and record the result.
  - If it is a counterexample, pin it as `#[kani::should_panic]`, decode it with concrete playback, and record the trace in the programme record.
  - If it proves, record the proof, skip 2.3–2.8, and go to 3.
- [x] 2.3 Write the failing-first adoption-lab test in `crates/lushtext/tests/widget/gtk_lush_adoption.rs`:
  - two slice bins in one outer scroller;
  - `scroll_to(.., FOCUS)` on both lists in one main-loop turn;
  - assert that B's row is inside the outer viewport through `mapped_list_rows`, that the outer is not at `outer₀ + d_A + d_B`, and that both bins' allocation and correction counts stop growing.
  Confirm that it fails on current code.
- [x] 2.4 Write the failing-first LushText test in `crates/lushtext/tests/widget/workspace_tree_virtualization.rs`: two workspace sections whose `select_and_scroll_to` run in one turn, through the real refresh path or an existing test-utils surface. Confirm that it fails. If no such path exists without a new seam, record why in the task and in the programme record, and rely on 2.3.
- [x] 2.5 Model the anchored-delta fix in `slice_loop::run_idles` and the request bookkeeping: a per-bin anchor recorded when the pending batch starts, and an idle that sets `anchor + pending`. Flip 2.1 to a proof, and re-run every `slice_loop_*` harness at its existing N. Done when all of them keep their recorded results (proofs stay proved, the two residuals stay `should_panic`).
- [x] 2.6 Implement the fix in `crates/gtk-lush/widgets/src/viewport_slice_bin/imp.rs`. `follow_child_request` and `child_adjustment_moved` record the outer anchor; the idle calls `set_value(anchor + pending)`; within-bin accumulation is unchanged. Done when 2.3 (and 2.4 if written) pass.
- [x] 2.7 Re-run the whole slice-bin widget population and confirm that it stays green:
  - `gtk_lush_adoption::test_adoption_*` (rendered bounds, padded bins, two bins without oscillation, the request-while-reconfiguring test);
  - `workspace_tree_virtualization::*` (keyboard traversal to the last row, header visible at rest, return to top, two sections reaching both ends);
  - the property tests in `gtk-lush-widgets`.
  Rerun the new tests alone to rule out flakes.
- [x] 2.8 Update GTK Lush governance:
  - the widgets CHANGELOG entry;
  - the README's `ViewportSliceBin` forwarding paragraph;
  - the rustdoc of `follow_child_request`.
  `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make gtk-lush-doctests` and `make gtk-lush-examples` pass, and the public-API advisory shows no change.

- [x] 2.9 Revisit the A7/A8 settle envelope against real consumers: instrument the slice-bin widget tests headless (settles vs corrections), run targeted page-change experiments, and record the reachability verdict (reachable: a viewport height change after an outer scroll).
- [x] 2.10 Widen the model to an explicit A7 anchor (stray after a publish, A20), pin the pre-fix counterexample, reproduce failing-first in real GTK (adoption lab and LushText sidebar), model-check and implement the fix (re-announce after allocation; `classify_child_scroll_in_frame`), and pin the resize-coincident request residual.
- [x] 2.11 Fix a request in a scrolled-past bin being a no-op (one-pixel off-screen band, inset-aware floor), failing-first; correct A10 (mapped does not mean drawn) and harden last-row assertions (`drawn_labels`).
- [x] 2.12 Re-slice on an outer `notify::page-size` from an idle (A19).

## 3. Adwaita axioms for the breakpoint loop

- [x] 3.1 Write isolated probes and samples in the `gtk-lush-axioms` crate (`crates/gtk-lush/axioms/src/probes/aNN.rs` plus `examples/aNN_<slug>.rs`, following the crate README's "Adding an axiom"), using pure Adwaita fixtures and no LushText or GTK Lush widget, for the candidate axioms A14–A18 in design D6:
  - condition units and text-scale dependence;
  - `set_condition` re-evaluation timing;
  - setter apply and unapply ordering, and what is restored;
  - `AdwOverlaySplitView` sidebar allocation, and the effect of `show-sidebar`/`collapsed` on the toplevel allocation;
  - the effect of breakpoints on the window minimum width.
  Each probe passes headless under `make gtk-axioms`, five times in isolation.
- [x] 3.2 Add ledger rows A14–A18 to `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md` and matching `gtk-lush-axioms` catalogue entries (`probe: None` with the row's reason when not isolable). Write each statement from its probe's observation, with its dependent designs, pin, "Verified against" (copied from the printed observation), and "Sample", and extend the "Envelope use" section for the breakpoint model. Review the A7/A8 settle envelope against the A7 probe's measured 4.42 px-per-pixel settle. `make check-gtk-axioms` passes.

## 4. Pure shell reconciliation plan

- [x] 4.1 Write characterization unit tests for the current `sync_secondary_surfaces` / `sync_properties_breakpoint` decisions: layout name, compact slot, both `show-sidebar` values, sheet `open`, and threshold reinstall, across wide, medium-compact, collapsed and Focus Mode inputs. Express them as expected writes, so they can target the pure function.
- [x] 4.2 Extract `plan_shell_reconciliation(RenderedShellState, AdaptiveShellLayout, installed_threshold) -> ShellReconciliation` into `crates/lushtext-core/src/ui/window/geometry/policy.rs` (no toolkit imports). Make `execution.rs` apply its writes in the existing order; focus restoration stays in execution. Done when 4.1 passes and the shell-geometry widget tests pass unchanged (the adaptive-geometry, split-view and workspace-sidebar animation widget suites, plus `ShellGeometryEvidence` persistence-field assertions).
- [x] 4.3 Run `cargo mutants` scoped to `ui/window/geometry/policy.rs` and record the new mutant count against the 81 baseline. Every surviving mutant of the plan is killed by a unit test or justified in the matrix row.

## 5. Breakpoint-loop Kani model

- [x] 5.1 Add `crates/lushtext-core/src/ui/window/geometry/kani_proofs.rs` (`#[cfg(kani)]`, declared in the facade `mod.rs`, still within the 370-line budget). It holds the D5 state and the `allocate(w)` step, which calls the real `derive_adaptive_shell_layout` and `plan_shell_reconciliation`, and an Adwaita step whose every `kani::assume` cites an A14–A18 (or earlier) ledger id.
- [x] 5.2 Add the harnesses:
  - `shell_loop_settles_at_a_stable_width`;
  - `shell_loop_sweep_does_not_flap`;
  - `shell_loop_preserves_requested_visibility`;
  - `shell_loop_layout_agrees_with_policy_at_rest`;
  - `shell_loop_allocation_never_persists`.
  Each states its width, preset, scale and bound domain, and all are in a new `core-shell-geometry` shard. Done when `make kani KANI_SHARD=core-shell-geometry` reports a result for each harness within the shard budget.
- [x] 5.3 Triage every counterexample per design D7.
  - If real GTK reaches it: write a failing widget test (a window created at the target width, and a text-scale override where needed), model-check the candidate fix, implement it, and make the harness a proof.
  - Otherwise: keep a named `should_panic` residual, and record reachability evidence and the candidate fix.
- [x] 5.4 Add `kani::cover!` checks confirming that the proved branches are reachable: pane↔sheet transition, compact slot handed to properties, collapsed workspace, and threshold reinstall.

## 6. Unbounded attempt 1: loop contracts

- [x] 6.1 Add `slice_loop_rest_persists_for_any_number_of_frames` and `slice_loop_spaced_requests_stay_honoured_for_any_number_of_requests`, using `#[kani::loop_invariant]` over the rest set `R`, plus `#[kani::loop_decreases]` where termination is claimed. Put them in a `widgets-slice-loop-unbounded` shard with `kani_flags = ["-Z", "loop-contracts"]`.
- [x] 6.2 Run them and record the outcome: proved with its domain, or the exact Kani diagnostic and limitation (for example modifies inference, nested-loop contracts, or projection in decreases) with any workaround tried. Done when the programme record's phase-3 table has a row for each.
  *Outcome:* both harnesses compiled and ran under `-Z loop-contracts`; neither verified (CBMC out of memory for rest persists, at 25 GB with a field-wise invariant; symbolic execution unfinished after 80 minutes for spaced requests). A failing harness cannot sit in the lane, so they are recorded verbatim in the programme record, and `widgets-slice-loop-unbounded` holds the L4 harnesses of attempt 2, which need no flags. The per-shard `kani_flags` plumbing stays, tested by the self-test.

## 7. Unbounded attempt 2: bin independence

- [x] 7.1 Generalize the one-bin model to an arbitrary origin and `outer_max` (without changing the existing `Loop<N>` harnesses' results). Add:
  - `slice_bin_rests_from_any_origin`;
  - `slice_bin_rests_after_any_outer_jump`;
  - `slice_bin_honours_a_request_from_any_origin` (L4).
- [x] 7.2 Add `slice_bin_allocation_touches_only_its_own_bin` (L1), `slice_bin_decision_depends_only_on_its_own_inputs` (L3), and a ghost-counter check that only `run_idles` writes `outer` (L2), in `widgets-slice-loop-pairs`.
- [x] 7.3 Write the proof note:
  - in the `slice_loop` module rustdoc and in `docs/next/formal-verification.md` phase 3;
  - composing L1–L5 into "any N rests within two frames; no oscillation; simultaneous requests reduce to the last";
  - naming each harness and the per-bin domain bounds, and saying exactly which claims are not covered (the two residuals, and any geometry outside the bounds).

## 8. Outcome gate and lane budget

- [x] 8.1 Run `make kani` for every shard and confirm that each shard's local time leaves headroom under the 30-minute CI cap (the target is under about 20 minutes). Narrow a domain (and record it) or split a shard otherwise. Dispatch `kani.yml` once and record the runner times, where runner access allows.
- [x] 8.2 Record the outcomes of both unbounded attempts in `docs/next/formal-verification.md` phase 3. Rewrite §2 "Lean is dormant" and the "Dormant: Lean" section of `docs/next/formal-verification-next.md`: Lean is re-discussed only if **both** Kani attempts fail **and** an unbounded claim is needed (for example GTK Lush publication), and only by a recorded maintainer decision.

## 9. Documentation sync and gates

- [x] 9.1 Update `docs/next/formal-verification.md`:
  - status line and final lane-run figures;
  - phase 3: remove "Still open", and add the two-bin result, the breakpoint-loop subsection with its table and envelope assumptions, and the unbounded attempts;
  - deferral inventory: new residuals, and the retired concurrent-request item.
  Mark N3 and the two-bin half of N4 as done in `docs/next/formal-verification-next.md` and update its §1 counts.
- [x] 9.2 Update the `WFR-SHELL-GEOMETRY` row in `docs/workflow-readability-matrix.md`: the file set gains `kani_proofs.rs`, and the owned pure policy gains `plan_shell_reconciliation`. Re-derive the measured cells (size, seam counts, policy consumers) row-scoped, excluding `#[cfg(test)]`. `make check-workflow-boundaries` passes.
- [x] 9.3 Update `AGENTS.md` / `.claude/CLAUDE.md`:
  - the viewport-slice design note (anchored forwarding, and why within-bin accumulation stays);
  - the adaptive document-properties design note (the reconciliation plan and the Kani model).
  Add the anchored-forwarding rule to `.agents/rules/widget-wiring.md`, and any new testing pitfall to the `gtk-testing` skill references.
- [x] 9.4 Run `make check`, `make test`, `make test-widget`, `make check-kani-shards`, the GTK Lush lanes from 2.8, and `make visual-geometry-smoke` (shell geometry invariants unchanged). All pass with no new warnings in the widget-lane output.
- [x] 9.5 Run `openspec validate extend-closed-loop-geometry-verification --strict`. It is valid.
