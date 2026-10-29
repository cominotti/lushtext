## 1. Preconditions and the failing-first baseline

- [ ] 1.1 Confirm that C4b is archived on `main` with the frame-record fix (`959018d2`), and that the next free ledger id is still A51 on `main` and in every in-flight change. If it is taken, use the next free id and update the design, the specs and the record.
- [ ] 1.2 Re-apply the A/B patch (`.claude/handoffs/c4c/c4c-ab-instrumentation.patch`) to a scratch build only, never to the tree. Reproduce `x7-base` and `x7-recommended` (`.claude/handoffs/c4c/ab-run.sh`, 7 repetitions) if more than a week has passed or the toolkit changed. Record any drift in the design before implementing.
- [ ] 1.3 Failing-first baseline under the reference precondition (C4b design D9: the 1-minute load below 4 for 30 s, no process at ≥ 80 % CPU, no other agent's `cargo`/`rustc`/`nextest`/`kani`/`cbmc`/`mutter --headless` process; record all three and discard a run that loses them). Run `main` on `baseline/small*` (4 profiles × 7 repetitions, `RESPONSIVENESS_VARIANTS=plain`, `RESPONSIVENESS_PHASES=background`, the measurement flock held for one run at a time). From the frame records and the existing spans, record:
  - the unmapped first `after-paint` per run;
  - the first painted frame and the first presentation, from the request;
  - composition complete;
  - the honest fill-in (from the first painted frame to the later of complete and the first frame after the activation turn);
  - every dispatch ≥ 4 ms;
  - `btrfs-stall-report`.

  This is the failing-first evidence. The honest fill-in is above 3 reference frames and the unseen frame is present.

## 2. GTK axiom A51 (run first)

- [ ] 2.1 Run `make gtk-axioms` on the current toolchain before adding anything.
- [ ] 2.2 Add the A51 ledger entry: the realize frame runs ahead of default-idle sources, is never presented on an unmapped surface, and paces the next frame one `FRAME_INTERVAL` later; realize and present in one dispatch runs no such frame. Record its statement, its dependent design (`shorten-window-content-fill-in` D1), the `gdkframeclockidle.c` lines it rests on, and the verified-against versions.
- [ ] 2.3 Add an isolated probe and a sample (interactive and `--check`) to `gtk-lush-axioms`, and register them in the adoption lab's `axiom_probes` runner:
  - with split dispatches, the probe asserts an unmapped `after-paint` before map and a next frame time at least 16 ms later;
  - with one dispatch, it asserts no unmapped `after-paint`.

  Update the crate's `README.md` and `CHANGELOG.md`.
- [ ] 2.4 Run `make gtk-axioms` and `make check-gtk-axioms`.

## 3. Composition: one realize-and-present dispatch, one reveal dispatch

- [ ] 3.1 Failing-first widget tests in `crates/lushtext/tests/widget/window_composition.rs` (shard `surfaces`; no new module, so `scripts/widget-shards.py` membership does not change). Show each one failing on the current code:
  - a window composed through the request path paints every `after-paint` while mapped;
  - the construction evidence's first frame is a mapped frame;
  - once the first frame has painted, one dispatch reveals content, workspace and properties, and composition completes on the next painted frame. This rewrites `test_reveal_stages_follow_painted_frames_in_order`: it keeps "the first frame shows the shell only" and "stages in order", and replaces "one stage per frame" with "every reveal in the frame after the first".
- [ ] 3.2 In `ui/window/composition/policy.rs`, replace `next_scheduled_reveal` with `scheduled_reveals_now` (the pending reveal stages before any test hold, as a `StageSet`), make `FramePaintedDecision::Queue` carry it, and rewrite the exhaustive unit tests over every reachable plan, including holds.
- [ ] 3.3 In `execution.rs`:
  - run stage R's realize and P's present in one dispatch, emitting both spans;
  - reveal the decided set in one dispatch, in stage order, with one span per stage, and queue the activation turn as B1 does.

  Keep `composition_window_realized`/`_mapped` as observers. **Fallback** (design D2, only if task 6.2 measures the populated profile's reveal frame above B_settle): two reveal dispatches, content and workspace, then properties. **Fallback** (design D1, only if the merged dispatch measures above B_settle): P as a `G_PRIORITY_HIGH_IDLE` source.
- [ ] 3.4 Update the facade narration in `composition/mod.rs`: the stage order, which dispatches exist, and the resumption points (one `after-paint` resumption now, plus the activation turns).
- [ ] 3.5 Re-run every existing `window_composition` test unchanged apart from 3.1's rewrite:
  - forced paths before every stage;
  - reads force nothing;
  - close during each stage;
  - rail and pane at every preset and text scale;
  - the per-frame chrome trace;
  - the opaque cover;
  - the tab strip from the first frame;
  - initial focus;
  - the readiness blocker;
  - the CLI open's priority over the restored session;
  - teardown.

  Add `test_closing_during_each_stage_stops_composition` coverage for the merged RP dispatch.

## 4. The first frame where it is seen, and activation shown

- [ ] 4.1 Failing-first widget test in `window_construction` (shard `surfaces`): with a window whose realize frame is forced to run unmapped (realize, then present in a later default-idle source, issued by the test), the construction evidence's `first_frame_us` and the settings writer's first-frame gate fire on the first mapped frame, not the unmapped one.
- [ ] 4.2 In `ui/window/construction.rs`, record the first `after-paint` on which the window is mapped. `first_frame`, `largest_dispatch_before_first_frame_us` and `settings_writes::first_frame_presented()` follow it.
- [ ] 4.3 Add `activation_shown_us` to `CompositionEvidence` (the first painted frame after the activation queue first drains; `None` without files). Keep the frame handler until both it and completion are known, and disconnect it at teardown as today. Project the field into `WindowConstructionEvidence` and the automation snapshot (`model/automation.rs`, `ui/automation.rs`). Re-run the evidence surface's three driven proofs over it. Update `docs/automation.md` and `docs/automation-reference.md`, then run `make check-automation-docs` and `make automation-client-self-test`.

## 5. Measured lane

- [ ] 5.1 In `scripts/responsiveness-smoke-driver.py`:
  - report the first presentation time from the probe's `frame` records;
  - define the fill-in as `max(composition_complete, activation_shown) − first_frame`;
  - check every run against `5 × REFERENCE_FRAME_US` and the median against `3 × REFERENCE_FRAME_US`, both read from the existing constant;
  - fail under `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` (never in CI) when a run exceeds 5 frames.

  Extend the self-tests (`make check-responsiveness-lane`) with an unmapped-first-frame fixture, an activation-shown-after-complete fixture, and the 5-frame failure.
- [ ] 5.2 Add the `steady-populated` construction profile (design D4): one synthetic workspace folder of a few hundred entries generated in the run's temp scope, with both side panes shown. Update the lane documentation in `.agents/rules/build.md`.

## 6. Verdict

- [ ] 6.1 Run `make check`, `make test-widget-shard WIDGET_SHARD=surfaces`, `make gtk-axioms`, `make check-gtk-axioms`, `make visual-geometry-smoke` (`first-frame-shell-chrome` must stay in `pixel_verified_invariant_ids`), `make visual-smoke`, `make accessibility-smoke`, `make crash-recovery-smoke` and the rustdoc lint gate. Run heavy commands through `scripts/with-memory-cap.sh`, one at a time, on CPUs 8–23, headless only. Run `make check-widget-shards`, and update the `surfaces` budget only if CI measures it past its margin.
- [ ] 6.2 Under the reference precondition, run `main` (task 1.3) and the branch back to back: 5 profiles (the four `baseline/small` ones plus `steady-populated`) × 7 repetitions, the measurement flock held one run at a time. Record per profile:
  - every dispatch until composition complete, which must be ≤ B_settle;
  - the RP dispatch and the reveal frame;
  - the first painted frame and the first presentation, which must be earlier than `main`'s honest first frame;
  - composition complete and activation shown;
  - the fill-in, which must be ≤ 5 reference frames in every run, with a median ≤ 3 reference frames;
  - the load lines and `btrfs-stall-report`.

  If the populated reveal frame or the RP dispatch exceeds B_settle, apply task 3.3's fallback and re-run. Never relax a budget.
- [ ] 6.3 Confirm that no run leaves a `TEMP-LEAK:`, and that no Gtk, Adwaita, GLib or GObject warning appears in the composition logs.

## 7. Documentation

- [ ] 7.1 Update `docs/next/long-line-documents.md`:
  - a C4c row in the sequence table and a C4c section with the breakdown, the candidates, the verdict and its precondition lines;
  - a correction note beside C4b's first-frame figures: an unpresented frame, with the honest values from task 1.3.

  Update `docs/next/long-line-evolution.md`.
- [ ] 7.2 Update `AGENTS.md`: the staged-composition decision (realize and present together, one reveal dispatch, and the first frame defined as the first mapped frame) and the composition module description.
- [ ] 7.3 Update `docs/workflow-readability-matrix.md`: re-derive `WFR-WINDOW-COMPOSITION`'s measured cells (sizes, seams, the new evidence field) and `WFR-SETTINGS-WRITES` (the first-frame gate), then run `make check-workflow-boundaries`.
- [ ] 7.4 Update the GTK axiom ledger (A51) and `docs/next/formal-verification.md`'s axiom list.
- [ ] 7.5 Update `.agents/rules/build.md` (the lane's first-frame, presentation and fill-in definitions, and the populated profile) and `.agents/rules/widget-wiring.md`, if it names the staged reveal order.
