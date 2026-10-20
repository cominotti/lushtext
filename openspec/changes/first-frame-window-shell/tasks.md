## 1. Preconditions and the failing-first baseline

- [ ] 1.1 Confirm that C4 is archived on `main` and that its spans, settings writer and `RESPONSIVENESS_CONSTRUCTION_ENFORCE` exist. Read the last ledger id; A42–A44 must still be free, otherwise take the next free ids and update the design, specs and record.
- [ ] 1.2 Make permanent the instrumentation this proposal measured with:
  - frame-phase spans (`frame_layout`, `frame_paint`) for each window's first four frames in `ui/responsiveness_probe.rs`;
  - spans for the window's `measure` and `size_allocate`, behind the probe feature only;
  - one span per composition dispatch.

  Shipped builds keep empty inline functions; extend the `make check-responsiveness-lane` self-tests.
- [ ] 1.3 On an **idle** machine (load < 2, maintainer-scheduled; open question 3), run `make responsiveness-smoke` on `baseline/small*` (`RESPONSIVENESS_VARIANTS=plain`, 7 repetitions) before any change. Record in `docs/next/long-line-documents.md` ("C4b"), per profile and backend: every span, the frame phases, every dispatch ≥ 4 ms in order, the load, and `btrfs-stall-report`. Also record whether the constructing and first-frame dispatches exceed B_settle; this run is the failing-first evidence.
- [ ] 1.4 Run the realize-isolation experiment again on the idle machine: the realize dispatch of a minimal shell, and a second window's realize. If the maintainer approves (open question 2), profile the first realize with `sysprof-cli` and attribute its one-time cost. If the minimal shell's realize dispatch alone exceeds B_settle, stop here and record that the budget cannot be met in LushText. File the upstream GTK report with the profile, and ask the maintainer how to proceed.

## 2. GTK axioms (run first)

- [ ] 2.1 Run `make gtk-axioms` on the current toolchain before adding anything.
- [ ] 2.2 Add ledger entries A42 (one-time first-toplevel realize cost), A43 (`gtk_window_show` validates CSS for the whole tree, and the size computation and first measure run in the first frame's layout phase) and A44 (a subtree parented into a mapped window is styled and measured in the next frame's layout phase). Give each its statement, dependent designs (`first-frame-window-shell`), and verified-against versions.
- [ ] 2.3 Add an isolated probe and a sample (interactive and `--check`) for each to `gtk-lush-axioms`, and register them in the adoption lab's `axiom_probes` runner. A42 records magnitudes without a hard threshold.
- [ ] 2.4 Write a probe-style check that an unparented `AdwTabView` with an `AdwTabBar` bound to it accepts pages, and that a page appended before the view is parented realizes correctly once it is. If it does not hold, revise D2 before continuing.
- [ ] 2.5 Run `make gtk-axioms` and `make check-gtk-axioms`.

## 3. The shell plan and the template split

- [ ] 3.1 Failing-first: add a widget test (new module `window_composition`, assigned to the `surfaces` shard in `scripts/widget-shards.py`) that asserts the `open` handler returns without a realized window and that the shell is presented before the body exists. Show it failing on the current code. Run `make check-widget-shards`.
- [ ] 3.2 Add `plan_first_frame_shell` and `FirstFrameShellPlan` to `ui/window/geometry/policy.rs`. They are whole-pixel, under the module's `forbid`, and reuse `plan_split_view_restore` and the reconciliation rules without copying them. Add unit tests for the wide, medium, compact, Focus Mode and properties-open cases.
- [ ] 3.3 Add the Kani harness that proves the shell plan agrees with the composed shell's first reconciliation for every `i32` width, preset and intent, with `kani::cover!` on each branch. Put it in `core-geometry-policies`; if the shard's measured budget would exceed the 15-minute pull-request margin, put it in a new `core-shell-plan` shard. Add it to `ORACLES`, measure it from a dispatched run, and run `make check-kani-shards` and `make kani KANI_SHARD=<shard>`.
- [ ] 3.4 Split `resources/ui/window.blp`:
  - the shell keeps the header bar, the tab bar, a `GtkLushClipBin` holding the body host, and the status bar;
  - the new composite `LushtextWindowBody` gets `window-body.blp`;
  - the sidebar, properties, search panel, palette and preview are filled into `AdwBin` hosts.

  Run `make blueprint-generate`, `make check-blueprint` and `make lint-blueprint`. Register the new types with `ensure_type()` before `bind_template()`.
- [ ] 3.5 Create the `AdwTabView` in the shell stage as a window-owned, unparented object bound to the tab bar, and parent it into the editor box in B1.
- [ ] 3.6 Measure the shell template, B1's template and B1's first frame on the idle machine. If B1's dispatch or its frame exceeds B_settle, split B1 again (skeleton plus editor box, then the empty page) and re-measure.

## 4. Composition stages (role home `ui/window/composition/`)

- [ ] 4.1 Create the facade `mod.rs`, which narrates the stage order and each idle and frame resumption point. Add pure `policy.rs` (stage order, dependencies, the stage a surface needs, forced-chain selection), `execution.rs` (idle scheduling, frame-gated succession, `ensure_composed`, `when_composed`) and `evidence.rs`. Add the `WFR-WINDOW-COMPOSITION` row to `docs/workflow-readability-matrix.md`, with cells derived from the code, and run `make check-workflow-boundaries`.
- [ ] 4.2 Change `open`/`activate` (`app.rs`) to record the request and schedule stage W. Implement the stages W (warm text and icons), R (shell, size and maximized restore, shell plan, realize), P (present), B1, B2, B3 and H as designed. Each schedules its successor after the painted frame that shows it.
- [ ] 4.3 Route every access to a staged template child through an ensure accessor. Extend `make check-workflow-boundaries` with a check that fails on a direct `imp().<staged child>` access outside `composition/`; prove it red on a deliberate violation.
- [ ] 4.4 Route `begin_startup_data_flow`'s completion, the activation's file opens and every session-restore application through `when_composed`. Keep the duplicate-path, draft-claim and focus rules on the unchanged open path.
- [ ] 4.5 Write widget tests first, for each forced path issued before each stage:
  - Ctrl+Shift+F, Ctrl+Shift+P, F9, `win.toggle-sidebar`, `win.begin-search`, a preview action, and the Open popover button;
  - a CLI open;
  - session-restore completion;
  - an automation action.

  Each must produce the same state and snapshot as on a composed window. Measure the costliest forced chain on the measured lane. If it exceeds B_settle, limit forced completion to B1 plus the target surface (D4).
- [ ] 4.6 Add widget tests for:
  - stage order, and that no stage runs before the frame showing its predecessor has painted (drive the real `after-paint` path, as C4's gate does);
  - the shell's rail and pane allocations equal the composed allocations at wide, medium and compact widths;
  - no Gtk, Adwaita or GLib warning during composition.

## 5. Evidence, readiness, automation, accessibility

- [ ] 5.1 Extend `WindowConstructionEvidence` with real `stages_pending` and `stages_total`, `shell_first_frame_us`, `composition_complete_us`, `largest_composition_dispatch_us` (`None` without the watchdog) and `forced_stages`. Project them into the automation snapshot. Add the evidence surface's three driven proofs.
- [ ] 5.2 Add the readiness blocker `window-composition` to `app-startup` and to every predicate that depends on a staged surface. Update `docs/automation.md` and `docs/automation-reference.md`, then run `make check-automation-docs` and `make automation-client-self-test`.
- [ ] 5.3 Make sure every staged surface applies its accessible metadata when it is built. Confirm that `make accessibility-smoke` waits on `accessibility-settled`, now blocked by `window-composition`. Update `docs/accessibility.md` and the affected rows of `docs/accessibility-matrix.md` (anchors exist after composition), run `make check-accessibility-policy` and `make accessibility-smoke`.

## 6. Visual proof

- [ ] 6.1 Add the `first-frame-shell` visual-geometry scenario, which captures the shell frame and the composed frame in one session at wide, medium and compact widths, with invariant `first-frame-shell-chrome` (protected: header bar, tab bar, status bar, rail edge, pane edge; allowed: content regions with bound anchors). Key the invariant by prefix in both `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`, with parity self-tests on both sides.
- [ ] 6.2 Run `make visual-geometry-smoke` and `make visual-smoke`, and confirm that `first-frame-shell-chrome` is in `pixel_verified_invariant_ids`.

## 7. Measured verdict

- [ ] 7.1 Extend the lane's construction verdict to every dispatch until `composition_complete`, plus the frame-phase table. With `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` it fails on any composition dispatch above B_settle. Extend the driver's self-tests.
- [ ] 7.2 On the idle machine, run the four `baseline/small` profiles (7 repetitions) and record the per-dispatch results against task 1.3's baseline. If any dispatch exceeds B_settle, name it, fix it in this change, or, for the A42 realize floor only, record it as not met with the upstream report (task 1.4). Never relax the budget.

## 8. Documentation

- [ ] 8.1 Update `AGENTS.md`:
  - the module layout (`ui/window/composition/`, `LushtextWindowBody`);
  - a new "Staged window composition" key decision;
  - the window-state persistence and adaptive-shell decisions (the shell plan).
- [ ] 8.2 Update `.agents/rules/ui.md` (the widget hierarchy, the shell versus body templates) and `.agents/rules/widget-wiring.md` (staged template children go through ensure accessors; actions reach them through `ensure_composed`). Update `.agents/rules/build.md` (the measured composition verdict).
- [ ] 8.3 Update the GTK axiom ledger and `docs/next/formal-verification.md` (A42–A44 and the shell-plan harness).
- [ ] 8.4 Update `docs/next/long-line-documents.md` (the C4b section with baseline, posture, verdict and the answers to open questions) and `docs/next/long-line-evolution.md` (C4b proposed, then done). Update `docs/end-user-coverage.md` and `README.md` if its architecture text names the template.

## 9. Verification

- [ ] 9.1 Run `make check`, `make test`, `make test-widget-shard WIDGET_SHARD=surfaces`, `make kani KANI_SHARD=<affected shards>`, `make gtk-axioms`, the rustdoc lint gate, and the measured lane. Stay under the 16-core cap, inside `scripts/with-memory-cap.sh`, and never against the live desktop. Fix every failure in this work stream, as the pre-existing-blockers rule requires.
- [ ] 9.2 Confirm that no run leaves a `TEMP-LEAK:`, and that the composition scenario logs contain no Gtk, Adwaita, GLib or GObject warning.
- [ ] 9.3 Run `make run` against restored workspaces, with an isolated XDG directory and maintainer consent. Watch stderr while toggling every staged surface during and after composition.
