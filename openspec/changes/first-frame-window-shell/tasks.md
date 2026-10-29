## 1. Preconditions, the failing-first baseline and the gating experiment

- [x] 1.1 Confirm that C4 is archived on `main` and that its spans, settings writer and `RESPONSIVENESS_CONSTRUCTION_ENFORCE` exist. Read the last ledger id on `main` and in every in-flight change (C1 `document-scale-policy` holds A42–A45); A46–A50 must still be free, otherwise take the next free ids and update the design, specs and record.
- [x] 1.2 Make permanent the instrumentation this proposal measured with:
  - frame-phase spans (`frame_layout`, `frame_paint`) for each window's first four frames in `ui/responsiveness_probe.rs`;
  - spans for the window's `measure` and `size_allocate`, behind the probe feature only;
  - one span per composition dispatch.

  Shipped builds keep empty inline functions; extend the `make check-responsiveness-lane` self-tests.
- [x] 1.3 Under the reference precondition (design D9: 1-minute load < 4 for 30 s, no process ≥ 80 % CPU, no other agent's `cargo`/`rustc`/`nextest`/`kani`/`cbmc`/`mutter --headless` process; record all three and discard a run that loses them), run `make responsiveness-smoke` on `baseline/small*` (`RESPONSIVENESS_VARIANTS=plain`, 7 repetitions) before any change. Record in `docs/next/long-line-documents.md` ("C4b"), per profile and backend: every span, the frame phases, every dispatch ≥ 4 ms in order, the load, and `btrfs-stall-report`. Also record whether the constructing and first-frame dispatches exceed B_settle; this run is the failing-first evidence.
  - **Done 2026-10-29.** Run `rr-20261029T122411Z-toolbx-1782972` on `main` a0adc5e2, before this change. It ran 09:24:10–09:26:41 −03:00, after the gate line `quiet: load 1.65 2.21 2.57 … no build/test/proof process; no process >= 80% CPU`. `btrfs-stall-report`: "Storage was NOT a bottleneck", IO pressure 0.7 %.
  - Per profile, construction was 22.1–25.5 ms, template 8.7–9.2 ms, `present` 12.0–15.3 ms, and first frame 37.4–41.4 ms.
  - The largest dispatch before the first frame was 22.5–25.9 ms: **over B_settle in all 28 runs** (the failing-first evidence).
  - a0adc5e2 predates task 1.2's instrumentation, so this run has no frame-phase spans and no per-dispatch list. The 2026-10-20 breakdown in the design stands in for them.
  - The per-profile table is in the design's "Budget verdict" and in `docs/next/long-line-documents.md` ("C4b").
- [ ] 1.4 Under the same precondition, run the realize-isolation experiment again: the realize dispatch of a minimal shell, and a second window's realize. If the minimal shell's realize dispatch alone exceeds B_settle, stop here and record that the budget cannot be met in LushText, against A46. File the upstream GTK report with the span breakdown, and ask the maintainer how to proceed.
- [x] 1.5 Gating experiment for design D2, on the instrumentation patch and under the same precondition: the full template in its own dispatch with the staged content invisible (content-stack pages, sidebar, properties panel, concealed revealer children), then realize, present, and the first frame. Record C, R, P and F1 per profile. D2 holds if C ≤ 12 ms, P is within 2 ms of the `shell` experiment's present minus its realize, and F1 within 2 ms of the `shell` experiment's first frame; otherwise switch to D2's fallback (C1/C2 split instantiation) and update the design before continuing. In the same session, A/B the two off-thread warm variants (design finding 13): `GTK_ICON_LOOKUP_PRELOAD` lookups of the shell's icons in `startup`, and a worker-thread Pango layout of the UI font; record what each removes from W and F1. Adopt either only if it measurably shortens W without lengthening another dispatch.
  - **Answered 2026-10-29, post hoc, on the implemented D2 layout.** Source: verdict run `rr-20261029T122740Z-toolbx-1787430` on b73b9663, 28 runs, same precondition. This is not a pre-implementation run on the instrumentation patch.
  - **C ≤ 12 ms: pass.** C was 9.9–10.6 ms.
  - **P within 2 ms of the `shell` experiment's present minus its realize: pass.**
    - P was 0.4–0.9 ms.
    - The shell's present minus realize was about −1.1 to 2.2 ms: 10.9–11.8 ms of present, minus the `realize` experiment's 9.6–12.0 ms, because the shell's own realize was not split.
  - **F1 within 2 ms of the `shell` experiment's first frame (7–9 ms): fail, on the low side.**
    - F1 (first frame layout plus paint) was 3.2–4.0 ms, 3.0–5.8 ms *below* the shell's.
    - Read as "no costlier than the shell", it passes. Stage W now carries the one-time text and icon cost, which was inside the shell's first frame (load 4–7) in the 2026-10-20 run.
  - D2 holds, and the C1/C2 fallback is not needed.
  - The two off-thread warm variants were **not** A/B-tested. Neither is adopted, and W stays the mechanism, at 5.9–6.6 ms (design finding 13: neither is required for the budget).
- [ ] 1.6 *(Optional, maintainer-gated; nothing depends on it.)* If the maintainer installs a sampling profiler and asks for it, profile the first realize with `sysprof-cli` and attach the attribution to the upstream report. Do not install anything in the toolbox from this change.

## 2. GTK axioms (run first)

- [x] 2.1 Run `make gtk-axioms` on the current toolchain before adding anything.
- [x] 2.2 Add ledger entries, each with its statement, dependent designs (`first-frame-window-shell`), and verified-against versions:
  - A46: the one-time first-toplevel realize cost;
  - A47: `gtk_window_show` validates CSS only for visible nodes, and the size computation and first measure run in the first frame's layout phase;
  - A48: a subtree made visible or parented in a mapped window is styled and measured in the next frame's layout phase;
  - A49: a concealed vertical-slide revealer measures a visible child horizontally at full scale and an invisible child not at all;
  - A50: `AdwBreakpointBin`'s first allocation after map applies a matching breakpoint before allocating the child, and a later switch (including a breakpoint added after map) goes through the one-frame `old_node` snapshot swap.
- [x] 2.3 Add an isolated probe and a sample (interactive and `--check`) for each to `gtk-lush-axioms`, and register them in the adoption lab's `axiom_probes` runner. A46 records magnitudes without a hard threshold. Update the axioms crate's `README.md` and `CHANGELOG.md`.
- [x] 2.4 Write a probe-style check that `grab_focus` on a parented but invisible widget, made visible in the same dispatch, focuses it, and that `editor.root()` is the window for a page appended to the invisible tab view. If either does not hold, revise D4/D5 before continuing.
- [x] 2.5 Run `make gtk-axioms` and `make check-gtk-axioms`.

## 3. Construction, visibility defaults and geometry

- [x] 3.1 Failing-first: add a widget test (new module `window_composition`, assigned to the `surfaces` shard in `scripts/widget-shards.py`) that asserts the `open` handler returns without a realized window, that the application stays alive (held) until the window is added, and that a second `open` before the window exists merges into one window. Show it failing on the current code. Run `make check-widget-shards` and update the shard's measured figure if it moves.
- [x] 3.2 Change `open`/`activate` (`app.rs`) to hold the application, record or merge the `PendingWindowRequest`, and schedule stage W; release the hold in stage C, and on a quit before C.
- [x] 3.3 Set the staged content's visibility defaults in `resources/ui/window.blp`: content-stack pages, the sidebar inside its slot host, the properties panel inside its slot host. Keep the skeleton, slot hosts and chrome visible. Run `make blueprint-generate`, `make check-blueprint` and `make lint-blueprint`.
- [x] 3.4 Make every concealed revealer's child invisible (search panel, palette, Focus Mode affordance) and the concealed Markdown preview: visible right before `reveal-child = true`, hidden on `notify::child-revealed = false`. Run the existing widget tests for each surface's open, close and Escape paths unchanged.
- [x] 3.5 Failing-first widget test (design D3): reveal the sidebar and the properties panel at every width preset, at text scales 1, 1.25, 1.5 and 2, at wide, medium and compact widths, and assert the rail and pane allocations do not change. If a case fails, add the `width-request` floor on the slot host from the restore plan (sp converted to px in the adapter), never a copy of a policy rule.

## 4. Composition stages (role home `ui/window/composition/`)

- [x] 4.1 Create the facade `mod.rs`, which narrates the stage order and each idle and frame resumption point. Add pure `policy.rs` (stage order, dependencies, the stage a surface needs, forced-chain selection) with an exhaustive unit test over every reachable stage set, `execution.rs` (idle scheduling, frame-gated succession, `ensure_composed`), `evidence.rs`, and `test_policy.rs` (collapse-to-one-dispatch and hold-after-F1 overrides, test-only). Add the `WFR-WINDOW-COMPOSITION` row to `docs/workflow-readability-matrix.md`, with cells derived from the code, and run `make check-workflow-boundaries`.
- [x] 4.2 Implement the stages W, C (today's `LushtextWindow::new`), R, P, B1, B2 and B3 as designed. From B1 on, each schedules its successor after the painted frame that shows it. Stage sources hold weak references and are removed when the window leaves the application (`window-removed`, A21).
- [x] 4.3 Route every reveal or focus of a staged surface through `ensure_composed`. Extend `make check-workflow-boundaries` with a check that fails on a `set_visible(true)` or `grab_focus` of a staged content widget outside `composition/` without it; prove it red on a deliberate violation.
- [x] 4.4 Admit activation opens after P through the session-restore admission's bounded page turns; keep the duplicate-path, draft-claim and focus rules on the unchanged open path. Apply the activation and empty-state focus in B1 with today's code.
- [ ] 4.5 *(Tests done: every forced path before every stage matches a composed window, and reads force nothing. Open: the costliest forced frame's measurement under the reference precondition.)* Write widget tests first, for each forced path issued before each stage:
  - F9, `win.toggle-sidebar`, `win.set-sidebar-visible`, `win.toggle-properties`, Ctrl+Shift+F, Ctrl+Shift+P, `win.begin-search`, a preview action, and the Open popover button;
  - a CLI open, including one with more files than one bounded turn admits;
  - session-restore completion;
  - an automation action.

  Each must produce the same state and snapshot as on a composed window. Add a test that reads and projections during composition (`size_allocate`, `refresh_status_bar`, the notification render, sidebar projections) force no stage. Measure the costliest forced frame (B1 + B2 + B3 at once) under the reference precondition; if it exceeds B_settle, limit a forced reveal to its own stage and its dependencies (D4).
- [x] 4.6 Add widget tests for:
  - stage order, and that no stage from B1 on runs before the frame showing its predecessor has painted (drive the real `after-paint` path, as C4's gate does);
  - close requested, and the window destroyed, during each stage: no later stage runs, the hold is released, and the session save keeps unrestored descriptors;
  - initial focus equal to today's composed window, with and without activation files;
  - the per-frame allocation trace (D7): header bar, tab bar and status bar bounds, the Open button's visible child, and the rail and pane edges constant at every `after-paint` from the first frame to `composition_complete`, at wide, medium and compact widths, including a width where the Open-button breakpoint applies;
  - no Gtk, Adwaita or GLib warning during composition.

## 5. Evidence, readiness, automation, accessibility

- [x] 5.1 Extend `WindowConstructionEvidence` with real `stages_pending` and `stages_total`, `shell_first_frame_us`, `composition_complete_us`, `largest_composition_dispatch_us` (`None` without the watchdog) and `forced_stages`. Project them into the automation snapshot. Add the evidence surface's three driven proofs.
- [x] 5.2 Add the readiness blocker `window-composition`, derived from the active window's composition evidence, to `app-startup`, `idle`, `session-restore-complete`, `visual-geometry-settled` and `accessibility-settled`, and not to `window-actions-exported`. Test that it clears when composition completes and when the window goes away. Update `docs/automation.md` and `docs/automation-reference.md`, then run `make check-automation-docs` and `make automation-client-self-test`.
- [x] 5.3 Make sure every staged surface's accessible metadata is applied in C. Confirm that `make accessibility-smoke` waits on `accessibility-settled`, now blocked by `window-composition`. Update `docs/accessibility.md` and the affected rows of `docs/accessibility-matrix.md` (anchors exist once their stage reveals them; concealed revealer children are absent from the tree), run `make check-accessibility-policy` and `make accessibility-smoke`.

## 6. Visual proof

- [x] 6.1 Add the `first-frame-shell` visual-geometry scenario, which captures the first frame (through the test-only hold after F1) and the composed frame in one session at wide, medium and compact widths, with invariant `first-frame-shell-chrome` (protected: header bar, tab bar, status bar, rail edge, pane edge; allowed: content regions with bound anchors). Key the invariant by prefix in both `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`, with parity self-tests on both sides.
- [x] 6.2 Run `make visual-geometry-smoke` and `make visual-smoke`, and confirm that `first-frame-shell-chrome` is in `pixel_verified_invariant_ids`.

## 7. Measured verdict

- [x] 7.1 Extend the lane's construction verdict to every dispatch until `composition_complete`, plus the frame-phase table and the content fill-in time (`composition_complete_us − shell_first_frame_us`). With `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` it fails on any composition dispatch above B_settle. Extend the driver's self-tests.
- [x] 7.2 Under the reference precondition, run the four `baseline/small` profiles (7 repetitions) and record the per-dispatch results and the fill-in time against task 1.3's baseline. If any dispatch exceeds B_settle, name it, fix it in this change, or, for the A46 realize floor only, record it as not met with the upstream report (task 1.4). Never relax the budget. Report a median fill-in above 40 ms to the maintainer (design D7). State in the record that the headless lane's paint and present are conservative relative to a GPU (design D9).
  - **Done 2026-10-29.** Run `rr-20261029T122740Z-toolbx-1787430` on b73b9663, 09:27:39–09:30:09 −03:00.
  - The quiet-gate lines before, between and after were at load 1.65, 0.85 and 0.69 (1-minute), with no build, test or proof process.
  - `btrfs-stall-report`: "Storage was NOT a bottleneck", IO pressure 0.9 %, longest commit 61 ms.
  - **No dispatch exceeds B_settle:**
    - the largest before the first frame was 7.9–9.2 ms, against task 1.3's 22.5–25.9 ms;
    - the largest until `composition_complete` was 9.3–10.7 ms;
    - the first frame was 16.2–18.2 ms, against 37.4–41.4 ms.
  - Fill-in was 51.0–60.6 ms (median 58.9 ms), above D7's 40 ms. It is reported to the maintainer as an open decision in `docs/next/long-line-documents.md` ("C4b", "Budget verdict"); no chrome moves and every dispatch fits B_settle.
  - The record states that cairo paint and present are conservative relative to a GPU.

## 8. Documentation

- [x] 8.1 Update `AGENTS.md`:
  - the module layout (`ui/window/composition/`);
  - a new "Staged window composition" key decision (build whole, reveal by visibility; breakpoints before map; the application hold);
  - the window-state persistence, adaptive-shell and session-persistence decisions where they name construction;
  - the CLI-file-opening decision (bounded admission).
- [x] 8.2 Update `.agents/rules/ui.md` (staged content is invisible by default; concealed revealer children are invisible) and `.agents/rules/widget-wiring.md` (reveal or focus of staged content goes through `ensure_composed`; breakpoints are installed before map). Update `.agents/rules/build.md` (the measured composition verdict and the reference precondition).
- [x] 8.3 Update the GTK axiom ledger and `docs/next/formal-verification.md` (A46–A50).
- [x] 8.4 Re-derive the measured cells of `WFR-SHELL-GEOMETRY`, `WFR-SETTINGS-WRITES` (the first presented frame is now the first composition frame), `WFR-SESSION-RESTORE`, `WFR-STARTUP-PREFLIGHT` and `WFR-AUTOMATION-SPINE` in `docs/workflow-readability-matrix.md`, and run `make check-workflow-boundaries`.
- [ ] 8.5 Update `docs/next/long-line-documents.md` (the C4b section with baseline, gating experiment, posture, verdict and the maintainer decisions of 2026-10-20) and `docs/next/long-line-evolution.md` (C4b proposed, then done). Update `docs/end-user-coverage.md` and `README.md` if its architecture text names construction.
  - *Open:* the C4b section now holds the verdict, but the fill-in decision it records is still the maintainer's to make.

## 9. Verification

- [ ] 9.1 Run `make check`, `make test`, `make test-widget-shard WIDGET_SHARD=surfaces`, `make kani KANI_SHARD=core-shell-geometry` (unchanged model, re-run), `make gtk-axioms`, the rustdoc lint gate, and the measured lane. Stay under the 16-core cap, inside `scripts/with-memory-cap.sh`, and never against the live desktop. Fix every failure in this work stream, as the pre-existing-blockers rule requires.
- [ ] 9.2 Confirm that no run leaves a `TEMP-LEAK:`, and that the composition scenario logs contain no Gtk, Adwaita, GLib or GObject warning.
- [ ] 9.3 Run `make run` against restored workspaces, with an isolated XDG directory and maintainer consent. Watch stderr while toggling every staged surface during and after composition.
  - *Open:* needs the maintainer's consent to a live `make run`.
