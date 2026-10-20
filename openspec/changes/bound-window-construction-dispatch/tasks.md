## 1. Preconditions and failing-first baseline

- [x] 1.1 Confirm that C0 (`probe-long-line-toolkit-costs`) has been applied. Locate its B_settle and B_interact policy constants, its watchdog and frame-timing helpers, its smoke lane, the stream's `docs/next/` record, and the last used GTK axiom ledger id. If anything is missing, follow the stream order and apply C0 first, without duplicating its constants.
- [x] 1.2 Inside the installed Flatpak, verify which GSettings backend GLib selects (`GSETTINGS_BACKEND` unset, no dconf finish-args). Record the result and the GLib version in the stream record. The Snap check applies only once the platform snap exists.
- [x] 1.3 Add a GTK-free construction span recorder that records `startup_toolkit`, `startup_lushtext`, `window_construction`, per-stage sub-spans and `first_frame` (from the window frame clock's first `after-paint`), plus the largest dispatch before the first frame (from C0's watchdog).
- [x] 1.4 Extend C0's `baseline` construction case in the measured smoke lane (do not add a parallel scenario): release build; fresh and steady-state profiles; keyfile and dconf backends; synthetic small fixture; `btrfs-stall-report --no-below` evidence for each run window.
- [x] 1.5 Run the baseline before any fix. Record every span in the stream record, including which combinations exceed B_settle. If a steady-state profile does not reproduce the stall, record that explicitly.

## 2. GSettings backend axioms (run first, decide D3)

- [x] 2.1 Run `make gtk-axioms` on the current toolchain before adding anything.
- [x] 2.2 Add ledger entries G1–G4 (keyfile synchronous write plus fsync per `set_*`; `delay`/`apply` coalescing to one write; whether a cross-thread write blocks the other instance across the fsync; `changed` timing on a second instance). Use the next free ids, and give each a statement, its dependent designs and its verified-against versions.
- [x] 2.3 Add an isolated probe and a sample (interactive mode and `--check`) for each entry in `gtk-lush-axioms`. Each runs against a private keyfile directory inside its temporary scope. Register them in the `axiom_probes` runner.
- [x] 2.4 Run `make gtk-axioms` and `make check-gtk-axioms`. Record the observed outcomes, and choose the writer's flush path (worker flush or deferred GTK-idle batch) in the stream record.

## 3. Coalescing settings writer

- [x] 3.1 Audit every imperative GSettings write (`set_*`, `reset`, setting-backed actions) in `ui/**` and `app.rs`, starting from the list in design.md and adding any it misses. For each routed key, list every reader (any instance, any dispatch) and every `changed` subscriber, and mark which must move to the writer's overlay and notification. Record the audit table in the stream record.
- [x] 3.2 Implement the pure batch algebra in `model/settings_write_batch.rs` (last-writer-wins, change-only, snapshot and acknowledge), with unit and property tests.
- [x] 3.3 Add a Kani harness for the batch algebra (latest value never dropped; an acknowledged equal value never re-emitted; an empty batch flushes nothing). Assign it to a shard in `scripts/kani-shards.py` with a measured budget, and run `make check-kani-shards`.
- [x] 3.4 Implement the cross-cutting writer in `ui/settings_writes.rs`. It has one instance per application, seeds change-only from a backend read on first enqueue, exposes the read overlay and an enqueue-time `connect_changed(key)` that also forwards external backend changes without echoing its own flush, does not flush before the first presented frame, uses a `gtk_lush_settle::Debounce` persistence window, executes the flush on the path chosen in 2.4, flushes synchronously only on the last window's `close_request` and on application shutdown (after joining any in-flight worker flush), and lets a non-last window's close enqueue without flushing.
- [x] 3.5 Measure one Preferences bound-row toggle under the keyfile backend. If it exceeds B_interact, bind those rows against the writer's delayed instance. Otherwise record the measurement.
- [x] 3.6 Route through the writer: the `default-width`, `default-height` and `maximized` handlers (`ui/window/imp.rs`); `actions.rs`; `preview.rs:45` (the construction-time reset) and `:67`; `search.rs`; the colour-scheme actions in `mod.rs`; `geometry/execution.rs:396`; `encoding/execution.rs`; `register_boolean_setting_toggle_action` in `app.rs`; `workspace_visibility.rs`; and the imperative writes in `preferences/imp.rs` and `preferences/excluded_names.rs`. Move every reader and `changed` subscriber found in 3.1 to the overlay and the writer's notification. Keep each visible effect immediate, including effects delivered to editor pages and other windows. Correct the doc comment on `LushtextWindow::new` so that it is true.
- [x] 3.7 Add a policy check (in `make check`'s fast audits) that fails on a direct `gio::Settings` `set_*`/`reset` of any key, or a direct read or `connect_changed` of a routed key, outside `ui/settings_writes.rs`, with an allowlist for two-way `Settings::bind` rows that 3.5 keeps. Prove it red on a deliberate violation.

## 4. Pure split-view restore plan

- [x] 4.1 Add `plan_split_view_restore` and `SplitViewRestorePlan` (whole-pixel, `PaneShare`s, float-free facts with the stored workspace fraction snapped in `execution.rs`; the simplify pass dropped the plan's persist set, because the writer's change-only filter already decides what reaches the backend) to `ui/window/geometry/policy.rs`, together with a pure transcription of today's `migrate_split_view_settings` and `restore_*` as a `#[cfg(any(test, kani))]` oracle.
- [x] 4.2 Add the Kani harness to `geometry/policy/kani_proofs.rs` for the four properties in the spec, in the existing `core-geometry-policies` shard (`scripts/kani-shards.py`). Re-measure that shard's budget and run `make check-kani-shards`.
- [x] 4.3 Replace `migrate_split_view_settings` and the writes in `restore_workspace_split_view` and `restore_properties_split_view` with: apply the plan to the widgets, then hand the values the pre-plan code wrote to the writer. Keep the order relative to `install_split_view_breakpoints`.
- [x] 4.4 Extend the shell-geometry `evidence.rs` with `WindowConstructionEvidence`, filled from the span recorder and the writer counters (the largest-dispatch field is `None` without C0's watchdog feature). Decide and document whether `persisted_workspace_fraction`/`persisted_properties_fraction` report the backend or the overlay value, and keep their existing consumers' meaning.

## 5. Gates

- [x] 5.1 Write the widget test module first and show it failing on the current code (steady-state today makes three writes, fresh eight), then keep it as the construction write-count gate (first confirm the harness delivers `after-paint`; steady-state: zero writes; fresh: zero writes before the first frame, then one flush equal to the oracle; a resize burst writes at most once per window; state is exact after the last window closes; closing a non-last window performs no synchronous flush; an editor page sees an invisible-characters change and the window sees a Preferences preset change before any flush). Assign it in `scripts/widget-shards.py` and run `make check-widget-shards`.
- [x] 5.2 Re-run the measured lane on all four profile and backend combinations, and record the post-fix spans against the baseline from 1.5.

## 6. Conditional staged construction (D4)

- [x] 6.1 If any LushText-owned span or pre-first-frame dispatch still exceeds B_settle in 5.2, use the per-stage breakdown to name the dominant costs. If none exceeds it, record that D4 is not needed and close 6.2–6.5 with that evidence.
- [ ] 6.2 Move surfaces that the first frame does not show behind a `ConstructionStage` sequence: an `AdwBin` placeholder, built in its own idle dispatch, with an `*_ensure_constructed` entry for early actions. Change the `.blp` sources and regenerate the `.ui` files (`make blueprint-generate`, `make check-blueprint`). **Not done (maintainer decision, recorded in `docs/next/long-line-documents.md` "C4"):** the residual is GTK `present` and template instantiation; staging every hidden surface measured 3–4 ms of 22–26 ms, so it cannot reach B_settle. A minimal first-frame shell is proposed as C4b.
- [x] 6.3 Add `stages_pending` and `stages_total` to the evidence. Make pending stages readiness blockers for the dependent predicates.
- [ ] 6.4 Add widget tests for an early action that forces a stage, proving it behaves the same as after normal staging and stays within budget on the measured lane. **Not done:** follows 6.2 (no staged surface exists).
- [ ] 6.6 If any stable AT-SPI anchor or accessibility-smoke step depends on a staged surface, make the smoke wait for its stage, run `make accessibility-smoke`, and update `docs/accessibility.md` and `docs/accessibility-matrix.md`. **Not applicable:** nothing is staged, so no AT-SPI anchor depends on a stage.
- [ ] 6.5 Re-run the measured lane and confirm that every combination is within B_settle. **Not met:** every combination measures 22–26 ms construction (idle run `rr-20261015T052216Z`); escalated with C4b.

## 7. Automation and documentation

- [x] 7.1 Project `window_construction` into the automation snapshot (`ui/automation.rs`). Update `docs/automation.md` and `docs/automation-reference.md`, then run `make check-automation-docs` and `make automation-client-self-test`.
- [x] 7.2 Update `AGENTS.md`:
  - the module layout (`ui/settings_writes.rs`, `model/settings_write_batch.rs`, any staging module);
  - the "Window state persistence" decision (coalesced, change-only writes, a synchronous flush at close, and the crash window);
  - the shell-geometry decision (the pure restore plan).
- [x] 7.3 Update `.agents/rules/ui.md` (imperative GSettings writes go through the writer; no backend write on construction or interaction paths) and `.agents/rules/build.md` (the measured construction scenario and the new axioms).
- [x] 7.4 Update `docs/workflow-readability-matrix.md`: re-derive the measured cells of `WFR-SHELL-GEOMETRY` and of every other live row whose files this change edits (read the assignment from the matrix; `WFR-SHELL-LAYOUT` is `superseded` and is not edited), and declare the writer as cross-cutting coordination with no role. Then run `make check-workflow-boundaries`.
- [x] 7.5 Update the GTK axiom ledger, `docs/next/formal-verification.md` (the new axioms and both Kani harnesses), `docs/end-user-coverage.md`, and the stream's `docs/next/` record (C4 posture, baseline and post-fix spans, the chosen flush path, and the answers to open questions).

## 8. Verification

- [x] 8.1 Run `make check`, `make test`, `make kani KANI_SHARD=<affected shards>`, `make gtk-axioms` and the measured smoke lane, all under the 16-core cap and never against the live desktop. Fix any failure in the same work stream, as the pre-existing-blockers rule requires.
- [x] 8.2 Confirm that no smoke or test run leaves a `TEMP-LEAK:` and that no Gtk, GLib or GObject warning appears in the construction scenario logs.
