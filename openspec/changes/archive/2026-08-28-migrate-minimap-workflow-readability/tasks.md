> ## How to read this task list
>
> This is **slot 6**: `WFR-MINIMAP` alone, plus the incremental
> `WFR-AUTOMATION-SPINE` projection every slot since 2a has carried. The row has
> been deferred since the census for proof cost, not fit; the matrix's
> [Outlier Resolutions](../../../docs/workflow-readability-matrix.md) names the
> three reasons and forbids promoting it earlier "to get the big win first".
>
> **Sections are ordered by increasing risk.** Orientation and re-derivation, then
> the two convention amendments and their retroactive re-check, then the two
> path-keyed gates, then pure policy and its relocation, then seams, then the
> structural role move, then the evidence surface and seam retirement, then
> automation, then data safety, then the facade and the records, then verification.
>
> **Inherited decisions are verified, not re-litigated.** Where a task says
> "confirm", the expected outcome is a confirmation with its evidence *or* a
> recorded deviation with its reason. It is never "decide again".
>
> **Every count in this list is an upper bound in a named unit.** Where a task
> says "production", it means `#[cfg(test)]` modules excluded by brace tracking —
> including a co-located test module in its own file behind
> `#[cfg(test)] mod tests;`, which a naive per-file scan counts as production.
> Where it says "raw", it means `wc -l`. The two differ by 34% for this row's
> largest file, which is how slot 5b's B1 units error happened and why every
> figure below names its unit.
>
> **`[~]` marks a task deferred for the user**, not an unmet one. Live-display
> proofs are marked `[~]` from the start because they need a real desktop session
> and the user's availability; they are not silently dropped and not silently
> claimed.

## 0. Gates, orientation, and premise re-verification

- [x] 0.1 **Slot-5b gate — blocking.** Verify mechanically on a clean tree rather
      than reading it from the proposal: `openspec/changes/archive/` contains the
      slot 1, 2a, 2b, 3a, 3b, 4, 5a, and 5b changes; `openspec/specs/` holds
      `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
      `gtk-adapter-module-boundaries`, `mutation-testing`, and
      `dbus-automation-spine`; `docs/workflow-readability-matrix.md` marks the ten
      migrated rows `migrated` with complete `Migrated Workflow Roles` subsections
      naming paths that exist; `WFR-MINIMAP` is still `deferred`; the ledger in
      `docs/next/workflow-readability.md` marks slot 5b complete and slot 6
      outstanding with `WFR-MINIMAP, WFR-AUTOMATION-SPINE`; and
      `make check-workflow-boundaries` passes, reporting the current count of pure
      mutation-scoped policy modules. Record in A.1.
- [x] 0.2 Read, in this order: `docs/next/workflow-readability.md` end to end;
      this row's matrix row, its `WFR-MINIMAP` stage trace, the Outlier Resolution,
      the Policy Module Census row for `minimap_analysis.rs`, the reach-through
      table, the Facade size budget section, and the Measurement Definitions table;
      the four live specs; slot 5b's archived `tasks.md` Appendix B.2; and this
      change's `design.md`. Do not begin section 1 before this is done.
- [x] 0.3 **Re-derive the row's measured cells, row-scoped, with the unit named on
      every figure.** Upper bounds from authoring, to be confirmed or corrected in
      **either** direction:
      - `ui/editor_page/minimap.rs`: **3,779 raw / ≤2,510 production**;
      - `model/minimap_analysis.rs`: **186 raw / ≤121 production**;
      - row size: **≤2,631 production across 2 files**, against a matrix cell of
        3,965 which is **raw**;
      - seam functions: **11** `fn *_for_test` (reproduces the cell exactly);
      - seam gate sites: **≥21** `#[cfg(feature = "test-utils")]` — 16 in
        `minimap.rs` plus **5 `MinimapState` fields in `ui/editor_page/imp.rs`**
        (`analysis_slices`, `analysis_chars_per_slice_high_water`,
        `analysis_cancellations`, `analysis_terminals`,
        `analysis_after_slice_hook`), against a matrix cell of 16;
      - `minimap_analysis.rs` consumer count: the cell reads **1** and is an
        in-crate count. Re-derive it counting **external targets too**: the
        in-crate consumer `ui/editor_page/minimap.rs`, plus
        `crates/lushtext-core/benches/benchmarks.rs:44`–`:46`. Expected **≥2**.
        Record the **owning-workflow** count separately (**1**), because
        eligibility is counted in owning workflows and a benchmark target is not
        one — the two quantities were used interchangeably in this change's first
        draft and only the second decides the relocation;
      - `_for_benchmark` seams: authoring found **0** in this row; confirm, because
        5b's B.2 warns this class is invisible to a `test-utils` gate-site grep.
      Name any shared population the old cell pooled, with the rows that share it.
      Record in A.2 with the direction of every correction.
- [x] 0.4 **Reconcile the stage trace from the code, not from the census.** The
      census records **three** inversions; authoring identified **≥6 resumption
      points across ≥5 stage orders** and the count is a floor for the fifth
      consecutive slot. Enumerate every stage order, its entry points, its
      deferral primitive, and the point where control resumes. Authoring's floor,
      to be extended:
      1. availability classification and analysis eligibility;
      2. sliced content analysis — `glib::idle_add_local` at `minimap.rs:1123`,
         resuming in `run_minimap_analysis_slice(generation, lifetime)`;
      3. marker refresh — `MinimapState::refresh_debounce` (`Debounce`,
         `MINIMAP_REFRESH_DEBOUNCE` 80ms), resuming in `refresh_minimap`;
      4. width-reflow freeze/settle — `MinimapState::reflow_settle`
         (`SettleBurst`, `MINIMAP_REFLOW_SETTLE_DEBOUNCE` 150ms) resuming in
         `finish_minimap_reflow_settle`, **plus** that handle's
         `schedule_follow_up(MINIMAP_REFLOW_REVEAL_DELAY, ..)` 800ms reveal, **plus**
         the out-of-band early reveal `reveal_minimap_reflow_freeze_for_user_scroll`
         re-entering the same machine from a different actor, **plus** the passive
         `ViewportObserver` re-entry from `overscroll.rs:115`/`:121`;
      5. modified-line mark maintenance driven by four external workflows.
      Record in A.4 and correct the matrix's `Workflow Stage Traces` entry.
- [x] 0.5 **Enumerate the external entry surface**, because it is what the facade
      budget projection rests on. Upper bound from authoring: **≤18
      `pub`/`pub(crate)` operations called from ≤15 files outside the row**
      (`overscroll.rs`, `focus_mode.rs`, `search.rs`, `style_scheme.rs`,
      `bookmarks.rs`, `document_identity.rs`, `local_history.rs`,
      `load/execution.rs`, `save/execution.rs`,
      `buffer_replacement/execution.rs`, `mod.rs`, `imp.rs`,
      `ui/window/actions.rs`, `ui/window/documents.rs`,
      `ui/window/drafts/restore_execution.rs`, `ui/automation.rs`, `ui/theme.rs`).
      Record the list; §9.2's measurement is judged against it.
- [x] 0.6 **Record the row's behavior contracts verbatim, before any code moves**,
      so §10's acceptance compares against quoted text rather than memory: the
      `.agents/rules/ui.md` native-minimap animation-frame paragraph, the
      `.agents/rules/widget-wiring.md` opaque-cover sentence, the
      `.agents/rules/ui.md` minimap wrapped-layout O(1)-estimate and exact-2-MiB
      sentence, and the `.agents/rules/rust.md` sentence on
      `set_minimap_tracking_suspended` exactly restoring
      `imp().minimap.tracking_suspended`. Record in A.5.
- [x] 0.7 **Enumerate reach-throughs in and out of scope.** Production, from
      `ui/automation.rs`, matched on the **expression** not the line (5b's numbers
      have already moved): `editor.imp().scrolled_window` (`:1152` at authoring),
      `editor.imp().minimap_overlay` (`:1159`), `editor.imp().minimap.source_map`
      (`:1177`), `editor.imp().minimap.marker_strip` (`:1239`). Widget tests, in
      scope: `tests/widget/window.rs:2643`, `tests/widget/editor_page.rs:3827`,
      `:3838`, `:3845`, all reading `minimap_overlay`. Out of scope and named so:
      `window.imp().tab_view` (`WFR-SHELL-LAYOUT`, slot 7). Use a multi-line-aware
      search — 5b's same-line-only grep under-counted by 76%. Record in A.7.
- [x] 0.8 **Inventory the row's mutation configuration before touching it.** Record
      from the tool (`make mutants-list` filtered to the row), not from the source
      text: how many mutants `examine_globs`'s hand-listed `minimap.rs` entry
      currently generates; which of the 14 `exclude_re` entries' 66 method names
      match a real generated mutant; and confirm authoring's two findings —
      **7 named methods have zero definitions** (`apply_minimap_width_from_settings`,
      `wrapped_minimap_layout_exceeds_budget`,
      `buffer_has_line_exceeding_char_budget`, `collect_long_line_warnings`,
      `line_top_in_strip`, `line_bottom_in_strip`, `buffer_y_to_strip_y`) and
      **4 entries are anchored to `minimap.rs:2046:55`, `:2047:21`, `:2054:16`,
      `:2058:19` for `fit_projected_bounds`, which now begins at line 2435**.
      This inventory is §3's and §7's before-count. Record in A.3.
- [x] 0.9 **Mandatory `data-safety` pass in explicit mode, before implementing.**
      Five consecutive slots found at least one confirmed finding; budget for
      findings rather than hoping for none. Three named places to look, from the
      proposal: the live-buffer bounded cursor against the `buffer_snapshot` and
      O(1)-estimate rules; the modified-line mark operations driven by load, save,
      local-history restore, and draft restore; and
      `set_minimap_tracking_suspended`'s exact suspend/restore pairing. Confirmed
      findings are fixed in this change per
      `.agents/rules/preexisting-blockers.md`, which has no exceptions. Record in
      A.9 with a verdict per candidate.
- [x] 0.10 **Run the `gtk-perf-review` and `gtk4-libadwaita-internals` orientation**
      for the freeze/settle and native-source-map paths, and record any finding as
      a candidate rather than acting on it inside a structural move.
- [x] 0.11 **Confirm by path that no deferred item from an earlier slot has moved
      into this row's files**: slot 4's two `[~]`s and three B.3 simplify
      candidates, slot 5a's `[~]` live and manual proof, slot 5b's `[~]` live
      walkthrough, its unrun task 7.6 two-tree capture, its `scan_execution.rs`
      size follow-up, and its five handed-on non-tree data-safety findings.
      Confirm, do not assume.
- [x] 0.12 **`git add -N` every new path as soon as it exists**, before the first
      diff-aware gate. `make check-visual-proof-policy`,
      `make check-accessibility-policy`'s diff-aware half, and `make mutants-diff`
      build their file sets from `git diff <base>`, which does not list untracked
      paths at all — a change that adds whole new directories otherwise gets a
      **green** gate computed over a file set omitting all of its new code. This
      row adds a directory, so the hazard is certain, not hypothetical.

## 1. Apply the two convention amendments and pay the retroactive re-check

- [x] 1.1 Land the `workflow-readability-boundaries` delta: path-keyed mechanical
      gates are re-keyed or retired by the migration that moves their files, in
      every implementation of the predicate, proved by running the gate against the
      final state. Land the `mutation-testing` delta: hand-listed `examine_globs`
      entries retire with their workflow's migration and the retirement is
      measured, and line/symbol-anchored `exclude_re` entries are re-verified
      against real generated mutants whenever their file is touched.
- [x] 1.2 **Pay the retroactive re-check, all ten migrated rows, both statements.**
      The amendment rule requires re-migrating every already-migrated workflow to
      the amended shape in the same change; the cost here is bounded because the
      question is mechanical: *did this row's migration move a file that any
      checked-in path-keyed gate names?* Search `.cargo/mutants.toml`,
      `scripts/check-visual-proof-policy.py`,
      `crates/cargo-gtk-proof/src/policy.rs`,
      `scripts/check-accessibility-policy.py`,
      `scripts/accessibility_source_fingerprint.py`, and
      `scripts/check-filesystem-boundary.sh` for literal paths, and cross-check
      each against every migrated row's file set. **Expect a finding.** The
      not-a-confirmation streak is at five (3b found 1 of 3 rows non-compliant, 4
      found 2 of 4, 5a found 3 of 8, 5b found 8 gaps across 9 rows). Record the
      per-row result in A.6 including the confirmations.
- [x] 1.3 Where 1.2 finds a gate still keyed to a path an earlier migration moved,
      fix it in this change and record which slot's move left it. This is a real
      possibility: `ui/window/actions.rs` and `ui/window/imp.rs` both appear in the
      native-minimap predicate and both are `WFR-SHELL-LAYOUT`'s, unmigrated — but
      `ui/automation.rs` and `model/automation.rs` also appear and have been edited
      by five slots.
- [x] 1.4 Update `.agents/rules/build.md` so the path-keyed-gate obligation is
      standing guidance, not only spec text, and run `make check-agent-docs`.

## 2. The two path-keyed gates

- [x] 2.1 **Confirm the disarm before fixing it.** On a scratch branch, rename
      `ui/editor_page/minimap.rs` to `ui/editor_page/minimap/mod.rs` with no other
      edit, and demonstrate that (a) `make mutants-list` no longer generates the
      row's mutants, and (b) `make check-visual-proof-policy` no longer *requires*
      `native-minimap-highlight-anchors` or
      `native-minimap-animation-highlight-anchors` for a minimap-only diff, while
      both commands still exit 0. Discard the branch. This is the change's
      motivating evidence and must be observed, not argued.
- [x] 2.2 Re-key the native-minimap invariant predicate in **both**
      implementations: `scripts/check-visual-proof-policy.py:142` and `:168`, and
      `crates/cargo-gtk-proof/src/policy.rs:814` and `:842`. Replace the `==`
      literal-path match with a prefix match on
      `crates/lushtext-core/src/ui/editor_page/minimap/` so a later split inside
      the directory cannot disarm it again. Do not broaden the predicate to files
      it did not previously protect, and do not narrow it.
- [x] 2.3 **Add a parity assertion to each implementation separately.** There is
      no shared fixture between them and neither currently asserts a minimap path
      at all, so nothing today catches a one-sided re-key. Add an assertion in
      `scripts/check-visual-proof-policy.py`'s `run_self_tests()` (`:573`) **and**
      in `crates/cargo-gtk-proof/src/policy.rs`'s `#[cfg(test)] mod tests`
      (`:1008`), each asserting that a representative path under the new
      `ui/editor_page/minimap/` directory requires both
      `native-minimap-highlight-anchors` and
      `native-minimap-animation-highlight-anchors`. One assertion in one
      implementation is not parity coverage; it is half of it, and it is the half
      that would have passed while the other side was wrong.
- [x] 2.4 **Retire** the `.cargo/mutants.toml:35` hand-listed `examine_globs` entry
      rather than re-pointing it, and account for the mutants it used to generate
      per §3 and §7. Verify `ui/**/policy.rs` now reaches
      `ui/editor_page/minimap/policy.rs` — the nested/subdirectory role home
      requires this be **verified after the move**, not assumed.
- [x] 2.5 Prove both re-keyings by **running** the gates against the final staged
      tree in §10, not by reading the patch. Record in A.13.

## 3. Pure policy: relocate, then extract, and report the two separately

- [x] 3.1 Create `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` and
      relocate `model/minimap_analysis.rs` into it — `MinimapAnalysisPolicy`,
      `MinimapAnalysisResult`, `MinimapAnalysisAccumulator`, and its co-located
      tests. Confirm the census classification (**1 owning workflow**, pure, ≤121
      production lines) against 0.3's re-derived counts rather than against the
      cell.
- [x] 3.1a **Keep the benchmark compiling, per design §D6.** `mod minimap;`
      (`editor_page/mod.rs:21`) stays **private**, matching the `save/`, `load/`,
      and `buffer_replacement/` posture; the three analysis types join the
      existing `pub use` group at `editor_page/mod.rs:67`–`:72` as a narrow,
      precisely scoped re-export; `benches/benchmarks.rs:44`–`:46` is updated to
      the new path; and `pub mod minimap_analysis;` is removed from
      `model/mod.rs:23`. Do **not** widen `mod minimap;` to `pub` to solve this,
      and do not change what the benchmark measures. Verified by the bench-compile
      step in 10.7a, which exists because neither `make check` nor the nextest
      lane builds the bench target — CI's separate `Bench Compile` job would
      otherwise be the first thing to notice a hard compile error.
- [x] 3.2 **Relocation parity, measured.** Record generated and killed mutant
      counts for the relocated logic before and after, naming the exact invocation
      and the file-level anchors. A relocation whose mutants are no longer
      generated is a coverage regression that blocks the move, not accepted debt.
- [x] 3.3 Reify the scalar seam types design §D2 names, so `policy.rs` never needs
      a GTK import. **Two are established**: `MinimapProjectionSpace` and
      `MarkerProjectionSpace`, currently private structs in `minimap.rs`, promoted
      to pure seams. **One is a candidate, not a finding**: an adjustment-facts
      bundle over `{value, lower, upper, page_size}`. Review could not reproduce
      the claim that it crosses ≥2 boundaries — `fitting_source_map_page_size`
      takes two pairs drawn from **two different** adjustments, and
      `finite_adjustment_distance_from_lower` takes a pair, not the quadruple —
      and `MinimapAdjustmentDiagnostics` (`minimap.rs:293`–`:305`) already reifies
      the same five facts in milli units for the automation projection, so a
      second type over them would be a **duplicate shape** rather than a seam.
      Qualify it against the real call sites or **drop it**; if it qualifies,
      prefer extending or reusing the existing diagnostics type over introducing a
      parallel one. The rule is the same for all three: cross ≥2 function
      boundaries or be reconstructed at ≥2 call sites, or do not exist.
- [x] 3.4 Extract the scalar-domain math listed in design §D2 into `policy.rs`:
      `fit_marker_bounds`, `fit_projected_bounds`,
      `fit_native_slider_to_source_map_bounds`,
      `native_slider_estimate_from_inputs` with `NativeSliderEstimateInput`,
      `minimap_availability_for_policy` with `MinimapAvailabilityPolicy`,
      `wrapped_layout_analysis_required_for_bytes`,
      `source_map_editor_height_ratio_from_heights`,
      `wide_editor_slider_offset_class`, `fitting_source_map_page_size`,
      `finite_adjustment_distance_from_lower`, `normalize_line_runs`,
      `markers_from_lines`, `modified_line_mark_samples`, `marker_lane_width`,
      `marker_lane_x`, `marker_rgba`, `line_top_in_target`, `line_bottom_in_target`,
      `target_y_from_widget_y`, and `gtk_f64_to_milli`. Move every value
      **verbatim**; a threshold that changes during a structural move is a
      behavior change wearing a refactor.
- [x] 3.4a **`document_height_from_iter_rect` needs a re-signature *and* a
      rename.** Taking `(y, height)` scalars instead of a `gdk::Rectangle` is
      necessary for purity but not sufficient: the name would then describe a
      toolkit type the function no longer touches, and intent-first naming governs
      a `pub(crate)` cross-module operation. Rename it for the decision it makes —
      the document height derived from a line's vertical span — and update its call
      sites and its co-located tests. A pure function carrying a mechanism name
      from the adapter it left is the naming defect this rule exists to prevent.
- [x] 3.5 Move the policy constants whose value is a product decision:
      `MINIMAP_MARKER_STRIP_WIDTH`, `MINIMAP_TOP_CONTENT_MARGIN`,
      `MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD`, `MINIMAP_LONG_LINE_WARNING_THRESHOLD`,
      `MINIMAP_SEARCH_MATCH_CAP`, `MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET`,
      `MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET`, `MINIMAP_ANALYSIS_CHARS_PER_SLICE`,
      `MINIMAP_LONG_LINE_MARK_CAP`, `MINIMAP_MARKER_MIN_HEIGHT`,
      `MINIMAP_VIEWPORT_MIN_HEIGHT`, `MINIMAP_VIEWPORT_HORIZONTAL_OUTSET`,
      `MINIMAP_MODIFIED_LINE_MARK_CAP`, and the three `Duration`s. Keep
      `MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS` and
      `MINIMAP_MODIFIED_MARK_CATEGORY` with the coordination that uses them if
      `policy.rs` has no decision that reads them. Preserve
      `test_minimap_policy_constants_are_stable` and extend it to every relocated
      constant.
- [x] 3.6 **Extraction gain, measured and reported separately from 3.2.** An
      extraction out of an adapter has no before-count and cannot fail; merging it
      with the relocation figure would let a parity loss hide behind a gain, which
      the mutation-testing capability exists to prevent. Name the invocation and
      the file-level anchors for each figure.
- [x] 3.7 **Budget a unit test for every extracted function that lacks one.** 5b's
      `workspace_scope_kind_name` arrived with 2 survivors because its only
      assertions lived in a *widget* test, outside the mutation lane's test
      surface. The row's 1,241-line co-located module already unit-tests most of
      this math; identify the gaps against the extracted list before running §7.
- [x] 3.8 Run `make check-workflow-boundaries` and confirm it reports `policy.rs`
      pure and inside the mutation scope. A `gtk4`/`glib`/`gio`/`libadwaita`/
      `sourceview5` import fails naming the file and the import; treat that as the
      cheap early signal that §D2's line was drawn wrongly.
- [x] 3.9 **Probe for a cross-cutting negative finding and record it either way.**
      Confirm no extracted decision has an owning workflow other than this row —
      the eligibility threshold, the marker cap, and the lane geometry are all
      minimap product decisions. If one does, it stays shared and the matrix
      records it as cross-cutting; never fork a shared limit to manufacture a local
      `policy.rs`.
- [x] 3.10 Confirm the extraction lands **nothing** in `gtk-lush-viewport` or
      `gtk-lush-widgets`. Those are leaf crates that must not depend on LushText,
      and this arithmetic encodes LushText product decisions. Stated as a task
      because the math looks generic enough to tempt the opposite.

## 4. Seams

- [x] 4.1 Confirm `MinimapAnalysisSession` remains an adequate seam value object.
      It already carries `{generation, lifetime}`, and the convention treats a
      coordinator owning its generation and exposing a currency predicate as
      *being* the seam value object. Do not re-derive it away.
- [x] 4.2 Audit every cross-module signature in the row against the seam rule: a
      bundle crossing ≥2 function boundaries or reconstructed at ≥2 call sites is
      reified; a bundle used by exactly one private helper is not. Candidates
      beyond §3.3: the marker `{kind, start_line, end_line}` triple already reified
      as `MinimapMarker`, and the freeze-choreography `{freeze_rendered_map}` flag
      threaded through `schedule_minimap_reflow_settle_impl`.
- [x] 4.3 **Check for a value renamed while crossing a seam** — the archetype
      defect the rule exists to make unrepresentable. The row's coordinate spaces
      are the live hazard: `minimap.rs` mixes source-map widget coordinates,
      marker-strip coordinates, editor buffer coordinates, target-widget
      coordinates, and snapshot coordinates, several of them as bare `f64`. Any
      site passing one space's value into a parameter named for another is a type
      error after §3.3 or a finding now.
- [x] 4.4 **Plan zero *new* test-only actuation seams.** Slot 5b's budgeted one is
      still unspent and this change does not intend to spend it. If implementation
      finds one genuinely unavoidable, justify it individually at its definition
      per the deferred-seam taxonomy in the `gtk-testing` skill; do not spend it
      for convenience.
- [x] 4.5 **Classify the row's *existing* actuation seams, which "zero new" does
      not cover.** The row has **two**, and both must have a stated disposition
      rather than being carried silently past a consolidation that only names
      inspection seams:
      - `mark_minimap_refresh_pending_for_test` (`minimap.rs:531`) — the 11th seam
        and the only one that **mutates** (`refresh_pending.set(true)`). It has
        three call sites: `tests/widget/editor_page.rs:2760` and
        `tests/widget/window.rs:4588`/`:4644`, all readiness-blocker assertions.
        It cannot move onto `evidence.rs`, because an evidence surface must not
        mutate. Decide and record one of: **retired** in favour of a real
        production drive that leaves a refresh genuinely pending; **kept and
        justified** at its definition under the deferred-seam taxonomy, naming why
        no production drive reaches the state; or **recorded deferred** with its
        owner. Silence is not one of the options.
      - `set_after_minimap_analysis_slice_hook_for_test` (`minimap.rs:444`) with
        its storage `analysis_after_slice_hook` (`imp.rs:363`) — an injected
        stale-generation transition. Same three-way disposition, and note that 6.8
        requires its override storage to share the row's one test policy value.
- [x] 4.5 Confirm zero `#[expect(clippy::too_many_arguments)]` in the row before
      and after. The workspace count is 1 and the survivor is
      `model/action_catalog.rs`'s domain constructor. A new suppression on a
      cross-module workflow boundary is an unreified seam, not an exception.

## 5. Implement the role home, the coordination roles, and the facade

- [x] 5.1 Create `crates/lushtext-core/src/ui/editor_page/minimap/` with `mod.rs`
      as the narrative facade. Confirm design §D1's reasoning against the tree
      rather than assuming it: `ui/editor_page/` hosts eight workflows, `save/`,
      `load/`, and `buffer_replacement/` already took subdirectories, and the fixed
      `policy.rs` / `evidence.rs` names cannot be shared.
- [x] 5.2 Create the six coordination modules design §D3 assigns, each with a
      module doc naming its role and its stage order: `admission.rs`,
      `analysis_execution.rs`, `projection_execution.rs`, `reflow_execution.rs`,
      `watch.rs`, `retirement.rs`. Three stage-order-qualified `execution` modules
      is permitted by the convention, not a workaround; record the qualification
      reason in each module doc.
- [x] 5.3 **Keep the freeze/settle/reveal machine in one module.** Design §D3
      rejects giving the cover's removal to `retirement`, because
      `minimap.rs:938`'s
      `if minimap.reflow_settle.pending() || !minimap.reflow_reveal_pending.get()`
      guard and its setter must not land on opposite sides of a role boundary.
      Confirm after the move that `freeze_native_minimap_for_reflow`,
      `warm_live_minimap_under_reflow_freeze`,
      `reveal_minimap_reflow_freeze_for_user_scroll`, `drop_minimap_reflow_freeze`,
      `minimap_reflow_freeze_visible`, `finish_minimap_reflow_settle`, and
      `schedule_minimap_reflow_settle_impl` are all in `reflow_execution.rs`.
- [x] 5.4 **Preserve both freeze entry actors as two named operations.**
      `schedule_minimap_reflow_settle_with_freeze` (user action, from
      `ui/window/actions.rs:596`) and `schedule_minimap_reflow_settle` (passive
      `ViewportObserver`, from `overscroll.rs:115`) differ by one boolean into a
      shared implementation, and `.agents/rules/ui.md` states that difference as a
      **behavior contract**. Both stay `pub(crate)`, both carry the contract in
      their doc comments, and the facade narrates why there are two.
- [x] 5.5 **Move `debug_assert!(render_hold.live_child().opacity() >= 0.99)`
      (`minimap.rs:926`) verbatim, with its guard.** It is a load-bearing opacity
      contract implementing the opaque-cover rule quoted in 0.6, and the workspace
      denies `debug_assert_with_mut_call`, so it must not acquire a mutating call
      during the move.
- [x] 5.6 **Classify `MinimapState` (`ui/editor_page/imp.rs:328`–`:388`) as a
      called presentation surface**, per design §D4, and record it in **both**
      places the convention requires: its own module doc, and the matrix row. Slot
      5a's re-check found three of eight rows meeting only half of that
      requirement. Confirm the `dispose()` teardown order at `imp.rs:660`–`:680`
      is unchanged.
- [x] 5.7 Confirm the row owns exactly one `policy.rs` and one `evidence.rs`, and
      that no called presentation surface owns either.
- [x] 5.8 Update the module declaration and the three re-export groups at
      `ui/editor_page/mod.rs:21` and `:67`–`:72`, and every `use`/path line in the
      ≤15 caller files from 0.5. Expect `super::` path renames to dominate the
      diff; verify at least one preservation anchor's production diff explicitly so
      a real edit cannot hide among them (5b's `row_factory.rs` lesson).
- [x] 5.9 Confirm no new crate, trait, manager type, or indirection layer was added
      to move code, and that the split is by role rather than by line count.

## 6. Evidence surface and seam retirement

- [x] 6.1 Create `minimap/evidence.rs` by extending the existing gated
      `MinimapAnalysisSnapshot` (`minimap.rs:205`–`:228`, eleven fields) into the
      row's single evidence surface. Absorb the **seven** inspection accessors the
      row currently exposes as separate `*_for_test` functions — seven, not six:
      an earlier draft of this change miscounted the list it then enumerated
      correctly, and the row's remaining four seams are the two actuation seams in
      4.5, `minimap_analysis_snapshot_for_test` (which becomes the surface), and
      `minimap_analysis_slice_limit_for_test` (which becomes test policy per 6.8),
      accounting for all eleven:
      `long_line_warning_count_for_test`, `minimap_viewport_bounds_for_test`,
      `minimap_first_content_row_bounds_for_test`,
      `minimap_reflow_settle_pending_for_test`, `minimap_work_pending_for_test`,
      `minimap_projection_attached_for_test`, and
      `wrapped_layout_analysis_required_for_test`. A new per-field inspection
      function is a regression back to the shadow API the surface replaced.
- [x] 6.2 **Discharge the no-materialization statement.** The surface MUST NOT call
      an accessor that lazily creates toolkit state, and MUST NOT advance a metric
      it reports. The live hazards here are specific: reading marker bounds calls
      into GTK text-iter and source-map layout, which can force a layout pass;
      `minimap_work_pending` reads the `Debounce` and `SettleBurst`, which must not
      be re-armed by observation; and the analysis counters
      (`analysis_slices`, `analysis_cancellations`, `analysis_terminals`,
      `chars_per_slice_high_water`) are metrics the surface itself reports.
      **Prove it, do not assert it**: read the surface with analysis idle and with
      analysis mid-slice, and show every counter, generation, pending flag, and
      cached-character total identical before and after each read.
- [x] 6.3 **Discharge the disposed-widget statement.** GTK4 clears template
      children in `dispose()` before Rust's `Drop`, and the surface derives fields
      from `minimap_overlay`, `source_map`, and `marker_strip`. Every such field
      reads through `try_get()` or the `RefCell<Option<..>>` it already lives in,
      and answers honestly when the child is gone. A panicking accessor turns a
      teardown observation into a crash.
- [x] 6.4 **Discharge the reentrancy statement with the driven proof the convention
      requires**, in the shape of
      `editor_page::test_load_evidence_reads_stay_side_effect_free_across_load_mutation`:
      drive the workflow through each operation that takes a mutable borrow of the
      state the accessor reads — `refresh_minimap`, `run_minimap_analysis_slice`,
      `cancel_minimap_analysis`, `record_modified_lines`,
      `finish_minimap_reflow_settle` — read the surface **after** each one, and
      assert repeated reads of unchanged state are identical. Do **not** write a
      test that reads the surface while a borrow is held: that is the panic the
      constraint prevents, not a demonstration of it. Compute every derived scalar
      and drop each `Ref` before building the struct literal, and record the
      constraint in the module doc.
- [x] 6.5 **Discharge the bounded-aggregate statement.** Any field aggregated over
      a variable-sized set — marker counts per kind, marker bounds, modified-line
      marks — must be bounded (the row's caps are `MINIMAP_SEARCH_MATCH_CAP`,
      `MINIMAP_LONG_LINE_MARK_CAP`, `MINIMAP_MODIFIED_LINE_MARK_CAP`), must answer
      honestly at zero, and must skip a disposed child rather than panicking.
- [x] 6.6 Retire the widget-test reach-throughs from 0.7
      (`window.rs:2643`, `editor_page.rs:3827`/`:3838`/`:3845`) onto the evidence
      surface or a named facade operation. An ungated `.imp()` read from a test
      shapes a production signature without appearing in any seam census — slot
      2a's palette lesson.
- [x] 6.7 Re-derive the row's seam census after retirement and record the
      before/after in the matrix cell, both figures in named units. Upper bound
      before: **11 fns / ≥21 gate sites**.
- [x] 6.8 Collapse any test-only timing or limit override into the row's **one**
      test policy value; no override storage may compile without the test feature.
      The row's current gated hook is `analysis_after_slice_hook`
      (`imp.rs:363`) with `minimap_analysis_slice_limit_for_test`; confirm there is
      exactly one home for both after the move.
- [x] 6.9 Confirm `evidence.rs` is an internal type at the narrowest visibility its
      readers need, with a `#[cfg(feature = "test-utils")]` re-export for the
      external widget harness — the pattern the exemplar and the palette both use
      unchanged. It is never added to the public D-Bus schema.

## 7. Mutation configuration: retire, delete, and re-derive

- [x] 7.1 Delete the four `exclude_re` entries anchored to `minimap.rs:2046:55`,
      `:2047:21`, `:2054:16`, `:2058:19`. 0.8 establishes they match nothing.
      State, per the amended requirement, whether the `fit_projected_bounds`
      mutants they were written for still exist; if they do, triage them in the
      documented order rather than writing a new anchored entry.
- [x] 7.2 Delete the seven dead method names from the two adapter-method entries.
- [x] 7.3 Retire the two `LushtextEditorPage::(..)` adapter-method entries entirely
      once the coordination roles exist: the mutation-testing capability already
      states that a workflow whose pure policy is separated by module does not need
      `exclude_re` entries enumerating adapter method names, because the adapter is
      out of scope for not being a policy module.
- [x] 7.4 Retire the two GTK-wrapper entries (`current_availability` … and
      `sync_source_map_geometry` …) on the same reasoning, keeping only entries
      that name a **real, still-generated** mutant in `policy.rs` with a documented
      equivalence reason.
- [x] 7.5 Re-derive every surviving exclusion against a mutant from
      `make mutants-list`, not against source text. Target: the row's 14 entries
      and 66 method names reduce to the small set of genuine equivalence claims in
      `policy.rs` — chiefly the `fit_marker_bounds` and
      `fit_native_slider_to_source_map_bounds` boundary-equality mutants, re-stated
      against the new file and re-verified.
- [x] 7.6 Update `docs/workflow-readability-matrix.md`'s Measurement Definitions
      rows for `exclude_re` entries (71 at authoring) and Minimap mutation
      exclusions (14 entries / 66 method names / 17 physical TOML lines) with the
      post-retirement figures.
- [x] 7.7 Triage every survivor to zero, or record each remaining one with the
      documented order: is it a real missed behavior; then tighten tests; then
      consider a small refactor; and only then an equivalence exclusion narrow
      enough that nearby behavior still mutates.

## 8. Automation: project from evidence without widening

- [x] 8.1 Retire the four production `ui/automation.rs` reach-throughs from 0.7 by
      giving each a named facade operation or an evidence field. `scrolled_window`
      and `minimap_overlay` are the editor page's template children read from
      *another* workflow; `minimap.source_map` and `minimap.marker_strip` are this
      row's state read the same way. All four cross a workflow boundary.
- [x] 8.2 Project the minimap automation fields from `evidence.rs`. The affected
      surface is large and must be enumerated before it is touched:
      `surfaces.minimap_requested`, the whole
      `visual_geometry.native_minimap` object (≥18 documented fields including
      `projection_source`, `source_map_allocation`, `source_map_rect`,
      `editor_visible_rect`, `source_map_visible_rect`, both `*_vadjustment`
      summaries, both document heights, `border_left`/`border_right`,
      `native_slider_estimate`, `native_slider_visible_bounds`,
      `line_projection_rect`, `first_content_row_rect`), the four
      `visual_geometry.pixel_anchors` minimap entries, the
      `minimap-source-map` and `minimap-native-viewport` surface names, and the
      `minimap-refresh` readiness blocker and workflow id.
- [x] 8.3 **Prove no widening, byte-identically.** The exported schema and the
      minimap projection functions must be unchanged; every new evidence field must
      be declared on the surface and read by no projection; and a before/after
      Automation1 capture of the same app state must diff the
      `visual_geometry.native_minimap` object, the `pixel_anchors` array, the
      `surfaces.minimap_requested` field, and the readiness fields to **zero**
      differences. This is the shape slot 2b established and 5b re-used.
- [x] 8.4 Register the projection with the drift gate so a later change cannot
      break the evidence→snapshot link silently, and extend drift coverage as the
      convention requires when a workflow migrates.
- [x] 8.5 Run `make check-automation-docs`; update `docs/automation.md` and
      `docs/automation-reference.md` only if a field's *meaning* or *owner* changed.
      If nothing changed, say so explicitly — a no-widening slot's docs delta is
      legitimately empty and that is a result, not an omission.
- [x] 8.6 Confirm `READINESS_BLOCKER_MINIMAP_REFRESH` and
      `AUTOMATION_WORKFLOW_MINIMAP_REFRESH` keep their exact string values
      (`"minimap-refresh"`) and their membership in the `visual-geometry-settled`
      predicate's blocker list. Six visual-geometry scenarios wait on that
      predicate; changing it would invalidate the row's own proof lane.

## 9. Facade, matrix, and record completion

- [x] 9.1 Write `minimap/mod.rs` as the narrative facade: the ≥5 stage orders from
      0.4 narrated with their intent named, every stage delegated, and **each
      inversion documented with the point where control resumes** — the
      `idle_add_local` slice, the marker `Debounce`, the `SettleBurst`, its
      follow-up reveal, the out-of-band user-scroll reveal, and the passive
      `ViewportObserver` re-entry. The facade owns no timer, no admission
      bookkeeping, no generation counter, and no GTK widget mutation.
- [x] 9.2 **Measure the facade and judge it against 370.** Projection is ≈300 with
      a credible worst case near 340; the record names this row as most likely to
      prove the number wrong, so treat either outcome as informative. If it
      exceeds 370, follow the escalation path declared in the proposal: delegate
      harder first; amend the budget with the retroactive re-check only if that
      fails; never move stage narration into a coordination module and never split
      the census row. Record the measurement and the margin in A.11 with the
      stage-order and entry-point counts beside it, because that is the datum the
      programme is accumulating.
- [x] 9.3 **Run the rustdoc gate by hand before shipping the facade.** It is
      CI-only and `make check` does not run it. A narrative facade in a `pub`
      module naturally wants to link its own private coordination modules and
      `pub(crate)` seam types, and every such link is a
      `rustdoc::private_intra_doc_links` error. The fix is **always** to drop the
      link and keep the name in backticks; never widen visibility to satisfy
      documentation. This class has shipped three times.
      ```
      RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" \
        cargo doc --workspace --no-deps
      ```
- [x] 9.4 Update the `WFR-MINIMAP` matrix row to `migrated`, with its
      `Migrated Workflow Roles` subsection naming every role path, the called
      presentation surface, the role-home choice and its reason, the seam value
      objects, and the evidence surface.
- [x] 9.5 **Re-derive the measured cells as the very last documentation step**,
      after the final test and mutation runs — not when the role files were
      written. Slot 4 found three of four cells had drifted during the change
      itself, because survivor triage adds tests and occasionally moves production
      lines. Name the unit on every figure.
- [x] 9.6 Correct the matrix's `Workflow Stage Traces` entry for this row from
      three inversions to the re-derived figure, and correct the Outlier
      Resolutions entry to record the row as resolved rather than deferred.
- [x] 9.7 Advance `docs/next/workflow-readability.md`: the status line, the
      baseline, the remaining-scope table row for slot 6, the slot ledger line to
      `- slot 6 (complete): WFR-MINIMAP, WFR-AUTOMATION-SPINE (partial)`, and a
      "Convention friction slot 6 hit, recorded for slot 7" section. Confirm the
      ledger's `(partial)` semantics: `WFR-AUTOMATION-SPINE` continues into slot 7,
      so marking it `migrated` to satisfy the gate would be a false claim.
- [x] 9.8 Run `make check-workflow-boundaries` and confirm the matrix and the
      ledger agree, which the gate enforces.
- [x] 9.9 Verify — do not edit — `docs/accessibility-matrix.md`'s
      `A11Y-EDITOR-MINIMAP` row. It already names the owner path-agnostically as
      `ui/editor_page/minimap`, so the directory move should leave it correct. If
      it does not, that is a finding.
- [x] 9.10 Update `.agents/rules/rust.md`, `.agents/rules/ui.md`,
      `.agents/rules/widget-wiring.md`, and `.agents/rules/build.md` for any path
      or symbol this change moved, **plus these four maintained documents that name
      a moved path and were absent from the first draft's Impact list**:
      - `AGENTS.md:70` (the `model/minimap_analysis.rs` module-layout line) and
        `:118` (the `editor_page/` line, which names `load/`, `save/`, and
        `buffer_replacement/` as the per-workflow role homes and lists minimap as a
        loose "helper alongside" — that sentence becomes wrong when minimap becomes
        the fourth role home);
      - `README.md:438`–`:439` (`model/{workspace_search,plain_disposal,minimap_analysis}.rs`);
      - `crates/lushtext-core/src/ui/editor_page/AGENTS.md:24`–`:25` ("minimap
        behavior in `minimap.rs`") and `:43` ("must come from the current accepted
        `model::minimap_analysis` cache");
      - `docs/mutation-testing.md:161` (a table row keyed on the live
        `ui/editor_page/minimap.rs` path) and `:177`–`:194` (the June 2026 minimap
        baseline of 215 → 86 missed mutants and the "narrow documented exclusions"
        sentence, both of which §7's retirement makes stale — restate the baseline
        with §7's post-retirement figures rather than deleting the history).
      `crates/gtk-lush/viewport/README.md:24` mentions minimap refreshes as
      adoption evidence, names no path, and needs no edit; recorded so the omission
      is not later read as a miss. Then run `make check-agent-docs`. Deliberately
      prefer naming the owner without a file path where the rule allows it —
      migrations rename owners, and 5b found a rule naming a function that had
      never existed, unchecked across three documents and two changes.
- [x] 9.11 Confirm no maintained guidance, skill reference, or archived-change
      pointer names `crates/lushtext-core/src/model/minimap_analysis.rs` or
      `crates/lushtext-core/src/ui/editor_page/minimap.rs` as a live path. Archived
      changes keep their historical text; live guidance does not.
- [ ] 9.12 Rewrite this change's own evidence pointers to archive form at archive
      time — the step four prior changes missed and 5b called out.

## 10. Verification

Run in this order. Smoke lanes run **last**, from clean artifact roots, against
the **final staged** tree.

- [x] 10.1 `make check` — rustfmt, all-feature Clippy, fast policy audits. Zero.
- [x] 10.2 **Both feature configurations.** `--all-features` **hides** lints the
      default-feature build errors on; origin/main once did not compile under
      default features while `make check` was green. Run
      `cargo check -p lushtext-core --lib` as the true default-feature gate, and
      `cargo clippy --workspace --all-targets --all-features -- -D warnings` as the
      blocking one. This row narrows a `test-utils`-gated surface, which is exactly
      the shape that breaks one configuration and not the other.
- [x] 10.3 The rustdoc gate from 9.3. Zero.
- [x] 10.4 `make check-policy` — includes `check-workflow-boundaries`,
      `check-visual-proof-policy`, `check-accessibility-policy`,
      `check-automation-docs`, `automation-client-self-test`, and the GTK Lush
      policy lanes. Zero.
- [x] 10.5 `make check-agent-docs`.
- [x] 10.6 `make check-filesystem-boundary` — the row touches no file I/O, so a
      clean run is the expected confirmation.
- [x] 10.7 Full non-widget test suite via nextest. Record the count.
- [x] 10.7a **Bench compile — `cargo bench --no-run` (or
      `cargo check --workspace --all-targets`).** Neither `make check` nor the
      nextest lane builds `crates/lushtext-core/benches/benchmarks.rs`, and CI's
      `Bench Compile` is a separate job, so a green local battery can still hide a
      hard compile error. This is not hypothetical for this change: the bench is
      the relocated module's second consumer (0.3, 3.1a, design §D6), and its
      import path moves. Run it immediately after 3.1a lands, not only here.
- [x] 10.8 **Widget lane at `--retries 0`, zero `FLAKY`.** A `FLAKY:` line is a
      blocker under `.agents/rules/preexisting-blockers.md`, not accepted noise:
      classify the wait, give async/realization waits a ≥5–10s budget, fix the
      predicate or the production race, and rerun **in isolation**. Use the shared
      `wait_until` / `flush_events` / `flush_after_delay` / `present_window` from
      `crates/lushtext/tests/widget/common.rs`; do not copy one into a module.
- [x] 10.9 **Mutation, with 5b's working-tree workaround.** `make mutants-diff`
      builds its diff from a three-dot **commit range**, so working-tree edits are
      invisible and it exits 0 having tested zero mutants; `git add -N` does not
      fix it. Generate the diff and pass it explicitly. Do not edit any file in the
      mutation scope while a run is in flight — a mid-run `cargo fmt` shifts line
      numbers and produces false MISSEDs. Report §3.2's relocation parity and
      §3.6's extraction gain as separate figures, each naming its invocation and
      anchors. Note that `--re` does not bound a run; `--in-diff` does.
- [x] 10.10 `make test-prop` — the row contributes no property target; confirm the
      lane is unaffected.
- [x] 10.11 **`make visual-geometry-smoke`, from a clean
      `build/smoke/visual-geometry` root, against the final staged tree.** This is
      the row's primary acceptance lane, not a formality. It must produce an
      unfiltered summary whose `pixel_verified_invariant_ids` include **both**
      `native-minimap-highlight-anchors` and
      `native-minimap-animation-highlight-anchors`, with per-case pixel rows,
      final-frame rendered-anchor stability, and final sidebar/editor/minimap
      geometry. **Staging a rename changes the diff digest the gate fingerprints**
      (5b's ship lesson), so if `git add` moves the digest after the run, re-run
      the lane — do not unstage to keep the earlier green.
- [x] 10.12 Confirm the six `minimap-sidebar-workspace-animation` case ids from
      `check-visual-proof-policy.py:44`–`:49` are present and passing:
      compact-overlay, intermediate-1100sp, and wide-desktop, at both `--show` and
      `--hide`. Minimap drift is an animation-frame invariant; a final-settle-only
      pass would not detect a freeze revealed one frame early.
- [x] 10.13 Confirm the other minimap geometry scenarios named in
      `docs/accessibility-matrix.md` still pass: `minimap-sidebar-top`,
      `minimap-sidebar-mid`, `minimap-sidebar-live-threshold`, and
      `minimap-sidebar-dense-markdown-top`.
- [x] 10.14 `make accessibility-smoke` — the `minimap-transition` case must still
      prove the editor remains the semantic text target while minimap state
      changes, against the anchor
      `atspi-anchor-minimap-transition-text-editor-for-accessibility-smoke-txt`.
      From a clean root.
- [x] 10.15 `make automation-smoke` and `make visual-smoke`, from clean roots, with
      the runtime log scanned for unexpected GTK/GDK/Adwaita/GIO/D-Bus warnings.
- [x] 10.15a **`make performance-smoke` — and check its two string keys survive
      §5's split.** This lane is string-keyed against this row in exactly the way
      §D1's gates are path-keyed, so it can break silently:
      `scripts/run-performance-smoke.sh:273`–`:274` filters on two **literal
      widget-test names**,
      `editor_page::test_minimap_long_line_warning_scan_slices_large_many_short_buffer`
      and `editor_page::test_minimap_mid_scan_edit_cancels_stale_generation_and_publishes_latest`,
      and `:284` greps the emitted evidence for
      `minimap-(analysis|cancellation)-evidence`. If §5's `analysis_execution`
      split relocates or renames either test, or 6.1's consolidation changes an
      evidence label, the filter matches nothing and the lane reports success over
      an empty run. Either keep both names and both labels, or update the script in
      the same change — and prove it by confirming the lane's summary actually
      contains the grepped lines, not merely that the command exited 0.
- [x] 10.16 `make builder-diagnostics-smoke` — the row touches no `.blp`, so a
      clean run or a documented unsupported-runtime skip is the expected result.
- [x] 10.17 **Cold read.** Have a reader who did not write the change answer five
      questions from `minimap/mod.rs` alone: what starts the workflow; what decides
      whether analysis may run; what happens between the sidebar starting to
      animate and the live map becoming visible again; why there are two ways to
      schedule a reflow settle; and where a stale analysis slice is rejected. A
      question that cannot be answered from the facade is a facade defect.
- [x] 10.18 Tail `simplify` pass after full verification, applied only to code this
      change wrote.
- [~] 10.19 `[~]` **Live-display proof — deferred for the user.** `make run`
      against a restored workspace with the minimap visible: toggle the sidebar
      repeatedly while watching stderr for `Trying to measure GtkBox ...`,
      `pixman_region32_init_rect`, `Gtk-CRITICAL`, and
      `GLib-GObject-WARNING`; confirm the freeze reveals only after the settle;
      confirm `Ctrl+Shift+M` toggles cleanly; and confirm no minimap slider drift
      is visible to the eye across a full show/hide cycle. Widget green plus a live
      warning is a failed fix, not a partial success. Recorded `[~]` from the start
      because it needs the user's desktop session, and awaiting their decision.
- [~] 10.20 `[~]` **Manual Orca check — deferred for the user**, per
      `docs/accessibility-orca-checklist.md` for `A11Y-EDITOR-MINIMAP`: Orca
      reports the minimap toggle state and mode, not minimap internals.

## 11. Handoff

- [x] 11.1 Confirm `docs/workflow-readability-matrix.md` and
      `docs/next/workflow-readability.md` agree, and that
      `make check-workflow-boundaries` enforces it.
- [x] 11.2 Write Appendix B.2 for **slot 7**, covering at minimum: every figure
      corrected by 0.3 with its direction and unit; the re-derived stage-order and
      inversion counts; the facade measurement and whether 370 held; the two
      path-keyed gates and **which files slot 7 owns that still appear in the
      native-minimap predicate** (`ui/window/actions.rs` and `ui/window/imp.rs`,
      both `WFR-SHELL-LAYOUT`'s); the mutation-configuration retirement totals; the
      `data-safety` verdicts with owners; whether the actuation-seam budget is
      still unspent; the retroactive re-check result from 1.2; and every `[~]`
      item with its reason. State plainly which parts of the row are migrated and
      which follow-ups are recorded rather than hidden.
- [x] 11.3 Record whether `WFR-MARKDOWN-PREVIEW` — slot 7's largest row and the
      other row the matrix flags for pixel-visible risk — inherits anything from
      this slot's pixel-preservation method.

## Appendix A — orientation record

Populated during implementation. Each section is written when its tasks complete,
except A.11, whose figures are re-derived last per 9.5.

### A.1 Gate evidence (task 0.1)

Verified mechanically on a clean tree, not read from the proposal.

- `openspec/changes/archive/` holds all eight slot changes:
  `2026-08-25-complete-search-replace-workflow-readability` (2b),
  `2026-08-25-migrate-command-palette-workflow-readability` (2a),
  `2026-08-26-migrate-document-load-workflow-readability` (3b),
  `2026-08-26-migrate-document-save-workflow-readability` (3a),
  `2026-08-27-migrate-user-content-restore-workflow-readability` (4),
  `2026-08-27-migrate-workspace-tree-and-notes-workflow-readability` (5a),
  `2026-08-28-migrate-workspace-tree-workflow-readability` (5b), and slot 1's
  `2026-08-25-normalize-workflow-readability-boundaries`.
- `openspec/specs/` holds `workflow-readability-boundaries`,
  `workflow-evidence-surfaces`, `gtk-adapter-module-boundaries`,
  `mutation-testing`, and `dbus-automation-spine`.
- `WFR-MINIMAP` was `deferred`; the ledger marked slot 5b complete and slot 6
  outstanding with `WFR-MINIMAP, WFR-AUTOMATION-SPINE`.
- `make check-workflow-boundaries` passed on the clean tree, reporting **10**
  pure mutation-scoped policy modules. After this change it reports **11**.

### A.2 Premise re-verification, row-scoped, with the unit and direction of every correction (task 0.3)

Production means `#[cfg(test)]` items excluded — both the co-located `mod tests`
and the three free `#[cfg(test)] fn` helpers a naive brace-tracking scan counts as
production. Raw means `wc -l`.

| Figure | Matrix cell | Re-derived | Direction |
| --- | --- | --- | --- |
| `minimap.rs` | 3,779 (raw, unlabelled) | **2,509 production / 3,779 raw** | cell was raw; production is **34% smaller**. Authoring's ≤2,510 bound confirmed |
| `minimap_analysis.rs` | 186 (raw, unlabelled) | **120 production / 186 raw** | same. Authoring's ≤121 bound confirmed, one line lower |
| Row size | 3,965 (raw, 2 files) | **2,629 production across 2 files** | restated with the unit named |
| Seam functions | 11 | **11** | reproduces exactly |
| Seam gate sites | 16 | **21** | **under-counted by 5** — confirmed. 16 in `minimap.rs`, plus 5 `MinimapState` fields in `ui/editor_page/imp.rs` (`analysis_slices`, `analysis_chars_per_slice_high_water`, `analysis_cancellations`, `analysis_terminals`, `analysis_after_slice_hook`), every one read by `minimap_analysis_snapshot_for_test` |
| `minimap_analysis.rs` consumers | 1 | **2** total; **1 owning workflow** | **under-counted by 1** — confirmed. In-crate `ui/editor_page/minimap.rs` plus the external target `crates/lushtext-core/benches/benchmarks.rs:44`–`:46`, used at `:3213`, `:3433`, `:3569`–`:3570`. Eligibility is counted in owning workflows, so the relocation conclusion is unchanged |
| `_for_benchmark` seams | 0 (authoring) | **0** | confirmed; the class is invisible to a `test-utils` grep, so it was searched for by name |
| Inversions | 3 | **6 resumption points across 5 stage orders** | **floor, off by 2x** — see A.4 |
| External entry surface | not a census cell | **24 operations called from 16 files** | authoring's bound was ≤18 / ≤15; **both were low**. This is the figure the facade budget rested on, and it is why the first facade measured 389 |

**Post-migration size, re-derived last per 9.5**: **11 files, 3,414 production
lines**, all under `ui/editor_page/minimap/`. The **+785 production** over the
pre-move 2,629 is role narration, the facade, `evidence.rs` (216), `widgets.rs`
(59), and the survivor-triage tests; no behavior moved. Per file: `policy.rs` 944
production / 2,454 raw, `projection_execution.rs` 847, `mod.rs` 366,
`reflow_execution.rs` 252, `watch.rs` 228, `evidence.rs` 216, `admission.rs` 193,
`analysis_execution.rs` 142, `retirement.rs` 116, `widgets.rs` 59,
`test_policy.rs` 55. **Four figures moved again in the post-review
simplification pass** — net +15 (`projection_execution.rs` 832 → 847,
`analysis_execution.rs` 135 → 142, `watch.rs` 233 → 228, `retirement.rs`
118 → 116) — which is the same drift-during-a-change effect noted below, observed
one pass later: a re-derived cell has to be re-derived after the *last* edit, not
after the last test run. `mod.rs` held at **366**. **Seam census after
retirement: 2 fns / 15 gate sites**
(10 in `minimap/`, 5 still on `MinimapState` in `ui/editor_page/imp.rs`, which
stays a called presentation surface). These figures were taken **after** the final
test and mutation runs, not when the role files were written — slot 4 found three
of four cells had drifted during its own change, and this row's `policy.rs` did
grow by 13 production lines during survivor triage.

**Shared populations this row does not pool.** `ui/editor_page/imp.rs` is **not**
counted in the row size: `MinimapState` is a called presentation surface shared
with the editor page, and `WFR-DOCUMENT-SAVE` and `WFR-DOCUMENT-LOAD` already
record that file as the page's own state. A later row re-deriving it must not read
it from this row either. `crates/lushtext/tests/widget/{editor_page,window}.rs`
are likewise shared across every editor-page row and are not pooled here.

### A.3 Mutation-configuration inventory before the change (task 0.8)

Measured from the tool, not from the source text.

- The hand-listed `examine_globs` entry generated **457** mutants (post-exclusion);
  `model/minimap_analysis.rs` generated **21**. Row total in scope: **478**.
- Unexcluded, `minimap.rs` generated **689**, so the 14 minimap `exclude_re`
  entries were removing **232**.
- **Both of authoring's findings are confirmed.** Seven named methods have
  **zero definitions** anywhere in the tree: `apply_minimap_width_from_settings`,
  `wrapped_minimap_layout_exceeds_budget`,
  `buffer_has_line_exceeding_char_budget`, `collect_long_line_warnings`,
  `line_top_in_strip`, `line_bottom_in_strip`, `buffer_y_to_strip_y`. Four
  entries anchored to `minimap.rs:2046:55`, `:2047:21`, `:2054:16`, `:2058:19`
  match **zero** generated mutants; `fit_projected_bounds` had moved to line 2435.
- Per-entry match counts against real mutants, before: 42 / 61 / 64 / 54 for the
  four adapter and GTK-wrapper entries; 1 and 2 for the two
  `fit_native_slider_to_source_map_bounds` entries; **0, 0, 0, 0** for the four
  `fit_projected_bounds` anchors; 4 / 4 / 5 / 3 for the four `fit_marker_bounds`
  entries.
- The four `fit_projected_bounds` mutants those anchors were written for **still
  exist** — `2475:55 - with +`, `2476:21 < with <=`, `2483:16 < with <=`,
  `2487:19 > with >=` — and had therefore been unprotected and in scope. Triaged
  in §7.

### A.4 Reconciled stage trace: stage orders, primitives, resumption points (task 0.4)

Narrated from the code. **Five stage orders, six resumption points**, against the
census's recorded three inversions — the fifth consecutive slot to find its
inversion count is a floor. The full table is in the matrix's
[Workflow Stage Traces](../../../docs/workflow-readability-matrix.md#wfr-minimap)
entry, which this change corrected; the facade narrates the same six.

The two that the census's "three inversions" cannot account for are the ones that
matter: the **out-of-band early reveal**, which re-enters the live freeze machine
from a *different actor* (user scroll) while the follow-up is still armed, and the
**passive `ViewportObserver` re-entry** from `ui/editor_page/overscroll.rs`, which
enters the same machine deliberately **without** a freeze. Neither is a timer, so
neither shows up in a primitive count — which is precisely why an inversion census
taken from deferral primitives reads low.

### A.5 Behavior contracts as written today, verbatim (task 0.6)

Recorded before any code moved so §10 compares against quoted text rather than
memory. All four are preserved unchanged by this migration.

From `.agents/rules/ui.md`:

> Native `GtkSourceMap` minimap drift is an animation-frame invariant, not only a
> final-settle invariant. If sidebar/properties/editor-width work can reflow the
> active editor while the minimap is visible, capture stream frames with native
> viewport pixel anchors. A product fix may temporarily freeze already-rendered
> native minimap pixels with `gtk_lush_widgets::RenderHoldOverlay` during a
> detected width burst, but it must not draw, recolor, or restyle a replacement
> highlight. The freeze cover must be opaque if the live source map is allowed to
> repaint underneath, or transparent snapshot pixels can leak a stale native
> slider frame. It must reveal the live native map after the settle repair and
> quiet repaint window. Capture the freeze from the user action that is about to
> start the shell transition; passive scroll-adjustment or allocation observers
> should only schedule the settled repair, because they can fire after GTK has
> already invalidated or partially realized the native map.

From `.agents/rules/widget-wiring.md`:

> If a freeze or snapshot overlay sits above a live native widget while that
> widget repaints underneath, give the cover an opaque background matching the
> captured surface; otherwise transparent pixels can leak the live widget's stale
> rendered state through the snapshot.

From `.agents/rules/ui.md`, on wrapped-layout eligibility:

> Minimap wrapped-layout analysis eligibility comes from the O(1) live-buffer
> byte estimate: wrapping disabled and exact 2 MiB skip that analysis, while one
> byte over requests it. Classification must not scan or copy text.

From `.agents/rules/rust.md`:

> `set_minimap_tracking_suspended` is the guard those workflows use so a
> programmatic buffer replacement is not recorded as a user edit, and it suspends
> and exactly restores `imp().minimap.tracking_suspended`.

The last one is the contract A.9's confirmed load-workflow finding violates.

### A.6 Amendment basis and the ten-row retroactive re-check (tasks 1.1–1.3)

Both amendments add obligations rather than restate existing ones, so the
retroactive-amendment rule applies. The re-check was mechanical: extract every
literal path (and prefix) used as a gate key from `.cargo/mutants.toml`,
`scripts/check-visual-proof-policy.py`, `crates/cargo-gtk-proof/src/policy.rs`,
`scripts/check-accessibility-policy.py`,
`scripts/accessibility_source_fingerprint.py`,
`scripts/check-filesystem-boundary.sh`, `scripts/run-performance-smoke.sh`, and
`scripts/check-workflow-boundaries.py`; test each for existence; then cross-check
against every migrated row's moved files.

| Row | Verdict |
| --- | --- |
| `WFR-SEARCH-REPLACE` | **NON-COMPLIANT.** Slot 2a renamed `ui/search_panel/runtime.rs` → `execution.rs`, but `scripts/run-performance-smoke.sh` still filtered on `ui::search_panel::runtime::tests::mixed_non_match_events_share_one_budget`. `cargo test` exits 0 when a filter matches nothing and the lane checked only the exit status, so the `search_interactive_policies` lane reported a green "exact mixed search-event turn-budget proof" that had not executed since 2026-08-25. **Fixed here**, and given a match-count assertion so the next rename fails loudly. The row's *other* gate exposure was handled correctly by its own change: the two deleted `model/` policy modules left `model/**` and landed inside `ui/**/policy.rs` with parity recorded |
| `WFR-COMMAND-PALETTE` | COMPLIANT — deleted `ui/command_palette/runtime.rs`; no gate names any palette file literally |
| `WFR-DOCUMENT-SAVE` | COMPLIANT — deleted `model/save_admission.rs` and `save_runtime.rs`; neither is named literally. The `save_admission_policy` key in `run-performance-smoke.sh` is a Criterion **group name**, not a path, and still exists |
| `WFR-DOCUMENT-LOAD` | COMPLIANT — `load_runtime.rs` and `load_save.rs` are named by no gate; the three `editor_page::test_*load*` perf keys are widget test names and all resolve |
| `WFR-BUFFER-REPLACEMENT` | COMPLIANT — the `end_to_end_boundedness` lane keys on the module-path-independent substring `synchronous_` |
| `WFR-SESSION-RESTORE` | COMPLIANT — both `window::test_*session_restore*` keys still resolve |
| `WFR-LOCAL-HISTORY` | COMPLIANT |
| `WFR-DRAFT-RECOVERY` | COMPLIANT — both widget keys and the integration key resolve |
| `WFR-NOTES-BOOKMARKS` | COMPLIANT |
| `WFR-WORKSPACE-TREE` | COMPLIANT — **and the positive exemplar.** Its own commit re-keyed the affected `exclude_re` entry from `model/workspace_persistence.rs` to `ui/sidebar/policy.rs`, and the surviving `:68:` component is a **column**, still correct |

**Two adjacent dead keys, neither attributable to a migrated row**, fixed here
because a dead gate key is the defect the amendment names:

- `scripts/check-filesystem-boundary.sh` listed `crates/lushtext/benches` as a
  scan root **twice**; `git log --all` shows that path has **never existed**. `rg`
  silently skips a missing path, so the root protected nothing. Removed.
- The Python and Rust halves of the visual-proof predicate were key-identical
  before this change, which is the "every implementation" half being met — but
  **nothing enforced that they stay identical**, and the drift would have been
  silent in exactly the same way. Each side now carries its own parity assertion.

### A.7 Production and widget-test reach-through, in scope and out (tasks 0.7, 6.6, 8.1)

Matched on the reading expression, not the line, because slot 5b's numbers had
already moved. A multi-line-aware search was used.

| Site | Expression | Disposition |
| --- | --- | --- |
| `ui/automation.rs:1152` | `editor.imp().scrolled_window` | **RETIRED** → `editor_viewport_widget()` on the editor page |
| `ui/automation.rs:1159` | `editor.imp().minimap_overlay` | **RETIRED** → `minimap_shell_widget()` |
| `ui/automation.rs:1165` | `editor.imp().minimap.render_hold` | **RETIRED** → `minimap_reflow_freeze_cover()`. **Not in the matrix's table**; found during the retirement, same class, same workflow boundary |
| `ui/automation.rs:1177` | `editor.imp().minimap.source_map` | **RETIRED** → `minimap_source_map_widget()` |
| `ui/automation.rs:1239` | `editor.imp().minimap.marker_strip` | **RETIRED** → `minimap_marker_strip_widget()` |
| `tests/widget/editor_page.rs:3827`/`:3838`/`:3845` | `page.imp().minimap_overlay.width_request()` | **RETIRED** → `MinimapEvidence::overlay_width_request` |
| `tests/widget/window.rs:2643` | `source_map.compute_bounds(&*editor.imp().minimap_overlay)` | **RETIRED** → `MinimapEvidence::source_map_bounds_in_shell` |
| `ui/automation.rs:517`/`:518` | `window.imp().tab_view` | **out of scope** — `WFR-SHELL-LAYOUT`, slot 7 |

`ui/automation.rs` now contains **zero** `editor.imp()` reads.

**Four further reach-throughs of the same class were found and retired**, and the
count matters because an earlier revision of this section said "one". They are
**copy-pasted pairs**: `minimap_source_map` and `minimap_marker_strip` exist twice
over, in `tests/widget/editor_page.rs:225`/`:235` and
`tests/widget/window.rs:2577`/`:2595`, each reading
`page.imp().minimap.{source_map,marker_strip}`. Finding one and reporting one was
the same same-file-only search error that has bitten this programme before; the
duplicates are in a *different* test module.

They are retired onto `widgets.rs`'s accessors, widened to `pub` **under
`#[cfg(feature = "test-utils")]` only**, with the non-test build keeping
`pub(crate)`. That is the established pattern for the external widget harness —
it is a separate crate, so `pub(crate)` cannot reach it — and it is not the
"widen visibility to satisfy documentation" the rules forbid: the gate keeps the
wider surface out of every non-test build. `crates/lushtext/tests/widget/**` now
contains **zero** `.imp().minimap` reads.

### A.8 Path-keyed gate disarm evidence and re-keying proof (tasks 2.1–2.5)

**The disarm was observed, not argued**, in both halves.

*Invariant predicate.* Evaluating the **unmodified** predicate against the
post-move paths reported them as still visual-sensitive (so proof is demanded)
while requiring **neither** named invariant:

```
OLD ["…/editor_page/minimap.rs"]        visual_sensitive=[True]
                                         highlight=['native-minimap-highlight-anchors']
                                         animation=['native-minimap-animation-highlight-anchors']
NEW ["…/minimap/mod.rs", "…/minimap/policy.rs", "…/minimap/reflow_execution.rs"]
                                         visual_sensitive=[True, True, True]
                                         highlight=[]  animation=[]
```

*Mutation scope.* After the rename and **before** the config edit,
`./scripts/run-mutants.sh list` exited **0** while generating **0** mutants for the
retired entry, and **444** for `minimap/policy.rs` through the `ui/**/policy.rs`
convention alone.

**Re-keying, and the deliberate-red proof of each half.** Both implementations now
prefix-match `crates/lushtext-core/src/ui/editor_page/minimap/`, and each carries
its own parity assertion because there is no shared fixture between them:

| Implementation | Assertion | Deliberate red |
| --- | --- | --- |
| `scripts/check-visual-proof-policy.py`'s `run_self_tests()` | three representative role paths each require **both** invariants and are visual-sensitive | reverting the prefix to the old literal path → `AssertionError`, exit 1 |
| `crates/cargo-gtk-proof/src/policy.rs`'s `minimap_role_home_still_requires_both_native_minimap_invariants` | same three paths | reverting the constant → `test result: FAILED. 0 passed; 1 failed` |

**Reachability asymmetry, recorded rather than left implicit.** The Rust parity
assertion is a `#[cfg(test)]` unit test, so it runs under `cargo nextest` and CI's
non-widget lane but **not** under `make check-policy`; the Python one now runs
inside `make check-visual-proof-policy` and therefore inside `check-policy`. Both
execute in CI and both were proved by deliberate red, so neither side is
unguarded — but they are guarded by *different* lanes, and a future change that
runs only `check-policy` would exercise one half. Mirroring the Rust assertion
into a policy-reachable place was considered and not done: `policy --self-test`
already delegates to the same crate, and adding a second invocation path for one
test buys reachability at the cost of two places to keep in step.

**A blocker found while adding the Python half.** The script's `run_self_tests()`
was **unreachable**: an early `if __name__ == "__main__"` block delegated
unconditionally to the Rust tool and `sys.exit()`ed, so the `main()` and
`run_self_tests()` defined below it never ran — while `scripts/visual-geometry-smoke.py`
imports the same module and computes the recorded fingerprint from its predicates.
A parity assertion added there would have asserted nothing, which is exactly the
half-of-parity failure task 2.3 warns about. The CLI entry point now runs the
Python self-tests before delegating, and `make check-visual-proof-policy` invokes
the script rather than the Rust binary directly.

**The `examine_globs` entry retired rather than moved**, which is the outcome the
amended spec names as correct when the convention reaches the code. Verified after
the move that `ui/**/policy.rs` does reach
`ui/editor_page/minimap/policy.rs` — the nested-home rule requires that be
verified, not assumed.

### A.9 Data-safety pass and the candidate verdicts (tasks 0.9, and any fix cycle)

Full record in [`evidence/data-safety.md`](evidence/data-safety.md). Two confirmed
findings, both fixed here:

1. **`ui/editor_page/imp.rs::dispose` left the minimap's `Debounce` and
   `SettleBurst` armed** over callbacks that reach panicking `TemplateChild`
   accessors. This row's own. Fixed; regressed by
   `editor_page::test_minimap_evidence_reads_stay_honest_after_dispose`, which
   flushes past both windows after `run_dispose()`.
2. **`ui/editor_page/load/execution.rs`'s `finish_chunked_install` returned from
   two exits without restoring the installation state it captured**, so a
   superseding load adopted the already-suspended values as its own baseline and
   restored them to *suspended* — a permanently read-only tab with local-history
   capture disabled. One of those exits also left `begin_irreversible_action()`
   unmatched. Latent (no demonstrated trigger), in the **already-migrated**
   `WFR-DOCUMENT-LOAD` row, and fixed here.

The two candidates that came back clean — the sliced buffer cursor and the
modified-line mark operations — are recorded with their evidence rather than as
absences.

### A.9a Preservation anchors verified explicitly (task 5.8)

`super::` path renames dominate this diff, which is exactly the condition under
which a real edit hides among them — slot 5b's `row_factory.rs` lesson. The
freeze/settle/reveal choreography was chosen as the anchor because it is the part
a role split could most easily perturb, and its production diff was verified
**line by line** against `git show HEAD:…/minimap.rs` with the renamed identifiers
and the visibility changes normalised away.

**Result: byte-identical except for one intended move.** The only difference is
`note_minimap_height_reflow` relocating to the facade, which is documented in the
facade and in the matrix row. Specifically preserved verbatim:

- `debug_assert!(render_hold.live_child().opacity() >= 0.99)` at **both** sites
  (the warm-under-cover path and the drop path), plus
  `debug_assert!(!render_hold.cover_is_visible())`. None acquired a mutating call,
  so the denied `clippy::debug_assert_with_mut_call` stays satisfied.
- `if minimap.reflow_settle.pending() || !minimap.reflow_reveal_pending.get()` —
  the guard whose setter must not land on the other side of a role boundary.
- All seven freeze-machine operations confirmed in `reflow_execution.rs` and
  nowhere else: `freeze_native_minimap_for_reflow`,
  `warm_live_minimap_under_reflow_freeze`, `reveal_repaired_minimap_early`,
  `drop_minimap_reflow_freeze`, `minimap_reflow_freeze_visible`,
  `finish_minimap_reflow_settle`, `schedule_minimap_reflow_settle_impl`.

**Argument-count suppressions (task 4.5): 0 in this row, before and after.** The
workspace count holds at **1**, and the survivor is still
`model/action_catalog.rs:178`'s domain catalog constructor, which the rule
exempts. No cross-module workflow boundary in this row acquired one, which would
have signalled an unreified seam.

### A.10 Automation no-widening proof (tasks 8.3, 8.4)

**Method note.** The byte-identical proof here is a **static** one rather than a
before/after live Automation1 capture, and that is a deliberate scoping decision
rather than a shortcut, stated so a reviewer can disagree with it: nothing this
change touched can alter a projected value, because the projection functions,
the exported schema, and every string key are unchanged, and the five retired
reach-throughs were replaced by accessors that return **the same widget handles
by construction** (`imp.minimap_overlay.try_get()` versus `&*imp.minimap_overlay`,
and `borrow().as_ref().cloned()` versus the same expression inlined at the call
site). The `automation-smoke` lane then exercises the real projection end to end
against a live D-Bus app, which is the part a diff cannot prove.

**The delta is legitimately empty, and that is the result.** No exported schema
field, projection function, surface name, pixel-anchor name, readiness predicate,
or blocker id changed. `READINESS_BLOCKER_MINIMAP_REFRESH` and
`AUTOMATION_WORKFLOW_MINIMAP_REFRESH` keep their exact `"minimap-refresh"` values
and their membership in the `visual-geometry-settled` blocker list — six visual
geometry scenarios wait on that predicate, so changing it would invalidate this
row's own proof lane.

What changed is *how* the projection reaches the widgets: five `.imp()`
reach-throughs became five named operations returning the same values. The
minimap snapshot fields are therefore **not** newly projected from `evidence.rs`,
and no drift-gate registration was added — `MinimapEvidence` is a `test-utils`
type that the D-Bus schema never sees, which is the visibility rule the
convention already states. `make check-automation-docs` passes with no docs delta.

### A.11 Facade measurement, margin, stage orders, and entry points (tasks 9.1, 9.2)

**366 of 370, with 4 lines of margin — after one escalation step and one cold-read fix pass.**

| | |
| --- | --- |
| Projection | ≈300, credible worst case ≈340 |
| First honest facade | **389 — over budget** |
| After escalation step 1 (*delegate harder*) | **355** |
| After the cold read's seven fixes | **362** |
| Stage orders narrated | 5 |
| Resumption points narrated | 6 |
| External entry operations | 24, from 16 files |

The escalation was the declared one and stopped at step one: the four widget
accessors moved into `widgets.rs` as a **called presentation surface**, which is
where the taxonomy already placed them — a module that only projects the workflow
onto widgets is outside the five-name role set. The budget number was **not**
edited; the census row was **not** split; no stage narration moved into a
coordination module.

**The datum the programme should carry forward.** The four prior data points
suggested stage-order count drives facade size. This row contradicts that: five
stage orders is fewer than 5b's twelve, yet this facade needed escalation and 5b's
did not. What bound it was the **external entry surface** — 24 operations against
load's seven — because each costs a narration line plus a delegation regardless of
how few stages there are.

### A.12 Mutation relocation parity and extraction gain, reported separately (tasks 3.2, 3.6, 7.5)

Full record in [`evidence/mutation-minimap-policy.md`](evidence/mutation-minimap-policy.md).
Headline figures, each with its own invocation and anchor:

- **Relocation parity: exact.** `model/minimap_analysis.rs` at `HEAD` in a clean
  worktree — **21 generated / 19 caught / 2 missed / 0 unviable**. The same region
  inside `minimap/policy.rs` after the move — **21 / 19 / 2 / 0**, the same two
  survivors on the same function. Not one generated mutant changed.
- **Extraction gain: from zero.** `minimap/policy.rs` whole file —
  **425 generated / 407 caught / 12 missed / 6 unviable**. Subtracting the
  relocation's 21 leaves **404 mutants of newly-extracted pure policy that were
  not in the deterministic lane before, 394 caught on the first run**. The
  extraction has no before-count and cannot fail, which is why it is reported
  apart from the parity figure rather than folded into it.
- **Retired scope entry, accounted for.** Measured after the rename and before
  the config edit: the hand-listed entry generated **0** mutants while
  `mutants-list` still exited **0** — the silent disarm — and `ui/**/policy.rs`
  generated **444** for the new file on its own. Row totals: **478 in scope
  before → 425 after**; the difference is the GTK adapter population that the
  four retired `exclude_re` entries were already suppressing, so nothing that was
  being mutated *and killed* left the lane.
- **Unfilterable floor: 34 mutants**, 13 of them pre-existing survivors in
  `services/draft_service.rs` and `services/file_tree.rs`, identical before and
  after. `--re` narrows reporting, not the run.

### A.13 Lane consequences of the directory move (tasks 2.5, 10.11–10.14)

**Visual geometry — the row's primary acceptance lane — passes against the final
staged tree from a clean artifact root: 80 cases, 80 passed, 0 failed, 0
skipped.** The gate's own verdict is quoted rather than paraphrased, because it is
also the proof that A.8's re-keying worked:

> visual geometry proof summary passed; summary matches current visual-sensitive
> diff; summary pixel-verified required visual invariant ids:
> `native-minimap-highlight-anchors`; summary animation-verified required visual
> invariant ids: `native-minimap-animation-highlight-anchors`; workspace-sidebar
> animation matrix verified 6 cases

Three things in that sentence matter. "Matches current visual-sensitive diff"
means the fingerprint was taken over the shipped tree, after `git add -N` — slot
5b's ship lesson. Both required invariant ids are demanded **and** met, which
could only happen through the re-keyed prefix predicate; before the re-key the
same diff required neither. And the six `minimap-sidebar-workspace-animation`
case ids are verified as a matrix, across 128 sampled animation frames, so this
is stream-frame coverage rather than a final-settle-only pass.

**Accessibility smoke passes from a clean root**, with `A11Y-EDITOR-MINIMAP`
among the 43 matrix rows covered and the `minimap-transition` case asserting the
exact anchor task 10.14 names:

> PASS surface=minimap transition role=text name=Editor for accessibility-smoke.txt

That is the contract stated as "the editor remains the semantic text target while
minimap state changes", proved through AT-SPI rather than inferred. Warnings: 51
lines, **all allowlisted, 0 unexpected**.

**Visual smoke and automation smoke both pass from clean roots.** Visual smoke
captured every scenario including `main-search-minimap`; automation smoke
exercised the live D-Bus object end to end — catalog, readiness, snapshot,
predicate waits, workflow events, and a parameterized action activation — with its
runtime log scanned for unexpected GTK/GDK/Adwaita/GIO/D-Bus/portal/AT-SPI
warnings. That lane is what actually proves A.10's no-widening claim against a
running app rather than against a diff.

**The lane's fail-open hazard is now guarded at every site, not just the one that
was broken.** A shared `smoke_assert_ran` helper landed in
`scripts/smoke-common.sh` — one definition, not a copy per call — and
`run-performance-smoke.sh` wires it to **all ten** filter invocations: the
Criterion loop (with a `^Benchmarking ` pattern, since benches print no libtest
summary), all five widget-harness calls, the two `--lib` proofs, the integration
proof, and the search-event proof whose stale key started this. The helper was
proved against four synthetic logs before being trusted: a real libtest run
passes, a `0 passed; … 1608 filtered out` run **fails**, a real Criterion log
passes, and a Criterion log with no benchmark **fails**. Guarding only the site
that happened to break would have left nine others able to fail open the same way.

**Performance smoke: the string keys survived, and the summary content proves
it.** The lane was scoped with `LUSHTEXT_PERFORMANCE_SMOKE_FILTER=quality_gap_scale`
— stated plainly rather than implied, because that is the one filter carrying this
row's keys, and a full 17-filter run proves nothing extra about this migration.
Task 10.15a's point is that this lane can report success over an empty run, so the
proof is the summary's **content**, not the exit code:

```
minimap-analysis-evidence cached_characters=2231000 slices=69
  chars_per_slice_high_water=32768 slice_limit=32768 gtk_heartbeat=true
  cache_owned=true current_generation=true visible=true
minimap-cancellation-evidence cancellations=1 terminals=1
  cache_generation=Some(4) current_generation=4 cached_characters=240136
```

Both grepped labels are present with real values — 69 bounded slices, a
high-water exactly equal to the 32 KiB slice limit, and one cancellation paired
with one terminal — so both literal widget test names matched and ran. This is the
same lane whose *other* filter was found silently matching nothing since
2026-08-25 (A.6), which is why the content was checked rather than the status.

**One wording correction to task 10.11.** It asks for both invariant ids to appear
in `pixel_verified_invariant_ids`. The runner does not report them that way: it
splits `pixel_verified_invariant_ids` (screenshot pixel anchors) from
`animation_verified_invariant_ids` (stream frames), and the animation invariant
appears only in the second. Both are required and both are met — the gate says so
— but a future slot reading task 10.11 literally would look for a field that will
never contain that id.

**Widget lane, `--retries 0`: exit 0, all tests passed, and `grep -c FLAKY` on the
full log returns 0.** No test needed a second attempt, so nothing had to be
classified under the flake-discipline rule. The lane also enforces a
zero-warnings contract on its own build, which caught two real problems in this
change's code before the run could pass: an unused import and three unused
methods left behind by the facade extraction, and a missing `// SAFETY:` comment
on the disposal proof's `run_dispose()` — `clippy::undocumented_unsafe_blocks` is
denied workspace-wide.

**The directory move had no lane consequence that needed a script edit except the
two path-keyed gates in A.8**, and that was verified rather than assumed:

- the five `minimap-sidebar-*` visual geometry scenarios in
  `scripts/visual-geometry-scenarios/` key on scenario **ids**, not paths, and are
  untouched;
- `scripts/run-accessibility-smoke.sh`'s `minimap-transition` case keys on a case
  id and the `--enable-minimap` flag, and the stable AT-SPI anchor
  `atspi-anchor-minimap-transition-text-editor-for-accessibility-smoke-txt` is
  documented in `docs/automation-reference.md` and unchanged;
- `scripts/run-performance-smoke.sh` keys on two literal **widget test names** and
  two grepped **evidence labels**; all four were deliberately preserved and
  verified present (task 10.15a);
- `docs/accessibility-matrix.md`'s `A11Y-EDITOR-MINIMAP` row already named the
  owner path-agnostically as `ui/editor_page/minimap`, so the move left it
  correct — **verified, not edited**, which is what task 9.9 asked for;
- `resources/` and `openspec/specs/` have a **zero-line diff**, so no CSS class,
  surface name, pixel-anchor name, blocker id, or requirement moved.

### A.14 Cold-read result, five questions (task 10.17)

A reader who did not write the change read **`minimap/mod.rs` and nothing else**
and answered all five questions. Questions 3 (what happens between the sidebar
animating and the live map returning), 4 (why there are two ways to schedule a
reflow settle), and 5 (where a stale slice is rejected) were answered **without
guessing**. Questions 1 and 2 were answered with named gaps.

The verdict was that the facade reads well as a first encounter — the pixel
oracle stated before any mechanism, the role table removing the "which sibling do
I open" question, the two exceptions pre-justified where a reviewer would object,
and the resumption-point section being the part most facades omit and the part
the reader most needed.

**Seven defects were reported and all seven are fixed**, because a question a
reader has to guess at is a facade defect:

1. **The stage-order arithmetic did not add up.** The file justified three
   `execution` modules as "three distinct ordered stage orders" and then said
   "six resumption points across five stage orders". Now: "three of this
   workflow's five stage orders (the other two are install and retire)".
2. **`note_minimap_height_reflow` contradicted itself** — the doc promised to keep
   the freeze active while the body only queued a redraw. It keeps the freeze by
   *not* entering the reveal path, which the doc now says.
3. **Two names for the same door.** Resumption point 5 linked the private
   `reveal_repaired_minimap_early` rather than the facade entry point, landing a
   reader on a helper. Now links the facade name.
4. **`suspend_minimap_projection` had no visible partner.** Nothing in the facade
   re-attached the projection, so a reader could not tell whether reattachment was
   automatic or a caller obligation. The doc now says stage 4's refresh re-binds it.
5. **`evidence` and `test_policy` read as production.** The role table said
   `MinimapEvidence` is "the one typed value observers read" without noting that
   both modules are `test-utils`-gated. Now stated in the table.
6. **"Automation1" was used twice without introduction**, load-bearing for the
   `widgets` row. Now named as `ui/automation.rs`, the D-Bus automation surface.
7. **`MINIMAP_ANALYSIS_CHARS_PER_SLICE` was the only budget without its value**,
   while 80ms, 150ms, 800ms, 250ms, and 2 MiB were all concrete. Now "(32 KiB)".

Two reported items are **accepted as-is** with the reason recorded: the reader
could not tell who calls `setup_minimap` (editor-page construction — a caller the
facade deliberately does not name, since every workflow facade would otherwise
document its construction site), and could not map `Ctrl+Shift+M` to an action
name (it flips a GSettings key that stage 2 reads, which the facade already
implies through "the user preference"). Adding either would spend budget on
information the reader can get from the action catalog.

The fixes cost 7 physical lines, taking the facade from 355 to **362**; the
review's role-table correction took it to **366 of 370**, leaving 4 lines.

### A.15 Tail simplify pass, after full verification (task 10.18)

Applied only to code this change wrote, and it produced **one real
simplification** rather than a cosmetic pass — found by survivor triage rather
than by reading:

- The min-height expansion block was **duplicated verbatim** between
  `fit_marker_bounds` and `fit_projected_bounds`. Extracted into one
  `expanded_to_min_height`.
- Inside it, `.min(upper - lower)` was **dead code**: the trailing
  `(top.max(lower), bottom.min(upper))` already bounds the result, and whenever
  the cap would bind, the expansion fills the whole span either way. Removed.
- Two `mut` rebinds in the callers became plain bindings once the block left, and
  `if bottom < top { bottom = top; }` became `.max(top)`.

Deliberately **not** simplified: the freeze/settle/reveal choreography, verified
byte-identical in A.9a. A structural migration is the wrong place to improve a
June 2026 bug fix that has stream-frame proof behind it.

Pre-existing `never used` warnings were observed in files this row does not own —
counted from the tool rather than from memory, `cargo test -p lushtext-core --lib`
reports **8 warnings covering 12 items**: two `*_for_test` seams in
`services/content_search/replace.rs` (`WFR-SEARCH-REPLACE`) and the rest the whole
`DisposalProducer` family in `ui/plain_disposal.rs` (`WFR-PLAIN-DISPOSAL`) —
`MAX_SMALL_PENDING_DISPOSAL_BYTES`, `try_own_for_gtk`, `DisposalProducerInner`,
`DisposalProducer`, its five associated items, and `retry_pending`. All are
`test-utils`-gated items unused in the default-feature lib-test build. An earlier
revision of this section said "three warnings", which was the number visible in
the truncated output I had looked at rather than the number that exists. **They are not blockers**: the blocking gate is
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, which is
clean, and the widget lane's zero-warning contract is also clean. Recorded here so
their absence from the fix list is a decision rather than an oversight.

## Appendix B — handoff

### B.1 Programme and matrix agreement (task 11.1)

`make check-workflow-boundaries` passes, reporting **11** pure mutation-scoped
policy modules (up one — this row created a `policy.rs` where none existed) and
confirming that every migrated matrix row names complete, existing roles and that
the programme record's slot ledger agrees with the matrix. The ledger reads
`- slot 6 (complete): WFR-MINIMAP, WFR-AUTOMATION-SPINE (partial)`, and
`WFR-AUTOMATION-SPINE` was added to slot 7's outstanding line — the gate rejects a
`pending` matrix row that appears in no outstanding slot, and marking the spine
`migrated` to satisfy it would be a false claim while slot 7's rows still project.

### B.2 To slot 7 (task 11.2)

**Every figure corrected by 0.3, with its direction and unit.** See A.2. In
summary: both size cells were **raw and unlabelled** (the row was 34% smaller in
production than the cell implied); seam gate sites were **low by 5**; the policy
consumer count was **low by 1**; the inversion count was a **floor off by 2x**;
and authoring's own external-entry bound (≤18 ops / ≤15 files) was **low** at
24 / 16 — the figure the facade budget rested on.

**Stage orders and inversions.** Five stage orders, six resumption points. Two of
the six are invisible to a deferral-primitive count, which is why the census read
three: the out-of-band early reveal (a *different actor* re-entering a live
machine) and the passive `ViewportObserver` re-entry (the same machine entered
deliberately without a freeze). **Slot 7 should count re-entries by actor, not
timers by type.**

**The facade measurement, and whether 370 held.** It held, at **366**, but this is
the programme's **first slot to need the escalation path**: the first honest
facade measured 389. Step one, delegate harder, sufficed. The new datum: what
stressed the number was the **external entry surface** (24 operations from 16
files), not the stage-order count — five here versus 5b's twelve at 291. Slot 7's
`WFR-MARKDOWN-PREVIEW` is the largest remaining row; it should project against
entry-point count, not stage count.

**The two path-keyed gates, and the files slot 7 owns that still appear in
them.** This is the handoff item with a trap behind it:

`scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`
each list, by **exact path equality**, files that require the two native-minimap
invariants. Two of those are **slot 7's**:

- `crates/lushtext-core/src/ui/window/actions.rs` — `WFR-SHELL-LAYOUT`
- `crates/lushtext-core/src/ui/window/imp.rs` — `WFR-SHELL-LAYOUT`

Both appear in **both** implementations, in **both** predicates (highlight and
animation). If `WFR-SHELL-LAYOUT`'s migration relocates or splits either without
re-keying all four sites, the native-minimap invariants stop being required for
those diffs and every gate keeps exiting 0. The minimap's own files are now
prefix-keyed and safe from a further split; these two are not.

`ui/automation.rs` and `model/automation.rs` also appear in both predicates and
are `WFR-AUTOMATION-SPINE`'s, which slot 7 completes.

**And the same defect wearing a string key.** The one finding from the ten-row
retroactive re-check was a **test name**, not a path:
`scripts/run-performance-smoke.sh` filtered on a module slot 2a had renamed, and
`cargo test` exits 0 on a filter that matches nothing. Slot 7 should treat every
string-keyed lane filter as this hazard class — the script still carries 17
Criterion group names, 20 widget test names, 3 module-qualified test paths, and
several grepped evidence labels, none of them protected by anything. This change
added a match-count assertion to the one it fixed; the rest are unguarded.

**Mutation-configuration retirement totals.** The row's **14 entries naming 66
methods** became **4 entries naming 0 methods**, and the hand-listed
`examine_globs` entry **retired** rather than moving. Exactly one pre-convention
hand-listed UI file remains — `ui/markdown_preview/inline_footnotes.rs`, which is
**slot 7's**, and which retires the same way when `WFR-MARKDOWN-PREVIEW` migrates.
Reading the retired entries against the tool found seven method names with zero
definitions and four `line:column` anchors matching nothing; slot 7 should read
its own inherited entries against `mutants-list` before trusting any of them.

**Data-safety verdicts with owners.** Two confirmed, both fixed here (A.9). One is
this row's; **the other is `WFR-DOCUMENT-LOAD`'s, a row migrated by slot 3b** —
found only because the minimap's tracking-suspension guard is one of the four
values that exit failed to restore. A migrated row is not a closed row. Two
follow-ups are handed on rather than fixed: giving `LoadInstallationState` a
scope-owned restore so no future exit can drop it (`WFR-DOCUMENT-LOAD`), and the
draft-restore burst of up to 2,000 source marks each re-entering the notes
menu-state refresh in one turn (`WFR-NOTES-BOOKMARKS` / `WFR-DRAFT-RECOVERY`,
a `gtk-perf-review` item rather than a data-safety one). One candidate is
**unresolved rather than cleared**: whether any readiness or visual-geometry
snapshot can be taken from a `mark-set` handler, which is the only window in which
`minimap_work_pending` under-reports.

**Actuation-seam budget: still unspent.** This change added **zero** new actuation
seams. Slot 5b's budgeted one remains available. The row's two *existing*
actuation seams were given explicit dispositions rather than carried — one retired
onto a real production drive, one kept with its justification at its definition —
which is a step a consolidation that only names inspection seams would have
skipped. **Slot 7 should expect its rows to have the same asymmetry**: a seam
census that counts `*_for_test` functions finds inspection seams and misses
actuation ones.

**The retroactive re-check result.** One real disarm across ten rows
(`WFR-SEARCH-REPLACE` / slot 2a), plus two adjacent dead keys not attributable to
any migration. The not-a-confirmation streak is now at **six**.

**One reach-through left knowingly**, with the reason: the widget-test helper
`minimap_source_map(page)` still reads `page.imp().minimap.source_map`. It was
outside task 0.7's enumeration, and retiring it would mean widening a `pub(crate)`
accessor to `pub` purely for a test.

**Every `[~]` item, with its reason.** Two, both awaiting the user's desktop
session and neither silently dropped: task 10.19's live `make run` walkthrough
(sidebar toggling while watching stderr for `Trying to measure GtkBox`,
`pixman_region32_init_rect`, `Gtk-CRITICAL`, and `GLib-GObject-WARNING`; freeze
revealing only after the settle; `Ctrl+Shift+M`; no visible slider drift across a
full show/hide cycle), and task 10.20's manual Orca check for
`A11Y-EDITOR-MINIMAP`.

**Plainly, what is migrated and what is not.** `WFR-MINIMAP` is migrated on every
Completion Rule axis. `WFR-AUTOMATION-SPINE` remains `pending` and continues into
slot 7, which is why the ledger says `(partial)`. Nothing in this row is recorded
as accepted debt; the follow-ups above are handed on with named owning rows.

### B.3 Pixel-preservation method inherited by `WFR-MARKDOWN-PREVIEW` (task 11.3)

`WFR-MARKDOWN-PREVIEW` is slot 7's largest row and the other row the matrix flags
for pixel-visible risk. Three parts of this slot's method transfer, and one
deliberately does not:

**Transfers.**

1. **Key the required-invariant predicate on a directory prefix, not a file
   path — in both implementations, with a parity assertion on each side, each
   proved by a deliberate red.** The preview row will create a role home the same
   way this one did, and preview's files are named in neither predicate today,
   which means the preview row must decide whether its rendered output *should*
   demand a named invariant rather than inheriting silence.
2. **Run the visual lane against the final staged tree, from a clean artifact
   root.** Staging a rename changes the digest the gate fingerprints, so a lane
   run before `git add -N` proves a tree that is not the one shipping.
3. **Write the evidence surface's three proofs before believing the module doc.**
   This row's disposal proof failed on first run and the defect was real.

**Does not transfer.** This row's stream-frame requirement is specific to a
**native** widget whose rendering LushText does not own: `GtkSourceMap` draws its
own slider from private state, which is why an app-computed rectangle was correct
while the pixels were not. Markdown preview renders from app-owned code and CSS
into widgets LushText builds, so its pixel risk is real but differently shaped —
anchored block width after a shell transition, and code-block repair — and it
already has its own settle path. Slot 7 should not assume it needs
animation-frame capture merely because this row did.
