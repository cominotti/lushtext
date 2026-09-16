## 1. Reproduce the defects before fixing them

- [x] 1.1 Confirm under the widget harness that a file created externally in an unexpanded directory is absent from the palette index, while the same file created through the sidebar context menu is present
- [x] 1.2 Confirm the save gap: save an untitled document into a workspace folder and record that its path is not searchable
- [x] 1.3 Confirm the sidebar emptiness gap: create a file inside a collapsed directory that shows `(Empty)` and record that the label, the expander, and `Focus Folder` all stay stale
- [x] 1.4 Keep each reproduction as a failing assertion, so every fix below has a proof it was needed

## 2. The save gap — independent and shippable on its own

- [x] 2.1 Confirmed no new seam is needed: `workspace_folder_for` is a pure prefix test against the raw configured folders (`index.rs:761`), and `indexed_file_from_path` (`index.rs:1297`) resolves the canonical identity on the mutation worker, so admitting the requested path already yields a build-identical entry with no GTK-thread syscall
- [x] 2.2 Admit the saved destination through `update_index_file_created` in `ui/window/dialogs.rs::complete_save_as`, which is the one path by which the application creates a file the index has never seen; no palette reach-through added to `ui/editor_page/save/`
- [x] 2.3 Cover the untitled first save, Save As to a new workspace path, a symlinked save admitting the resolved target, and a save outside every workspace folder

## 3. Policy, as pure functions, before any wiring

- [x] 3.1 Add the attention-refresh admission decision to `ui/command_palette/policy.rs`: the process-wide adaptive interval keyed on the workspace folder set, with floor and ceiling, derived from the previous pass's observed duration
- [x] 3.2 Add the suppression predicate: refuse while any editor is saving, a close transaction is in flight, or a draft autosave is pending; a refusal is never queued for replay
- [x] 3.3 **Amended by the convention**: the throttle has two owning workflows, so `.agents/rules/workflow-convention.md` makes it cross-cutting rather than owned by either `policy.rs`. It lives in `model/attention_refresh.rs`, beside the existing `model/editor_memory.rs` precedent, which also keeps it mutation-scoped through the `model/**` glob
- [x] 3.4 Unit tests for floor, ceiling, proportional deferral, refusal-not-queued, and the multi-window single-refresh case
- [x] 3.5 Property test that no sequence of triggers can produce refreshes closer together than the floor
- [x] 3.6 No `.cargo/mutants.toml` edit needed: `examine_globs` already covers `crates/lushtext-core/src/model/**/*.rs` by convention, which is the payoff of placing cross-cutting policy in `model/` rather than under a workflow

## 4. Coordination and wiring

- [x] 4.1 Own the interval state in `ui/command_palette/index_admission.rs`; keep it process-wide rather than per-window
- [x] 4.2 Hook `notify::is-active` in `ui/window/imp.rs`, beside the existing visibility hookup, routing through the policy
- [x] 4.3 Trigger from command-palette open in `ui/window/palette_shell.rs`
- [x] 4.4 Trigger the sidebar's existing `queue_auto_full_refresh()` from the same attention moments
- [x] 4.5 Not needed, and adding it would have been the wrong shape: `refresh_workspace_surfaces_on_attention` **returns** its `AttentionRefreshAdmission`, so the decision is observable at the production entry point rather than through a new evidence field or a `*_for_test` getter. The externally reachable `*_for_test` count stays at 165, within the recorded ceiling
- [x] 4.6 Confirmed: `ui/automation.rs:741` already blocks `command-palette-index` on `file_index_builds.has_work()` and pending index updates, and the widget tests wait on the index settling through it. No predicate added, so no `docs/automation.md` or `docs/automation-reference.md` change and no automation-docs gate to re-run

## 5. Coverage

- [x] 5.1 Widget coverage for the palette: branch-switch shape, deep terminal-created file, external removal, excluded directory stays excluded, and a newer build racing a refresh
- [x] 5.2 Widget coverage for the sidebar: gained content, emptied on disk, hidden-only entries, and tree state preserved
- [x] 5.3 Widget coverage for suppression: a save in flight refuses a refresh, and a save completes within budget while a refresh is in flight
- [x] 5.4 **Retired with the sweep it was specified for.** The lane existed to stop a warm-cache benchmark hiding the sweep's per-directory probe cost. With the reuse design the work started at an attention moment is the already-benchmarked, already-cancellable `rebuild_file_index()`; what is new is the throttle, whose behaviour under an arbitrarily slow pass is covered by `prop_admitted_refreshes_never_start_closer_than_the_floor` without needing a slow filesystem to produce one. Recorded in `docs/end-user-coverage.md`
- [x] 5.5 Every async assertion polls a predicate (`wait_until` for positives, a local non-panicking `polled` for negatives, which `wait_until` cannot express because it panics on timeout). The one fixed delay is a deliberate grace period *before* a negative assertion, so it cannot pass by checking too early
- [x] 5.6 Prove each new check fails against the unfixed code, per the pre-existing-blockers discipline
- [x] 5.7 Check the externally reachable `*_for_test` count against the ceiling recorded in the matrix before adding any new seam

## 6. Documentation and gates

- [x] 6.1 `AGENTS.md`: attention-moment refresh among the key design decisions, including why no watcher and no staleness sweep
- [x] 6.2 `README.md`: the Features section gains palette and sidebar freshness against external changes — unconditional, this is user-visible behavior
- [x] 6.3 `docs/accessibility.md` and `docs/accessibility-matrix.md`: the collapsed-row expander and `Focus Folder` affordance change under a possibly-focused row; record the silent-update expectation and the affected rows
- [x] 6.4 `docs/workflow-readability-matrix.md`: re-derive the measured cells for `WFR-COMMAND-PALETTE` and the workspace-tree row, row-scoped and excluding `#[cfg(test)]` modules, and declare any new module in its row
- [x] 6.5 `docs/end-user-coverage.md`: the lane mapping for this behavior, including the delayed-filesystem lane
- [x] 6.6 No new rule: the change applied existing conventions (cross-cutting policy placement, evidence over test seams, reuse of bounded paths) rather than establishing one. The durable findings are recorded in `AGENTS.md` and in this change's `design.md`
- [x] 6.7 All green: `make check` (fmt, all-feature Clippy, filesystem boundary, Blueprint, workflow boundaries, accessibility policy, visual-proof policy, terminology), `cargo nextest --workspace` 1,837, property 56, **widget 1,255 in one attempt with no `FLAKY`**, the rustdoc lint gate, and `cargo check --workspace --benches`
- [x] 6.8 Verified in a real session against this repository as a workspace (**87,624 files**), with isolated XDG so no user app data was touched. A file created from a terminal in a deep, never-expanded directory became searchable **3.4 s** after the attention moment (87,624 → 87,625, two palette results). The readiness blockers observed were exactly `workspace-tree-refresh` and `command-palette-index` — the existing pair, confirming empirically that no new predicate was needed. Rapid re-triggering was refused twice and admitted on the third cycle once the adaptive interval elapsed. **Zero** GTK/GLib/GDK/Adwaita warnings across the session

## 7. Post-review corrections (`/simplify`, four angles)

- [x] 7.1 **Real defect, not style**: `started_at` was keyed by the workspace folder set and the settle re-resolved that key at the far end, so a scope change mid-flight stranded the old record `RefuseInFlight` permanently — and `evict_oldest_attention_record` never evicts an in-flight record, so the leak also wedged its own cap. Fixed by collapsing the map to one record that carries its own key, and settling without re-resolving the scope
- [x] 7.2 Palette open no longer rescans the sidebar tree. `AttentionSurfaces::IndexOnly` for the palette, `IndexAndTree` for window activation: the palette does not read the tree, and a full materialized-tree rescan per `Ctrl+Shift+P` is real filesystem work competing for the same worker slots as the rebuild the palette needs
- [x] 7.3 `queue_auto_full_refresh_for_attention` was a byte-for-byte copy of `refresh_for_visibility_change` one function above it. Collapsed into `refresh_folder_trees_silently`, and the sidebar-level fan-out is now the single one both the visibility subscription and the attention path call
- [x] 7.4 The chooser bracket became `ModalSurfaceGuard`, an RAII guard that survives an early return, and now covers the sidebar's Add Folder chooser and the print dialog — both of which the review found unguarded, which is the argument against per-call-site obligations made concrete
- [x] 7.5 **Rejected with reason**: deriving the guard from how long the window was inactive, instead of bracketing. It does not cover the case the guard exists for — during a Save As the absence is long by any threshold, and the window still reactivates before the save is queued
- [x] 7.6 `test_excluded_directory_stays_excluded_after_attention` waited on a condition that was already true, so its negative assertion ran while the refresh was still in flight. Now waits for the refresh to stop being in flight — and consequently joined the set of tests that fail against the unfixed code, 6 of 10
- [x] 7.7 Test helpers de-duplicated: `wait_until_or_false` and `active_editor` promoted to `tests/widget/common.rs` (the `polled` copy was the exact hand-rolled-wait-loop the repo has a rule against), `settle_index` alias dropped, two chooser tests merged, one doubled assertion and one fixed one-second sleep removed
- [x] 7.8 Policy simplified: `starts_refresh()` and its tautological test removed, the false "only RefuseThrottled passes on its own" justification corrected, and `RefuseNoWorkspace` added so the shell stops borrowing an unrelated refusal for the empty-scope case
- [x] 7.9 Deferrals recorded in `design.md`: no single `user_work_in_flight()` concept (this is the shell's fourth composite, with differing memberships), and no general "a file was written" notification for the index (audited every writer; no second live instance today)
- [x] 7.10 `design.md` Decision 6's evidence-surface commitment recorded as reversed, with the reason — the observable fact is a decision about one event, not retained state
