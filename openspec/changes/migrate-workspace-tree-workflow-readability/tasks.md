> ## STATUS — read before the task list
>
> **`WFR-WORKSPACE-TREE` is MIGRATED**, and the migration is complete in the sense the
> Completion Rule means: narrative facade, bounded coordination roles, one `policy.rs`,
> one `evidence.rs`, seam value objects, mutation-parity evidence, **and the inspection
> seams retired into the surface**.
>
> **Headline results:**
>
> - **Facade 292 of 370**, by delegate-harder alone — no escalation, no census-row split.
> - **Four dissolutions** (`tree_loading.rs`, `tree_index.rs`, `watch_targets.rs`,
>   `workspaces.rs`), two of which **corrected the inherited map from the code**.
> - **Twelve stage orders / 44 resumption points** against a floor of five — **8.8x**,
>   the programme's widest correction.
> - **`evidence.rs` discharges the no-materialization statement with a driven proof**,
>   inertness asserted with rows collapsed **and** expanded.
> - **Seams 60/111 → 41/93**: nineteen inspection seams retired, ~114 call sites
>   rewritten, five tuple-returning seams replaced by named fields, and the destructive
>   "take touched rows" split into a non-destructive read plus a reset drive.
> - **`window.workspace` projects from evidence**; both `ui/automation.rs`
>   reach-throughs retired; live D-Bus capture matches the contract field for field.
> - **Mutation: 59 / 49 caught / 10 unviable / 0 missed**, with exact relocation parity
>   and the 7 inherited survivors triaged to 0.
> - **Data safety: seven defects fixed** — two from pass 1, **three from pass 2**, and
>   **two more from the fix cycle**, including two **CRITICAL** ones: the M-4 guard
>   destroying the whole workspace list, and that fix then being **inert** because the
>   bit it read meant "sections were rebuilt", not "a load was adopted". The count is
>   stated once, with its rule, at the end of `evidence/data-safety.md`.
> - Lanes: `make check` 0, `check-policy` 0, nextest **1741**, widget **zero FLAKY at
>   `--retries 0`**, all three smoke lanes green from clean roots, rustdoc clean.
>
> **The pass-2 audit is the most important artifact here.** It found **six** findings,
> every one introduced by this change or by a fix inside it — the same shape slot 5a's
> pass 2 found. The CRITICAL one: the M-4 guard, when the mutation lands *before the
> first load*, left memory holding only the new workspace while the already-scheduled
> write committed it over **every workspace on disk**. The driven M-4 test did not catch
> it because that test waits for the startup load to settle, always running the safe half
> of the race. Fixed with a one-bit decision now living in pure policy, and pinned by a
> test that asserts the resurrection hazard is real.
>
> **And the fix cycle found that fix was inert.** The bit fed to that pure policy was set
> by `build_sections_from_file`, which *every* mutation reaches — so the guard chose
> `KeepMemory` in exactly the case `MergeAndPersist` exists for, and `MergeAndPersist`
> had no reachable production caller. The bit is now written only by the load-adoption
> entry point, and the pre-first-load widget test the earlier revision had deleted is
> reinstated against a **standalone** sidebar (no startup gate, so the interleaving is
> deterministic). It fails `left: 1, right: 4` against the landed code.
>
> **Remaining follow-up, recorded rather than hidden** — also on the matrix row and the
> slot ledger:
>
> 1. `workspace_section/scan_execution.rs` is **~2,000 production lines**. Accepted debt
>    with its reason; the split path is named.
> 2. **Five confirmed non-tree data-safety findings** are handed on with owning rows, and
>    **M-8's upstream fix** is named with this row's amplifying step recorded.
> 3. Task 7.6's **two-tree** capture-and-diff was not run; the live capture plus an
>    unchanged schema, a passing `--self-test` docs gate, and the newly **registered**
>    `window.workspace` drift-gate projection are the claim made instead. The task is
>    recorded `[~]`.
> 4. The two ungated `_for_benchmark` seams are still undisposed.
>
> Live and manual proof (task 10.13) remains `[~]` and **awaiting the user's decision**.

> ## How to read this task list
>
> This is **slot 5b**: `WFR-WORKSPACE-TREE` alone. Slot 5a migrated
> `WFR-NOTES-BOOKMARKS`, landed both convention amendments of that change, fixed
> seven confirmed data-safety defects in **this row's** file operations, and left
> this row's `policy.rs`, `seams.rs`, and `test_policy.rs` in place. The archived
> change is `openspec/changes/archive/2026-08-27-migrate-workspace-tree-and-notes-workflow-readability/`,
> and its `tasks.md` sections 4, 5, the tree halves of 6, 8, 9, 10, and its
> Appendix B.2 are this change's scope skeleton.
>
> **Sections are ordered by increasing risk.** Orientation, then the convention
> amendments, then the decisions to re-measure, then pure policy and its
> relocations, then seams, then the structural role move, then the evidence
> surface and seam retirement, then automation, then the data-safety pass over the
> restructured file operations — the workflow that renames and deletes the user's
> own documents — then the facade and the records, then verification.
>
> **Inherited decisions are verified, not re-litigated.** Where a task says
> "confirm", the expected outcome is a confirmation with its evidence *or* a
> recorded deviation with its reason. It is never "decide again".

## 0. Gates, orientation, and premise re-verification

- [x] 0.1 **Slot-5a gate — blocking.** Verify mechanically on a clean tree rather
      than reading it from the proposal: `openspec/changes/archive/` contains the
      slot 1, 2a, 2b, 3a, 3b, 4, and 5a changes; `openspec/specs/` holds
      `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
      `gtk-adapter-module-boundaries`, `mutation-testing`, and
      `dbus-automation-spine`; `docs/workflow-readability-matrix.md` marks the
      **nine** migrated rows `migrated` with complete `Migrated Workflow Roles`
      subsections naming paths that exist; `WFR-WORKSPACE-TREE` is still
      `pending`; the ledger in `docs/next/workflow-readability.md` marks slot 5a
      complete and slot 5b outstanding; and `make check-workflow-boundaries`
      passes, reporting the current count of pure mutation-scoped policy modules
      (**10** at authoring). This row is `tier-3`, so the two-proof rule applies
      and is satisfied nine rows over. Record in A.1.
- [x] 0.2 Read, in this order: `docs/next/workflow-readability.md` end to end
      including "Why slot 5 split into 5a and 5b" and every "Convention friction
      slot N hit" section; `docs/workflow-readability-matrix.md`'s Settled
      Conventions, Facade size budget, Evidence-surface reentrancy, Cross-cutting
      eligibility, Evidence pointer form, Completion Rule, and **all five**
      amendment re-check subsections; then slot 5a's archived `tasks.md` Appendix
      A and B — **this change is its named recipient** — and its
      `evidence/shared-ownership-decisions.md`,
      `evidence/evidence-surface-materialization.md`,
      `evidence/stage-traces.md`, `evidence/facade-measurements.md`,
      `evidence/census-reverification.md`,
      `evidence/widget-test-reach-through-migration.md`,
      `evidence/tree-behavior-equivalence.md`, and `evidence/data-safety.md`.
      Then `.agents/rules/rust.md`, `.agents/rules/widget-wiring.md`, and
      `.agents/rules/ui.md`'s **File Tree** and **Multi-Workspace Sidebar**
      sections, which are this row's behavior-preservation anchor contracts.
- [x] 0.3 **Premise re-verification — before any code, and state the unit on every
      figure.** This change's own first draft got this wrong: it compared 5a's
      **production-only** census (the method is stated verbatim at
      `evidence/census-reverification.md:3`) against **raw** file totals and
      concluded the cell had gone stale by ~900 lines. It had not. Four files had
      grown **only** inside their `#[cfg(test)]` module. Re-derive with the unit
      named at every step:

      - **Size**: production lines only, excluding `#[cfg(test)]` modules —
        including a co-located test module in **its own file** behind
        `#[cfg(test)] mod tests;`, which a naive per-file scan counts as
        production. Authoring measured **11,741 production lines across 23 row
        files**, which equals 5a's 11,214 plus 527 of growth **and** the direct
        per-file sum, so it is checkable two ways: canonical home `mod.rs` 415,
        `workspaces.rs` 864, `imp.rs` 246, `callbacks.rs` 219, `dialogs.rs` 205,
        `policy.rs` 134 (raw 261), `seams.rs` 92 (raw 133), `test_policy.rs` 88;
        section `tree_loading.rs` 1,269, `tree_index.rs` 844 (raw 969),
        `folders.rs` 835, `context_menus.rs` 809, `dnd.rs` 769, `mod.rs` 744,
        `peek.rs` 728, `actions.rs` 707, `refresh.rs` 666 (raw 702), `watch.rs`
        593 (raw 606), `imp.rs` 508, `row_factory.rs` 463, `watch_targets.rs`
        264 (raw 337), `row_accessibility.rs` 199, `icon_presentation.rs` 80
        (raw 155). Confirm that `ui/sidebar/file_tree_item.rs` (150) still counts
        as **not this row's** per the matrix's "Surfaces With No Coordination
        Tier".
      - **Growth attribution, file by file, in production units**, so the
        correction is attributable rather than mysterious: `actions.rs` +173
        (5a's rename/cleanup fixes), `workspaces.rs` +21 (the M-4 load-generation
        guard), `watch.rs` +10 (the watch-target repair operation), `mod.rs` +9
        (module declarations and one re-export pair), the three new files +314
        (`policy.rs` 134, `seams.rs` 92, `test_policy.rs` 88) — total **+527**.
        And state explicitly that **four of 5a's per-file figures are exactly
        reproducible with zero production growth** — `tree_index.rs` 844
        (`#[cfg(test)]` opens at `:845`), `watch_targets.rs` 264 (`:265`),
        `icon_presentation.rs` 80 (`:81`), `refresh.rs` 666 (`:667`) — because
        recording that 5a was *right* is what keeps the next slot from
        re-"correcting" a correct cell.
      - **The one genuinely stale cell**: the matrix records 5a's `policy.rs` at
        **190** lines, which is neither its current raw size (**261**) nor its
        production size (**134**). Correct it in task 9.6 with the unit named.
      - **Seams**, per kind, with the gate-site count and the unit stated.
        Authoring counted **60 `*_for_test` functions across 111
        `#[cfg(feature = "test-utils")]` gate sites** — matching the matrix cell
        exactly — distributed as `workspace_section/mod.rs` 15/23, `watch.rs`
        15/24, `refresh.rs` 9/10, `dialogs.rs` 6/6, `dnd.rs` 6/12,
        `tree_loading.rs` 4/10, `tree_index.rs` 3/3, `test_policy.rs` 2/1, plus
        gate-only sites in `workspace_section/imp.rs` 8, `watch_targets.rs` 7,
        `folders.rs` 3, `actions.rs` 2, `mod.rs` 2. Re-classify all 60 across the
        four kinds (inspection / configuration / actuation / probe); 5a's
        pre-migration classification of the 58 was 22/8/19/9.
      - **Two ungated `_for_benchmark` seams outside that census**, because
        neither is behind `test-utils` and no gate-site grep finds them:
        `workspace_section/mod.rs:608
        child_cache_rebuild_operation_evidence_for_benchmark`, a `pub` function
        imported by `benches/benchmarks.rs:95`, and
        `services/workspace_watch.rs:267 merge_backend_result_for_benchmark`
        (used at `benchmarks.rs:3113`). Add a census row for each and decide its
        disposition in task 6.1: narrowing this row's evidence to the surface's
        visibility, or folding the first into `evidence.rs`, **breaks the bench
        target** unless the decision is explicit. An ungated `pub` bench seam is
        the same class of invisible seam as a test-only field on a production
        struct — and the second one is not even this row's file, so say which row
        owns it.
      - **Test-only override storage.** This row still has **no module statics**:
        its configuration overrides are test-only *fields on production state
        structs* — `RefreshRuntimeState`'s `test_reconcile_batch_delay`,
        `test_scan_delay`, `test_empty_probe_reads` and `WatchRuntimeState`'s
        `test_start_delay`, `test_drop_delay`, `test_worker_starts`,
        `test_last_poll_notices`, `test_disabled` in `workspace_section/imp.rs` —
        plus a `tree_loading.rs` thread-local counter and `watch_targets.rs`'s
        `touched_rows`. **A test-only field on a production state struct is a
        configuration seam that no `static` grep finds.** Re-verify each still
        exists and record it as one.
      - **Pure policy consumer counts** for `model/workspace_scan.rs`,
        `model/workspace_persistence.rs`, and `model/workspace.rs`, counted as
        **owning workflows** rather than referencing files, with substring false
        positives named. Authoring's import-level check found persistence
        consumed by exactly `ui/sidebar/imp.rs` and `ui/sidebar/workspaces.rs`,
        and scan by `folders.rs`, `imp.rs`, and `tree_loading.rs` **plus two
        references to the public `model::workspace_scan` path in
        `crates/lushtext-core/benches/benchmarks.rs` (`:57`, `:3198`)**. Confirm
        by import rather than substring, and confirm **no `services` or `model`
        consumer**, which would forbid the move outright.

      Record the whole re-derivation in `evidence/census-reverification.md`
      (summary in A.2), and state each correction's **direction**. An unchanged
      cell is not the expected outcome.
- [x] 0.4 **Confirm the reconciled stage trace from the code**, do not inherit the
      number. Slot 5a recorded **11 stage orders, 27 deferral primitives, and 11
      non-primitive callback resumptions = 38 resumption points**, against the
      matrix's `Workflow Stage Traces` floor of **five** — a floor off by ~7.6x,
      the widest in the programme. Re-walk the code and attribute **every**
      primitive to exactly one stage order so the subtotals sum, as 5a's
      reconciliation had to (it reassigned `tree_loading.rs:143` from the scan
      order to the DnD shield and named `folders.rs:471` once as shared). The
      eleven: directory scan and expansion; watcher install plus mailbox
      reconcile; targeted in-place refresh; folder-reorder DnD; file
      create/rename/delete; `Space` peek; workspace list add/rename/unlist with
      debounced persistence; workspace-list load; the workspace scope filter fade
      and its settle timer; focused-folder drilldown; and the top-level folder-row
      / empty-probe order.

      **Decide the twelfth candidate rather than inheriting its absence:
      workspace folder add and remove.** It has its own entry point, ordered
      stages, and persistence terminal — `dialogs.rs:71 show_add_folder_dialog`
      → `workspaces.rs:305 handle_add_folder_to_workspace` (resolving folder
      identity off the GTK thread) → `:353 apply_add_folder_to_workspace` →
      persist; `:397 handle_remove_folder_from_workspace`; and the section-side
      route at `workspace_section/mod.rs:315
      connect_remove_folder_requested`. `.agents/rules/ui.md` names the
      "add-folder request" as a section callback the sidebar handles itself, and
      `Add Folder` is already in the row's `Entry points` cell, so the stage order
      is documented from two directions while the inherited trace omits it.
      Decide whether it is a twelfth stage order or a stage *within* the
      workspace-list order, and record which — the answer changes the floor
      correction in 9.7, the facade's narration budget in 2.1/9.2, and whether
      `list_execution.rs` owns folder membership (task 5.1).

      Attribute also the primitives that cross out of this row's files: the
      one-shot `pending_rename` handoff (`file_tree_item.rs:35` set at
      `actions.rs:75`, consumed and cleared at `row_factory.rs:308-320`) is a
      **non-primitive callback resumption** in the inline-rename order — control
      resumes when GTK next binds the recycled row — and belongs in the count.

      Record in `evidence/stage-traces.md` and A.4. The resulting number is what
      task 9.1's facade narration must cover and what task 9.7 corrects in the
      matrix.
- [x] 0.5 **Re-verify the six no-materialization code facts**, which 5a confirmed
      from the code and whose line numbers its own fixes may have moved. 5a's
      evidence file tabulates **six** rows while its prose says "five": reconcile
      that count and say which it is. The facts:
      `find_store_for_dir` calling `row.children()` and then inserting into
      `dir_stores` (`workspace_section/tree_index.rs` ~`:389`, `:402`, `:405-408`);
      `visible_child_stores` calling `row.children()` with **no `is_expanded()`
      filter** (~`:483`, `:499`); `expanded_store_index` safe only because of a
      guard (`refresh.rs` ~`:452`); `set_expanded(true)` at four sites
      (`folders.rs` ~`:376`, `:484`, `actions.rs` ~`:65`, `tree_loading.rs`
      ~`:1260`) materializing children **and** firing the `notify::expanded` hook
      that queues a watcher restart; `derive_expanded_paths_from_model`
      incrementing the `expansion_capture_scans` / `expansion_capture_rows`
      counters the surface must itself report (`tree_index.rs` ~`:28-35`); and
      `find_dir_row` evicting from `dir_rows` on a nominal read (~`:370`). Record
      current line numbers in `evidence/evidence-surface-materialization.md`, and
      note that the matrix row's cell still says "**All five** offending code
      facts" — task 9.6 corrects it to the reconciled number.

      Record in the same file the **no-rewalk clause** this row must preserve:
      `.agents/rules/ui.md` reserves `derive_expanded_paths_from_model`
      (`tree_index.rs:28`) for bootstrap, pre-replacement capture, and the test
      oracle, and forbids a targeted in-place refresh from rewalking the
      flattened model to rediscover expansion. Record its call sites (`:60`,
      `:173`, plus the oracle) and which category each is, because task 5.3's
      `tree_index.rs` dissolution is **precisely** the move that could turn the
      oracle into a production caller: the derivation's new home would sit beside
      the code that must not call it.
- [x] 0.6 **Record the contracts as implemented today, verbatim, before any move**,
      in `evidence/durability-contracts.md`: the five `ui/sidebar/AGENTS.md` Local
      Contracts (scan flight, watcher mirror, mailbox cap, DnD shield, row-factory
      ownership); expansion-state authority per `.agents/rules/ui.md` —
      `expanded_paths` is authoritative live state and deferred restore callbacks
      must read it **at apply time**, not clone it at schedule time; the
      `GtkTreeExpander` internal-gesture disable for file rows at
      `row_factory.rs:324-343`; **the `connect_bind` row-recycling cleanup**,
      which `.agents/rules/ui.md` states as its own contract — a recycled
      `ListItem` must have any lingering inline-rename `GtkEntry` removed and its
      label restored to visible — and whose loop is **duplicated** at
      `row_factory.rs:296-305` (in `connect_bind`) and `:391-406` (in
      **`connect_unbind`**, not `connect_bind` as this list first said), the second
      copy also resetting `use_markup`, the drag handle's visibility, sensitivity
      and accessible hidden/disabled state, and the content-box end margin, so a move that touches one copy and not
      the other is a real regression; the peek key controller's `Capture` phase
      and its `focus_allows_peek_shortcuts()` gate; the DnD inert-hover rules (accept
      hover for every row, show/apply only valid top-level same-workspace
      positions, never expand a folder, never materialize descendants, never
      restart a watch, no filled drop rectangle); the inner-`ScrolledWindow`
      `propagate-natural-width=false` contract and the no-horizontal-scrollbar
      rule; workspace persistence's latest-generation semantics and its debounce;
      and the file-operation semantics including 5a's landed rename refusal. Each
      is a preservation obligation, and each must be quotable in task 6.5.
- [x] 0.7 **Catalogue the widget-test reach-through by field name**, not by line.
      5a confirmed **190 ungated `.imp().` sites**, of which **113 of 158 are
      `TemplateChild` widget handles and out of scope**, and **45 are in-scope
      tree-side runtime reads that move here**. Re-derive the 45 per file and per
      field. Follow slot 3a's finding that an ungated `imp()` **write** is usually
      a real drive in disguise: prefer an existing configuration seam plus a real
      drive over adding a counted actuation seam.
- [x] 0.8 **Mandatory `data-safety` pass in explicit mode, before any code.** Slot
      5 ran this pass over exactly this code and found **eleven** findings; the
      programme record's lesson is that a tier-3 slot must budget for more than
      one. Run it, record every finding with severity and site in
      `evidence/data-safety.md`, and treat each confirmed finding as blocking work
      **in this change** per `.agents/rules/preexisting-blockers.md`. If the pass
      again consumes this change's capacity, that is a **recorded deviation** at
      the head of this file with the scope decision and its reason — never a
      silent absorption, and never a partially migrated row (the Completion Rule
      forbids marking the row `migrated` while any role, seam value object,
      evidence surface, or parity claim is missing).
- [x] 0.9 **`git add -N` every new file and directory immediately after creating
      it**, before the first diff-aware gate runs. `check-accessibility-policy`,
      `check-visual-proof-policy`, and `mutants-diff` are diff-aware, and slot 4
      recorded that a green diff-aware gate over untracked files is **unproven**,
      not passing.
- [x] 0.10 Confirm no other slot's deferred work has migrated into this row's
      files: slot 4's two `[~]` items (the live-session paned proof and the
      quiet-machine `bench-compare`) and its three B.3 simplify candidates
      (`drafts/journal.rs`, `local_history/preview_execution.rs`, and the
      `current_window_width` duplication in `ui/window/imp.rs`) remain **slot 4's
      or slot 7's**; slot 5a's `[~]` live and manual proof remains 5a's. Neither
      tick them nor re-plan them. Record the confirmation.

## 1. Apply the two convention amendments and pay the retroactive re-check

- [x] 1.1 **Establish each amendment's basis from the code and the specs before
      amending**, per slot 5a's method:

      - **dissolution**: confirm that `tree_index.rs` and `watch_targets.rs`
        each contain more than one kind of thing (pure arithmetic **plus**
        cache/coordination for the first; pure arithmetic **plus** generation
        newtypes **plus** a snapshot for the second), that no bounded role name
        describes either as a whole, and that the live spec's only stated
        response to "no name fits" is amendment. Quote the sentence.
      - **already-correctly-named**: confirm the live spec's qualification
        paragraph does not scope itself to created/renamed modules, and that
        slot 2b's narrow reading exists only in task prose.
      - **mutation floor**: confirm from the tool's behavior, not from memory,
        that `--re` does not filter struct field-deletion mutants, and record how
        it was confirmed.
      - **parity versus gain**: confirm the live `mutation-testing` requirement
        says nothing about reporting the two separately, and that slot 4 had a
        gain with no before-count while this change has both.
- [x] 1.2 Land `specs/gtk-adapter-module-boundaries/spec.md` and
      `specs/mutation-testing/spec.md` as **MODIFIED** requirements carrying the
      full updated requirement text. Verify both deltas are **pure additions**
      with a per-requirement diff against the live specs showing **zero removed
      non-blank lines**, and record that check.
- [x] 1.3 **Pay the retroactive re-check, per row, individually.** Add a
      `### Slot 5b amendment re-check` subsection to
      `docs/workflow-readability-matrix.md` covering the **nine** migrated rows
      (`WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`,
      `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`,
      `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`, `WFR-NOTES-BOOKMARKS`). Per row,
      answer:

      1. does the row own a coordination module that is **not one cohesive job**,
         or whose name states a pre-convention *topic* rather than a job?
      2. did the row rename or qualify an already-correctly-named sibling for
         symmetry?
      3. for the changes that relocated pure policy or ran focused mutation
         (slots 3a, 4, 5a): does the recorded mutation evidence state the
         unfilterable floor, and does it separate parity from gain?

      **Do not expect confirmations.** Slot 3b, slot 4, and slot 5a each found
      genuine non-compliance, 5a's statement (b) found **six of eight** rows
      non-compliant, and the not-a-confirmation streak stands at three
      consecutive amendments. Fill any gap **in this change**, in the matrix and
      the programme record. An **archived** change's evidence file is history and
      is **not** rewritten — the correction lands in the live matrix cell with a
      pointer saying which archived figure it supersedes. Record in A.6.
- [x] 1.4 Confirm nothing beyond these statements is absorbed: the facade line
      budget, the five role names, the bounded coordination set, the seam
      value-object shape, the per-workflow and nested role homes, and the
      evidence-surface visibility, reentrancy, no-materialization, and
      child-collection rules are **not** amended here.

## 2. Confirm the inherited decisions, and re-measure the two that moved

- [x] 2.1 **Re-measure the facade budget projection before any facade text is
      written.** Slot 5a projected **≈351 of 370** by arithmetic over
      `ui/sidebar/mod.rs` **at 406 lines**; the file is now **415**, and
      **three** of its four subtrahends are line-range extractions whose ranges
      are stale by construction — `WorkspaceSidebarWidthPreset` (recorded at
      171–273), `SidebarFileRowStateSnapshot` (56–91), and
      `WorkspacePersistenceFlushError` (34–54). **Do not carry those ranges
      forward**: re-derive each subtrahend against the current file by reading it,
      re-project, and record both the old and the new projection in
      `evidence/facade-measurements.md`. Fold in whatever task 0.4 decided about
      the twelfth stage-order candidate, which costs narration lines. Then carry
      the escalation path, in order:

      1. **delegate harder** — slot 2b's exact sequence: delegate every stage
         body, compress each inversion to one line, fold module-ownership detail
         into the role table and the shared-state table;
      2. **escalate in-change with the measured count** — a convention amendment
         costing a **nine-row** retroactive re-check. Make the case with the
         honest measurement or make the narration fit; do neither by editing the
         budget line quietly;
      3. **split the census row** — available **only** on new evidence, because
         5a already weighed and rejected it: the two halves are not independent
         (workspace add/unlist creates and destroys the very sections the file
         tree lives in, `load_workspaces` is the single entry point for both, and
         both share `current_scope`, `workspaces_file`, and the persistence
         debounce). Reversing that needs new facts, not a budget problem.

      Also confirm slot 5a's own budget finding, which sharpens the programme's
      model: its five-stage-order facade fits in **178**, so stage-order count
      alone is not the pressure. Say what this row's eleven stage orders actually
      cost.
- [x] 2.2 **Confirm the role home and the module classification** recorded in 5a's
      `evidence/shared-ownership-decisions.md` §2.5: canonical role home
      `ui/sidebar/` with nested bounded coordination modules inside
      `ui/sidebar/workspace_section/`, exactly one `policy.rs` and one
      `evidence.rs` at the canonical home. Confirm module by module against the
      current code — 5a's fixes changed several of these files — and record any
      deviation with its reason. This is the **first adopter** of the nested role
      home; report how it reads.
- [x] 2.3 **Confirm the `journal` verdict for workspace persistence** — *not* a
      journal, `execution` with latest-generation supersession, named
      `persist_execution.rs` — by re-applying slot 3a's test: *does a later stage
      of the same workflow read the record back*, and is that read-back
      **recovery from a failure** or ordinary next-launch load? 5a's answer was
      the latter: no generation in the file, no stale-record cleanup, a failed
      write leaves the previous file intact and awaits **explicit** user retry.
      Confirm; do not re-decide. Apply the same test once to the **expansion
      state** persisted with the workspace file, if any, and record the verdict.
- [x] 2.4 **Confirm the excluded scope**, where a reader hits the adjacency:
      `WFR-SHELL-LAYOUT` (slot 7) keeps the sidebar show/hide animation and its
      `workspace-sidebar-animation` blocker — which **follows the animation, not
      the row name**, 5a's reusable form of "a field whose name contains *save*
      is not thereby save-workflow state" — and the recent-documents surface;
      `WorkspaceSidebarWidthPreset` is that row's value and leaves this facade;
      `ui/sidebar/file_tree_item.rs` has no coordination tier; `WFR-MINIMAP`
      (slot 6) keeps its four `ui/automation.rs` reach-throughs;
      `ui/window/startup_data.rs` stays **cross-cutting** (5a's option (3)) even
      though it calls `sidebar.load_workspaces()`; and the ten shared services
      **stay**, with a `services -> ui` relocation forbidden outright.
- [x] 2.5 **Confirm the closed boundaries are not re-opened**:
      `model/workspace_search.rs`, `model/file_load.rs`,
      `model/buffer_replacement.rs`, `model/editor_memory.rs`,
      `model/migration_ledger.rs`, `ui/plain_disposal.rs`,
      `ui/buffer_snapshot.rs`, `services/single_flight.rs`, and
      `services/sync.rs`. Record the confirmation so a reader does not think a
      question is open.
- [x] 2.6 Record all of section 2 in `evidence/shared-ownership-decisions.md`,
      framed as **confirmations of 5a's decisions plus the two re-measurements**,
      with a summary in A.3. Where a confirmation failed, say so plainly and give
      the deviation its own row.

## 3. Pure policy: extend the row's `policy.rs`, and relocate the two model modules

- [x] 3.1 **Extend the existing `ui/sidebar/policy.rs`** (261 lines at authoring,
      landed by 5a with the rename intent, the destination-collision refusal, the
      unique-name candidate sequence and its attempt ceiling, and the prefix
      matching rule). Add, in two groups:

      **Already-pure free functions that only need relocation** — refresh-directory
      minimization with its common-prefix/suffix helpers, the desired-folder-rows
      computation, the changed-path-to-owning-directory resolution (which needs a
      **key-set view** rather than a `ListStore`, because a policy module may not
      import GTK), the DnD post-drop index and before/after edge computations plus
      the hover verdict and the payload encode/decode, the watch target-for-folder
      and start-error message, the peek metadata/size/time formatters, the icon
      selection pair, the context-menu key predicate and header-background hit
      test with their declarative spec tables, the row accessible-description
      builder, and the file-row open/active visual state.

      **Decisions currently inline in the GTK adapters, which are the highest
      value** — refresh coalescing (full versus paths, cap-overflow promotion to
      full, manual versus auto debounce, drop-when-already-full), the
      full-versus-directories refresh verdict, the desired-versus-current
      top-level row diff that drives the splice window, the **readiness
      predicate** over its scalar inputs, the **expansion transition rule**
      (collapse prunes descendants, with its ambiguity fallback) and the rename
      prefix rewrite, the persistence error-to-message mapping and terminal-effect
      routing, and the auto-expand-versus-remembered-intent decision.

      **Pin the caps and budgets this row owns to concrete literals with the
      user-facing reason beside them** — the shared mailbox path cap, the
      reconciliation batch size, the scan admission ceiling, the persist debounce
      — and assert against those literals rather than against the constants, so a
      changed constant fails a test rather than silently redefining the contract.
- [x] 3.2 Keep `policy.rs` importing **no** `gtk4`, `glib`, `gio`, `libadwaita`,
      or `sourceview5`. Run `make check-workflow-boundaries` after every move
      rather than once at the end; it names the file and the offending import.
- [x] 3.3 **Relocate `model/workspace_persistence.rs` (338 lines, raw total
      including its co-located tests)** into the row's policy home with its tests.
      Consumer set verified by import at authoring as exactly `ui/sidebar/imp.rs`
      and `ui/sidebar/workspaces.rs` — no `services`, no `model`, no bench — so it
      is the programme's cleanest relocation. **Re-verify by import, not
      substring, before moving.**
- [x] 3.4 **Relocate `model/workspace_scan.rs` (231 lines, raw total)**, handling
      the complication rather than discovering it: besides three `ui/sidebar`
      importers, `crates/lushtext-core/benches/benchmarks.rs` references the
      **public** `model::workspace_scan` path at two sites (`:57` a `use`, `:3198`
      a fully-qualified return type). A move is therefore a **public-path break
      for the bench target**. Decide between updating the bench imports and
      keeping a precisely scoped `pub` subset — slot 3a's `save_admission`
      precedent — and record the reason. Confirm again that no `services` or
      `model` consumer exists, which would forbid the move outright.
- [x] 3.5 **Prove mutation parity for both relocations, before and after.** Both
      sources are in `model/**` and therefore **already inside `examine_globs`**,
      so unlike slot 4's relocations there **is** a before-count and parity is a
      real claim that can fail. Record the exact `make mutants-diff` invocation
      with **file-level anchors**, the generated and killed counts on both sides,
      and an account of every difference. Note the working-tree constraint
      explicitly: `mutants-diff` compares against `origin/main` and
      `MUTANTS_IN_PLACE=1` **refuses a dirty worktree** outside CI, so use the
      default copy-based mode (or a disposable worktree with a **short** path) and
      say which was used. A relocation whose mutants are no longer generated is a
      coverage regression, not an acceptable consequence of the move. Write to
      `evidence/mutation-workspace-tree-policy.md`.
- [x] 3.6 **Report the extraction gain separately from the relocation parity**, as
      this change's own `mutation-testing` amendment now requires: the task 3.1
      extraction out of the GTK adapters has **no** before-count and is a gain
      from zero; the two relocations have one and owe parity. One aggregate number
      would let a parity loss hide behind the gain.
- [x] 3.7 Confirm `model/workspace.rs` (799) stays in `model/` as **domain**, with
      its consumer count re-derived as **owning workflows** and any `services`
      consumer named. Update the `Policy Module Census` rows for all three
      workspace modules in task 9, and leave a pointer where a reader following the
      old census snapshot would otherwise think a decision is still open.
- [x] 3.8 Confirm `policy.rs` is reachable by `examine_globs` through the literal
      `ui/**/policy.rs` convention **at the canonical role home**, and that
      `make check-policy` passes.

## 4. Seams: reify the last required value object, and re-audit the existing ones

- [x] 4.1 **Reify `WorkspaceWatchTicket`** in the row's existing
      `ui/sidebar/seams.rs`, carrying `{targets_generation, lifetime_generation}`.
      Today the pair travels into the watch-install worker as a loose
      `(section_weak, generation, lifetime)` tuple and is compared clause-by-clause
      in the completion closure, where a **lifetime** mismatch retires the watcher
      and a **generation** mismatch re-enters the install. Construct it once at
      dispatch and validate it as a unit, keeping the two consequences
      distinguishable — a single `bool` predicate collapsing "this section is
      gone" into "this generation is stale" would lose the retire-versus-restart
      decision. Follow the established Ticket/Facts/predicate shape and 5a's
      naming lesson: if the method decides whether a completion may act rather
      than asking whether something is current, name it for the decision.
- [x] 4.2 **Re-audit the scan side rather than re-inventing it.**
      `model/workspace_scan.rs` already reifies `WorkspaceScanTicket` with a
      one-active/one-latest flight and its metrics. Audit it against the
      two-boundary rule, confirm it is constructed once per admitted scan and
      validated as a unit, and record it as an existing seam value object that
      needed no change — or reify what the audit finds. Do the same for
      `watch_targets.rs`'s two generation newtypes and its snapshot, which task
      5.3 dissolves into `seams.rs` and `evidence.rs`.
- [x] 4.3 Confirm `FileOperationTicket` + `FileOperationFacts` + `row_is_current`,
      which 5a landed as the fix for the wrong-row/stale-watch defect, still
      satisfies the two-boundary rule after this change's module moves, and that
      no call site reconstructs it inline. Record it as **done, unplanned** in the
      matrix's Seam Value Objects section.
- [x] 4.4 Re-audit for any remaining unreified bundle crossing two or more
      function boundaries or reconstructed at two or more call sites, and for any
      `#[expect(clippy::too_many_arguments)]` on a cross-module workflow boundary
      in this row's files — which the convention treats as a marker of an
      unreified seam, not an accepted exception. Record the count found and
      whether it moved. **Two candidates authoring found, to weigh rather than
      discover:**

      - **The inline-rename widget triple `(entry, label, content_box)`** —
        reconstructed at five sites and crossing three function boundaries today,
        and after task 5.2 it spans **`file_execution.rs` and `row_factory.rs`**,
        a role boundary *this change creates*. That is exactly the seam rule's
        trigger: a bundle whose parts are passed positionally across a boundary
        the reader must now cross to follow the rename. Reify it in `seams.rs`, or
        record why the three parts are better passed individually. Note that the
        recycling cleanup contract reads the same triple from the other side
        (task 0.6), so a mismatch between the two readings is invisible to review
        — the archetype defect the rule exists to make unrepresentable.
      - **`apply_add_folder_to_workspace` (`workspaces.rs:353`)** takes six
        parameters across a private boundary, three of which are parallel
        existing-paths / identity / existing-identities slices whose order is
        positional. Weigh it against the two-boundary rule once task 0.4 has said
        whether folder add/remove is its own stage order.

## 5. Implement the nested role home, and dissolve the two modules that are not one job

- [x] 5.1 Create the canonical-home coordination modules confirmed in task 2.2:
      `list_execution.rs` (workspace-list load and add/rename/unlist),
      `persist_execution.rs` (the persistence pipeline with its latest-generation
      state and close-time flush), and `filter_execution.rs` (the workspace scope
      filter fade and its settle timer, whose `filter_animation_active` state this
      row projects to automation). Decide `list_execution.rs`'s scope against
      task 0.4's verdict on **workspace folder add/remove**: either it owns
      folder membership alongside the workspace list, or that stage order gets
      its own coordination module. Say which, and give the dialog route
      (`dialogs.rs:71`) and the section request route
      (`workspace_section/mod.rs:315`) a named destination either way.

      Move `WorkspaceSidebarWidthPreset` out to `ui/sidebar/width_preset.rs` and
      record it as `WFR-SHELL-LAYOUT`'s, not this row's. **It has three consumers
      outside this row that the move re-points** and that must be named in the
      change rather than discovered by the compiler:
      `ui/preferences/imp.rs` (4 references), `ui/window/adaptive_shell.rs` (12),
      and `ui/window/imp.rs` (6). `adaptive_shell.rs` is `WFR-SHELL-LAYOUT`'s own
      file, so the touch must stay a **path edit proved by compilation** — not a
      restructuring of a row this change does not own.
- [x] 5.2 Create the nested coordination modules, **each with its source named**
      — 5a's map left the first two sourceless, which is what let the row's
      largest file go unclassified: `scan_admission.rs` (`admission`: the
      per-child-store scan flight, its process-global permit/limit/high-water at
      `tree_loading.rs:73-104`, and the admission retry at `:419-478`),
      `scan_execution.rs` (the rest of `tree_loading.rs`: the child scan worker,
      child-store identity/mirror/splice, batched reconciliation, directory-state
      clearing, the deferred expansion restore, and child-store materialization,
      plus `tree_index.rs`'s child-store lookup and cache maintenance per task
      5.3), `refresh_execution.rs` (refresh coalescing and
      planning, from `refresh.rs`), `folder_execution.rs` (top-level folder rows,
      the empty probe, and focused-folder drilldown, from `folders.rs`),
      `file_execution.rs` (create, inline rename, delete, from `actions.rs`),
      `peek_execution.rs` (`Space` peek, from `peek.rs`), and
      `reorder_execution.rs` (folder-reorder DnD, from `dnd.rs`). **`watch.rs`
      keeps its name** — it is already a correct bounded role name and this
      change's own amendment forbids renaming it for symmetry. Record that
      explicitly so a reader does not read the asymmetry as an oversight.
- [x] 5.3 **Dissolve the three modules that are not one coordination job**, per
      this change's amendment, recording each part's destination:

      - **`tree_loading.rs` (1,269 raw — the row's largest file, which 5a's map
        never classified).** Authoring's read: the scan admission permit, limit,
        and high-water statics (`:73-104`) and the admission retry (`:419-478`)
        → `scan_admission.rs`; `populate_child_store`, `start_child_scan`,
        `finish_child_scan`, the reconcile-batch loop, child-store identity,
        mirror generation and splice, the record-insert/remove/path-update
        operations, directory-state clearing, and
        `schedule_child_state_restore` → `scan_execution.rs`; and
        `empty_children_model_for_drag_hover` (`:130`) with its two seams
        (`:157`, `:163`) → `reorder_execution.rs`, which is consistent with 5a's
        own stage-trace reconciliation reassigning `tree_loading.rs:143` from the
        scan order to the DnD shield. **Verify this read against the file rather
        than adopting it**, and give `build_children_model` (`:115`, the
        `GtkTreeListModel` create function) an explicit destination: it is the
        materialization entry point the evidence surface must never reach.
      - `tree_index.rs` (969 raw / 844 production) — pure index arithmetic
        (splice windows,
        changed-path→owning-directory, common prefix/suffix, desired-versus-current
        diff) to `policy.rs`; child-store lookup and cache maintenance to
        `scan_execution.rs`, where child stores are materialized. This was the one
        module with a real risk of forcing an escalation for an `index` role;
        dissolving it is the honest alternative.
      - `watch_targets.rs` (337 raw / 264 production) — the two generation
        newtypes to `seams.rs`, the mirror arithmetic to `policy.rs`, the
        snapshot to `evidence.rs`.

      **No dissolution may leave a part without a role destination**: that would
      mean the module was not fully understood, which is the failure mode the
      amendment's scenario names. Three dissolutions rather than two also
      strengthens the amendment's basis — record that in task 9.10, because a
      pattern that recurs three times in one row is evidence the escalation-only
      spec text was wrong rather than merely incomplete.
- [x] 5.4 **Record every remaining module as a called presentation surface**, which
      is not a role — **nine** of them, not the six this change's first draft
      said: `ui/sidebar/callbacks.rs`, `dialogs.rs`, `imp.rs`, and
      `workspace_section/{mod.rs, imp.rs, row_factory.rs, context_menus.rs,
      row_accessibility.rs, icon_presentation.rs}`. Count them in the artifact
      that records them, so the matrix row (task 9.5) states the number that is
      actually true. Each keeps every behavior
      obligation the `gtk-adapter-module-boundaries` requirement "Workspace-section
      wiring has focused owners" places on it, with its ownership stated in its own
      module doc and named in the matrix row. `row_factory.rs` is classified this
      way **precisely so** no role move touches the `GtkTreeExpander`
      internal-gesture disable at `:324-343`; verify that block is byte-identical
      afterwards by diff.
- [x] 5.5 **Prove the moves are literal.** Perform every move as a slice of the
      original file and verify by diff that no statement was lost or reordered,
      naming the blocks whose exact preservation matters: the `GtkTreeExpander`
      gesture disable, the peek controller's `Capture` phase and
      `focus_allows_peek_shortcuts()` gate, the DnD inert-hover target setup, the
      deferred expansion-restore callbacks that must read `expanded_paths` at
      apply time, **both copies of the rename-entry cleanup loop
      (`row_factory.rs:296-305` in `connect_bind` and `:391-406` in
      `connect_unbind`, the second also resetting `use_markup`, the drag handle,
      and the content-box end margin — an asymmetry that is the unbind-side reset
      of state `connect_bind` sets affirmatively, and must be preserved rather
      than "fixed")**, the `pending_rename` one-shot handoff
      at `:308-320`, and 5a's rename refusal and placeholder cleanup. Record in
      `evidence/tree-behavior-equivalence.md`.
- [x] 5.6 Update `crates/lushtext-core/src/ui/sidebar/AGENTS.md` **in the same
      breath** so its Responsibilities, Local Contracts, and Editing Rules
      describe the migrated shape rather than the pre-migration one, with every
      contract re-pointed to its new owner and none dropped.

## 6. Evidence surface, seam retirement, and the reach-through

- [x] 6.1 Build one `ui/sidebar/evidence.rs` at the canonical role home, at the
      narrowest visibility its readers require, folding in **every**
      pre-convention typed observation so no second path survives:
      `WorkspaceScanPressureEvidence`, `WatchTargetSnapshot`,
      `WorkspaceWatchMailboxSnapshot` as this row reads it,
      `SidebarFileRowStateSnapshot`, the refresh and reconciliation metrics, the
      child-cache rebuild metrics, the scan admission active/high-water counters,
      the expansion capture metrics, and the DnD hover fallback count. A
      pre-existing wider evidence type is **narrowed** to the surface's
      visibility rather than left wide.

      **Two facts make this surface harder than the field list suggests, and both
      must be decided rather than papered over:**

      - **The scan admission counters are process-global, not per-section.**
        `ACTIVE_WORKSPACE_SCAN_TASKS` and `WORKSPACE_SCAN_TASK_HIGH_WATER`
        (`tree_loading.rs:77-78`) are `AtomicUsize` statics guarding a
        process-wide `WORKSPACE_SCAN_TASK_LIMIT` of 4 across **all** sections,
        while task 6.3 requires every aggregated field to be bounded and honest
        with zero workspaces. A field reading a process-global counter cannot be
        described as per-section, and a read on a window with zero workspaces
        would still report scans belonging elsewhere. Decide between per-section
        accounting and **naming the scope honestly in the field's own name and
        doc**, and record which. An inherited global that the surface silently
        presents as row state is the same class of quiet mismatch the
        materialization rule exists to catch.
      - **Two ungated `pub` bench seams read this row's internals**:
        `workspace_section/mod.rs:608
        child_cache_rebuild_operation_evidence_for_benchmark` (imported at
        `benches/benchmarks.rs:95`) and
        `services/workspace_watch.rs:267 merge_backend_result_for_benchmark`
        (used at `:3113`, and owned by the **service**, not this row). Narrowing
        this row's evidence to the surface's visibility breaks the bench target
        unless each has an explicit disposition: keep it as a documented bench
        seam beside the surface, re-express the bench against the surface, or
        leave the service-owned one alone with its owner named. Decide, record,
        and prove the bench still compiles (task 10.9).
- [x] 6.2 Discharge the three standing proofs:

      - **tight-borrow discipline**: compute every derived scalar into a local
        and drop each `Ref` before the struct literal, and record the constraint
        in the module doc where the surface is defined;
      - **the disposed-widget rule**: every `TemplateChild` read through
        `try_get()`, including one reached **transitively** through an ordinary-
        looking operation — the shape that panicked on 5a's first run, from
        `active_editor()` rather than from a direct read. This row reaches N
        sections plus a window, so look for the transitive path deliberately;
      - **the reentrancy proof**: drive the workflow through **each** operation
        that takes a mutable borrow of state the accessor reads, read the surface
        **after** each one, and assert repeated reads of unchanged state are
        identical. Do **not** write a test that reads the surface while a borrow
        is held: that is the panic the constraint prevents, not a demonstration
        of it.
- [x] 6.3 Discharge the two proofs this row exists for, which no prior surface has
      owed, recording both in `evidence/evidence-surface-materialization.md`:

      - **no materialization.** The surface MUST NOT call any accessor that runs
        the `GtkTreeListModel` create function, populates a child store, or
        advances the full model derivation. It MUST derive from `expanded_paths`
        — the authoritative live set — rather than repeating any of the six
        guarded or unguarded walks from task 0.5. **Prove it, do not assert it**:
        read the surface with rows collapsed **and** with rows expanded, and show
        the scan admission counters, the child-store registry, the watcher
        generation, and the expansion capture metrics **identical before and
        after** each read, and that no worker started and no watcher restart was
        queued.
      - **child collection.** Every field aggregated across sections is bounded,
        answers honestly with **zero workspaces**, and **skips a disposed section
        rather than panicking on it**. Write the disposal proof before believing
        the surface is safe.
- [x] 6.4 **Retire this row's seams — the largest population in the programme**,
      60 functions across 111 gate sites. Per kind:

      - **inspection**: retire into the surface with **no remaining callers**,
        including the destructive-read "take touched rows" seam, whose reset must
        be separated from its observation so counting does not mutate;
      - **configuration**: collapse into the existing `ui/sidebar/test_policy.rs`,
        entirely behind `#[cfg(feature = "test-utils")]`, keeping every public
        setter name. This includes the **eight test-only fields inside
        `RefreshRuntimeState` and `WatchRuntimeState`** that no `static` grep
        finds and that currently make production state structs carry test
        storage, and `WatchRuntimeState`'s permanent restart-suppression flag,
        whose meaning must be preserved **exactly**;
      - **actuation**: classify each (the six dialog bypasses, the watcher
        merge/disconnect/poll/stop drives, the refresh queue/apply drives, the DnD
        hover simulations) as a programme-level deferral with its reason;
      - **probe**: preserve the oracles and lifecycle probes with their reason —
        the readiness-predicate oracle, the derived-expansion oracle, the
        indicator-would-show predicates.

      Add **exactly one** new seam: the load-worker delay M-4's driven race test
      needs (task 8.1). Count and justify it individually at its definition, as
      5a did for its two. No override storage may compile without the test
      feature. Record before/after counts per kind in `evidence/test-counts.md`.
- [x] 6.5 Do **not** weaken a test to make a retirement possible. If a test needs a
      fact the surface does not expose, **extend the surface** — never add a
      second narrow getter, which is the regression back to the shadow
      introspection API the surface replaced.
- [x] 6.6 Migrate the **45 in-scope tree-side runtime reads** from task 0.7: private
      runtime reads become evidence reads, and any write becomes a real drive
      through an existing configuration seam wherever possible. Record the
      out-of-scope `TemplateChild` handle population (113 of 158) and its reason
      so a later slot does not read the omission as an oversight. Write to
      `evidence/widget-test-reach-through-migration.md`, per file and per field,
      before and after.
- [x] 6.7 **Retire the two production `.imp()` reach-throughs that are this row's**:
      `ui/automation.rs:766` (readiness blocker) and `:927` (workspace snapshot),
      both reading `imp.sidebar.imp().workspace_filter_animation_active`. They
      become a named accessor on the sidebar facade or a projection from the
      evidence surface, with the readiness blocker and the snapshot field keeping
      **identical values**. Record the six out-of-scope production reach-throughs
      with their owning row and **do not fix them here**: `:518`/`:519`
      (`window.imp().tab_view`, `WFR-SHELL-LAYOUT`, slot 7) and
      `:1144`/`:1151`/`:1169`/`:1231` (editor/minimap, `WFR-MINIMAP`, slot 6) —
      re-derived at this change's authoring, because 5a's handoff records the
      pre-fix line numbers `:1137`/`:1144`/`:1162`/`:1224` and this change's
      first draft copied them. Re-verify before writing them into the matrix.
      Fixing one from outside is how a migrated row acquires a change nobody
      planned. Close the matrix's reach-through table rows for the two retired
      sites.
- [x] 6.8 Reconcile the shared widget-test harness configuration: after the
      collapse into `test_policy.rs`, the harness must set the same behavior
      through the new owners with **no test-visible timing change**. Keep the
      shared wait helpers from `crates/lushtext/tests/widget/common.rs` —
      `wait_until`, `flush_events`, `flush_after_delay`, `present_window` — and
      **do not add a private copy** or change a working helper's mechanism. This
      row's tests are the ones most likely to tempt a private copy: they live in
      the two heaviest widget modules in the tree.
- [x] 6.9 Run mutation on the extended `policy.rs` before writing this row's
      behavior-equivalence file, and record generated/killed/missed/unviable with
      the exact invocation and file-level anchors, reporting the extraction gain
      separately from task 3.5's relocation parity and stating the field-deletion
      floor.

## 7. Automation: project `window.workspace` from evidence without widening

- [x] 7.1 Identify the exported surface exactly, from `model/automation.rs` and
      `docs/automation-reference.md` rather than from memory, and record the
      **pre-change values**: the `window.workspace` object's ten fields
      (`scope_kind`, `scope_workspace_id`, `scope_workspace_name`,
      `workspace_count`, `folder_count`, `scoped_folder_count`, `no_workspaces`,
      `persistence_inflight`, `persistence_dirty`, `filter_animation_active`);
      the `workspace-persist`, `workspace-tree-refresh`, and
      `workspace-filter-animation` readiness blockers; the
      `workspace-refresh-complete` predicate and the `workspace-refresh` workflow
      id; and every predicate that lists one of those blockers, which includes
      `app-startup`, `recovery-restore-complete`, `visual-geometry-settled`, and
      `accessibility-settled`.
- [x] 7.2 Make every field project from the new evidence surface instead of
      re-deriving from widgets, keeping names, types, and semantics **unchanged**.
      Where a readiness blocker needs one bool, read it through a cheap facade
      accessor **identical by construction** rather than building a whole surface
      per poll — the pattern 3a, 3b, 4, and 5a all used. This row's readiness
      predicate is itself a pure function over scalars and belongs in `policy.rs`,
      so the blocker and the surface can be identical by construction rather than
      by inspection.
- [x] 7.3 **Register `window.workspace` as a new projecting object** in
      `docs/automation-reference.md`'s Evidence Projection Map, keyed by evidence
      type and attributed by the binding each field is read through. Verified at
      authoring: the map holds rows for `window.content_search`,
      `window.command_palette`, `window.tabs`, `window.local_history`, and (from 5a)
      `window.notes` — **no `workspace` row exists**. Registering a new projecting
      object is different work from extending attribution. Honour 5a's recorded
      rule above the map for a fact that belongs to neither workflow: the same
      user-visible fact must be reached exactly one way.
- [x] 7.4 Confirm every other new evidence field — generations, tickets, admission
      and mailbox counters, expansion sets, retained weights, queue depths,
      truncation state — is internal and reaches **no** snapshot, and that no
      absolute filesystem path beyond the already-bounded scope fields can reach
      the schema. The existing redaction tests are the contract.
- [x] 7.5 **Honour 5a's ownership decisions rather than re-deciding them**:
      `workspace-sidebar-animation` is `WFR-SHELL-LAYOUT`'s because the blocker
      follows the animation, not the row name; the palette's
      `command-palette-index` disjunct **stays a direct call**; and the two
      recorded **absences** (the notes browser dialog's coordinators and the
      startup format-upgrade flow have no readiness blocker) are the status quo,
      not gaps to fill. Adding one would be widening.
- [~] 7.6 **DEFERRED, recorded rather than claimed — the two-tree capture-and-diff was
      not run.** What stands in its place is recorded in
      `evidence/automation-no-widening.md`: an unchanged schema verified from
      `model/automation.rs`, a passing `make check-automation-docs --self-test`, a
      passing `make automation-client-self-test`, the preserved blocker-list asymmetry, a
      **live** single-tree D-Bus capture of `window.workspace` matching the contract
      field for field, and — added in the fix cycle — the object's **registration** in
      the drift gate's `EVIDENCE_PROJECTIONS`, proved to reject a rename injected on
      either side. That registration is what makes the ten documented rows gated rather
      than merely written, which is the durable half of what the two-tree diff would have
      shown once. Original task text: **prove no widening rather than asserting it**, and close the gap 5a
      left: run `make automation-smoke` on a pre-change tree and on the changed
      tree under isolated headless Mutter and a private D-Bus session with the
      same fixtures, diff the `workspace` object, the action catalog, and **all**
      readiness predicates to zero differences, and record the normalizations
      applied and why each is about the fixture rather than the contract. Slot 5a
      did **not** run this two-tree capture-and-diff; this row's object is ten
      fields and three blockers, so the gap is not narrow here. Write to
      `evidence/automation-no-widening.md`, and **keep the comparison worktree's
      path short** — slot 4 lost a run to `libmutter-ERROR: Failed to create
      socket` under a deep scratch path, a message that says nothing about path
      length.
- [x] 7.7 Carry `WFR-AUTOMATION-SPINE` forward as `(partial)`: on slot 5b's
      complete ledger line and on slot 6's outstanding line and remaining-scope
      row. It stays `pending` in the matrix rather than `migrated`, because it
      continues per migrated workflow; marking it `migrated` to satisfy a gate
      would be a false claim.
- [x] 7.8 Run `make check-automation-docs`, and
      `make automation-client-self-test` if the client changed.

## 8. Data safety over the restructured file operations

This section is the highest-risk work in the change: this row renames and deletes
**the user's own documents**, it is `tier-3` throughout, and slot 5's pass over
exactly this code found eleven findings. A confirmed finding is fixed **in this
change** with a regression test **proved to fail without its fix** — the method 5a
established, which caught two tests that passed against the broken code as well.

- [x] 8.1 **Candidate: the M-4 driven race test, which 5a named this slot's
      highest-value remaining test.** The fix landed in `ui/sidebar/workspaces.rs`:
      `load_workspaces` captures `requested_generation()` before dispatch and
      skips `build_sections_from_file` when a mutation superseded it, because
      `persist()` has already scheduled that mutation for disk and adopting the
      loaded file would revert an in-memory workspace the user just created while
      its write was pending. 5a proved it **by the guard's shape only**. Drive the
      real race: force a "New Workspace" between the load dispatch and its
      completion using the one new configuration seam from task 6.4, and prove the
      test fails with the guard reverted.
- [x] 8.2 **Candidate: H-5's covered-by-construction claim.** 5a recorded rename
      and delete acquiring `TargetWriteGuard` as proved "by construction plus
      existing coverage", not by a driven race. Now that the file operations move
      modules, re-confirm the guard is acquired on **every** path — including any
      path this change's restructuring creates or reorders — and decide whether a
      driven test is now cheap enough to add. Record the verdict either way.
- [x] 8.3 **Candidate: the persistence path as an amplifier for M-8.** M-8 is
      cross-cutting (`services/recovery_metadata.rs` classifies a transient read
      failure identically to structural corruption, quarantines the live
      `workspaces.json`, and returns default state with `replacement_allowed =
      true`) and is **not** this row's defect. But its consequence is **this row
      persisting an empty configuration over the user's workspace list**. Confirm
      from the code whether this row's persistence path can refuse to write empty
      state it did not derive from user intent, or whether that must stay with the
      owning capability, and record the verdict with its reason. A "no, it belongs
      upstream" with evidence is a complete answer.
- [x] 8.4 **Candidate: ordering of file operations against the watcher, the
      expansion set, and the sidecars, after the module move.** A rename updates
      the expansion set, clears directory state, sets the row path, refreshes the
      watch row, updates the item cache, fires the rename callbacks, and **then**
      triggers sidecar migration through the migrated notes facade. 5a confirmed
      the ordering guarantee is **this row's**, and found one real stranding
      (fixed). Re-confirm after the move that no reordering can lose a sidecar,
      resurrect a stale watch target, or leave the expansion set describing a path
      that no longer exists — and that `migrate_note_sidecars_after_rename` is
      still called after those updates settle, as a **call** into a migrated row.
- [x] 8.5 **Candidate: the deferred expansion restore reading live state at apply
      time.** `.agents/rules/ui.md` requires every deferred restore callback —
      `schedule_child_state_restore` being the one implementing site; the rule's
      former second name `restore_materialized_state` was a **phantom symbol**
      that never existed and is corrected in this change — to read `expanded_paths` **at apply time**, not
      clone it at schedule time, so a user collapse between scheduling and the
      callback is never resurrected. Confirm the property survives the move into
      `scan_execution.rs` / `folder_execution.rs`, and that the evidence surface's
      no-materialization derivation did not accidentally become a second source of
      truth for expansion.
- [x] 8.6 **Record the boundary for the coverage gap that is not this row's.** 5a
      left four unproven pass-2 defects (P2-2 through P2-5: the empty-set
      `sidecar_resolved` deferral, the flush-versus-in-flight race, and the Save
      As reset) in `ui/window/notes/bookmark_execution.rs`, whose drivers are tab
      close and Save As — **neither a tree file nor a tree entry point**. Record
      the boundary with `WFR-NOTES-BOOKMARKS` named as owner, so a later slot does
      not read the omission as an oversight, and confirm this change's tree-side
      rename path does not become a fourth driver.
- [x] 8.7 Run the `data-safety` skill in explicit mode again **over the finished
      diff**, aimed specifically at hazards this change's own restructuring could
      introduce. Slot 5a's second pass found **seven** defects each introduced by
      an earlier fix in the same change; a pass over a diff that moves eleven
      stage orders across fourteen modules owes at least that suspicion. Record
      both passes with every finding and its disposition in
      `evidence/data-safety.md`.

## 9. Facade, matrix, and record completion

- [x] 9.1 Write the facade's module-doc narration **from the code**, not from the
      stage-trace file, naming all eleven stage orders and, for each inversion,
      the point where control resumes. Delegate every stage: the facade owns no
      timer, no admission bookkeeping, no generation counter, and no widget
      mutation. Carry a **"State this workflow shares with others"** table, the
      form the load facade established — it is how a reader learns that the
      cross-cutting startup gate calls `load_workspaces()`, that the file-row
      state snapshot is pushed down from the window, and that the sidebar's
      structure-changed signal drives the palette's file index, without opening
      those files. Name explicitly the inversion that most needs naming: the
      **deferred expansion restore** that must read live state at apply time.
- [x] 9.2 **Measure the facade against 370 physical lines** and record it beside
      task 2.1's re-projection in `evidence/facade-measurements.md`. If it does
      not fit after the full delegation sequence, escalate in-change with the
      measured count per task 2.1; do not edit the budget line quietly.
- [x] 9.3 **Protect the other facades' headroom.** `ui/search_panel/mod.rs` sits at
      369 of 370: do not add a physical line to it. Re-measure the notes (178),
      save (223), load (271), palette (335), and four slot-4 facades and confirm
      none is pushed over. Slot 5a found **three of eight** previously recorded
      facade sizes stale, so re-measure rather than reading the recorded numbers.
- [x] 9.4 **Run the rustdoc lint gate.** It is in neither `make check` nor
      `make pre-commit` nor `make check-policy`; CI's `Lint` job enforces it, and
      slot 3a shipped this exact failure. A new `pub` facade naming its own private
      coordination modules and `pub(crate)` seam types is precisely the
      `rustdoc::private_intra_doc_links` shape, and a **nested** role home makes
      the temptation worse: the facade will want to link
      `workspace_section::scan_execution`. The fix is always to drop the link and
      keep the name in backticks, never to widen visibility. Command in
      `.agents/rules/build.md`.
- [x] 9.5 Add a `### WFR-WORKSPACE-TREE` subsection under `Migrated Workflow
      Roles` naming the facade with its measured size, every coordination module,
      the policy module, the evidence surface, the seam value objects, the
      called presentation surfaces **with the count task 5.4 actually produced**
      (nine at authoring, not the six 5a's summary implied), the **three**
      dissolutions with their destinations, the mutation-parity evidence pointer,
      and the role home it chose (**nested** — the first adopter). **Pointers in
      live
      `openspec/changes/migrate-workspace-tree-workflow-readability/evidence/<file>.md`
      form**; an archive-prefixed pointer fails the gate immediately, because the
      archive directory does not exist yet and rewriting the pointers is part of
      archiving.
- [x] 9.6 Update the row's `Current size`, **`Entry points`**, `Seams (i/c/a/p)`,
      `Seam value object`, `Evidence surface`, `Owned pure policy`, `Risk`, and
      `Status` cells from tasks 0.3 through 8, naming the pooled populations the
      old cells shared and the rows that share them, and **naming the unit on
      every size figure**. Three specific corrections the cell owes beyond the
      re-derivation:

      - the stale `policy.rs (190)` figure → its current size with the unit named
        (**134 production / 261 raw** at authoring);
      - "**All five** offending code facts" → the reconciled count from task 0.5;
      - the row's own `Current size` narrative, which must say that 5a's
        production figure was **correct** and that the growth is attributable
        file by file, so a later reader does not "re-correct" a correct cell.

      **`Entry points` is not optional**: three consecutive slots found omissions
      there, and this row's cell must account for the cross-cutting startup
      gate's `load_workspaces()`, the window's scope-consumer refresh
      (`ui/window/workspace_scope.rs:35 current_workspace_folder_paths` and
      `:40 refresh_workspace_scope_consumers`), row activation, the DnD drop, the
      context-menu routes into local history and notes, the refresh button, the
      close-time persistence flush (`ui/window/dialogs.rs:715`), watcher events,
      the workspace scope filter change (dropdown and window-driven), entering
      and leaving focused-folder mode, and **workspace folder add/remove**
      (`dialogs.rs:71`, `workspaces.rs:305`/`:397`,
      `workspace_section/mod.rs:315`) per task 0.4.
- [x] 9.7 Update `Seam Value Objects`: `WorkspaceWatchTicket` moves from
      `required` to `done`; `FileOperationTicket`/`Facts` is recorded as done
      (unplanned, landed with 5a); the scan-side ticket and the dissolved
      watch-target newtypes are recorded as **audited**. Update the
      `Workflow Stage Traces` entry so it names the real counts from task 0.4
      rather than the census floor of five, and say plainly that this is the
      **widest floor correction in the programme** with both numbers.
- [x] 9.8 Update the `Policy Module Census`: the two relocated workspace modules
      move out of the "Additional single-workflow modules" table with their
      outcomes and their parity numbers recorded, and `model/workspace.rs` is
      confirmed domain and staying with a re-derived owning-workflow count. Leave
      a pointer where a reader following the old snapshot would think a decision
      is still open. **The relocation-candidate count finally moves**: this is the
      first slot since 3a to relocate anything, and it relocates two.
- [x] 9.9 Advance `docs/next/workflow-readability.md`: flip slot 5b's ledger line
      to `complete` with `WFR-AUTOMATION-SPINE (partial)`, **correct its artifact
      cell, which currently says "the existing slot-5 change carries them" and is
      now false** — name this change; add `WFR-AUTOMATION-SPINE` to slot 6's
      outstanding line and remaining-scope row; record the change name in the
      slot/name table; update the status paragraph and the remaining-scope table;
      and add a **"Baseline after slot 5b"** table reporting workflows migrated,
      share of `ui/` + `model/` migrated with the corrected footprints, relocation
      candidates remaining, seams addressed and reified, long signatures
      shortened, automation projections, facade budget outcome, role names and
      homes used, and every convention change.
- [x] 9.10 Add a **"Convention friction slot 5b hit, recorded for slots 6 and 7"**
      section. Candidates already visible: the **first nested role home** and how
      it read; the **first evidence surface over a lazily materialized model and
      over a variable-sized child collection**, and whether the six code facts
      were the whole hazard; the first facade measured against an eleven-stage-order
      narration and which of delegate/escalate/split it needed; **the units error
      this change's own proposal committed** — comparing a production-only census
      against raw totals and concluding a correct cell was stale by ~900 lines,
      which is the strongest available argument that the re-derivation obligation
      must state its unit mechanically rather than rely on care, and which is
      worth recording precisely *because* it happened to the change that carries
      the corrective; **three dissolutions rather than the two 5a decided**, and
      the fact that the row's largest file (1,269 lines) was unclassified in a map
      whose own heading said "Every module classified" — a pattern recurring three
      times in one row is evidence the escalation-only spec text was wrong rather
      than merely incomplete; the **twelfth stage order** the inherited trace
      omitted and how a documented entry point can be missing from a trace that
      looked arithmetically consistent; whether any migrated
      row failed the dissolution re-check; whether the mutation floor and
      parity/gain statements found gaps in slots 3a, 4, or 5a; the `services/file_tree.rs`
      survivor triage outcome; the verdict on each data-safety candidate; and the
      retroactive-amendment cost now standing at **nine** rows plus the mutation
      evidence of three changes. Record the cost warning for slot 6 explicitly.
- [x] 9.11 **Update the action catalog's owner strings, which the module renames
      make stale.** `services/action_catalog/mod.rs` carries **15 rows** whose
      owner is the literal `"sidebar/workspace_section"` (`:1563`–`:1765`), and
      `scripts/check-automation-docs.py` parses that field at `:252` and renders
      it into `docs/automation-reference.md` at `:282`. After task 5.2 those
      strings name a directory whose modules have all been renamed, so decide
      whether the owner stays the **directory** (still accurate, and stable
      against future renames) or becomes the **owning role module** (more precise,
      and re-stales on the next move) — then apply it consistently across all 15
      rows and regenerate the docs. `make check-automation-docs` is what proves
      the two sides agree; a passing gate with a stale owner string is the
      failure mode, because the gate checks agreement rather than truth.
- [x] 9.12 **Repair the dangling matrix evidence pointers — TWELVE, not five** left in slot 5a's
      *live* form after 5a was archived: `docs/workflow-readability-matrix.md`
      `:94`, `:500`, `:2056`, `:2057`, and `:2500` still point at
      `openspec/changes/migrate-workspace-tree-and-notes-workflow-readability/...`,
      which no longer exists. The matrix's own rule at `:2189` says an archived
      change's pointers are **rewritten to archive form** so a human following the
      path finds the file; 5a's archiving missed these. Rewrite all five to
      `openspec/changes/archive/2026-08-27-migrate-workspace-tree-and-notes-workflow-readability/...`
      and confirm each resolves on disk. This is not scope creep: they are in a
      file this change rewrites anyway, three of the five are in **this row's own
      cell**, and the same rule governs this change's own pointers at archive
      time.
- [x] 9.13 Update `AGENTS.md`, `README.md`, `ui/sidebar/AGENTS.md`, and any
      `.agents/rules/*.md`, `.agents/skills/**`, or `docs/**` reference naming a
      moved path or a retired seam. `.agents/rules/ui.md`'s **File Tree** and
      **Multi-Workspace Sidebar** sections name several of these modules **by
      path**, `.agents/rules/widget-wiring.md` names the sidebar's live-run
      obligation, `docs/end-user-coverage.md` names
      `make test-workspace-row-states`, and `docs/accessibility-matrix.md` holds
      this row's accessibility rows. **Grep
      `scripts/accessibility_warning_allowlist.py` for every renamed module that
      logs**: it keys on module paths, so a rename silently turns an expected
      `tracing::error!` into an "unexpected warning". 5a verified its only key is
      `editor_page::load::execution`, so this stays a **conditional confirmation**
      — but re-read the file rather than trusting that. If a coupling exists,
      update it and re-verify it still **rejects** both an unrelated path and the
      stale module name so it has not become a blanket match.

## 10. Verification

- [x] 10.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries`, `check-automation-docs`,
      `check-accessibility-policy`, `check-visual-proof-policy`, and
      `check-filesystem-boundary` — the last because this row mutates the user's
      own files.
- [x] 10.2 **Build and test both feature configurations, not only
      `--all-features`.** `make check` runs Clippy with `--all-features`, which
      **hides** `unused_imports` that a default-feature build emits — the exact
      failure a change that gates seam storage behind `test-utils` produces. Build
      and test the default-feature configuration explicitly and record both.
- [x] 10.3 The rustdoc lint gate from task 9.4, clean. Recorded as its own line
      because it is CI-only and has already shipped broken once.
- [x] 10.4 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines** and **no retry relied upon**. A recovered flake is a blocker under
      `.agents/rules/preexisting-blockers.md`: classify the wait first
      (synchronous UI flip versus async `spawn_blocking_then` or realization),
      root-cause it, fix the cause, and **rerun in isolation** to separate a real
      break from load. This change adds work to
      `crates/lushtext/tests/widget/workspace_section.rs` (6,135 lines) and `window.rs` (~19,000 lines) — the two heaviest widget modules in
      the tree — so a load-amplified timeout there is the expected shape, and a
      load-amplified flake is still a real fragility. Record before/after project
      test counts in `evidence/test-counts.md`; the count must not decrease.
- [x] 10.5 `make test-workspace-row-states` clean — the focused idempotent
      workspace file-row state lane, which exists precisely for this row's
      surface.
- [x] 10.6 `make mutants-diff` clean, with the evidence files from tasks 3.5, 6.9,
      and 10.7 attached, every survivor accounted for, **relocation parity
      reported separately from gain-from-zero**, the field-deletion floor stated,
      and the exact invocation plus file-level anchors recorded. Confirm every
      `policy.rs` is reachable by `examine_globs` and imports no GTK-family crate.
- [x] 10.7 **Triage the 11 inherited surviving field-deletion mutants in
      `services/file_tree.rs`**, which slot 4 handed on and slot 5a re-handed as
      baseline rather than regression. Follow `.agents/rules/build.md`'s order and
      **do not skip to the last step**: decide whether each represents real missed
      behavior; then add or tighten deterministic tests; then consider a small
      refactor that makes the behavior testable; and only then an exclusion, which
      must be narrow enough that nearby behavior still mutates and must carry a
      project-specific rationale. The file already carries one narrowly scoped
      `exclude_re` entry for the `classify_entry` symlink match guard: that is the
      shape, and it must not be widened. Record each of the 11 with its verdict in
      `evidence/mutation-file-tree-survivors.md`, and remember the focused-run
      floor while triaging — a focused run of a handful of policy mutants also
      runs every field-deletion mutant in scope, so state the floor and do not
      attribute its pre-existing survivors to this change.
- [x] 10.8 The behavior-equivalence battery, each case asserting the
      **user-visible outcome** and the resulting tree, record, or persistence
      state, in `evidence/tree-behavior-equivalence.md`: a workspace with zero,
      one, and many folders; an empty workspace preserved as a real section; a
      deep tree with long paths and the no-horizontal-scrollbar contract; expand
      and collapse, and **a user collapse racing a deferred restore callback**
      which must not be resurrected; a directory scan superseded by a newer one
      and one whose section is gone when it resumes; a scan refused by admission
      and retried; a watcher install superseded during its worker in **both** its
      retire and its restart consequence; a watcher whose start fails terminally,
      settling readiness as unavailable rather than pending forever; a mailbox
      overflow promoting targeted paths to one full refresh; a targeted in-place
      refresh after create, rename, and delete including a directory rename
      matched by prefix against open tabs; a pending full refresh dominating
      queued targeted paths; folder-reorder DnD including an invalid drop position
      and a hover that must not expand a folder, materialize descendants, or
      restart a watch; `Space` peek including a stale request, a changed path, and
      **reached by keyboard focus on a realized row** rather than on the list
      view; a **double-click that opens a file row while a double-click on a
      directory row expands it** — the `GtkTreeExpander` internal-gesture contract
      `.agents/rules/ui.md` records as a three-iteration lesson; inline rename
      including empty, unchanged, duplicate, and the focus-out double-fire guard;
      **a recycled row that must retain no stale rename `GtkEntry` and no hidden
      label** — the `connect_bind` cleanup contract from task 0.6, driven by
      scrolling or refreshing a row out of and back into realization mid-rename,
      and covering the `pending_rename` one-shot so a new item's rename starts
      exactly once; **a targeted in-place refresh that must not rewalk the
      flattened model to rediscover expansion**, asserted through the expansion
      capture counters rather than by inspection, which is the no-rewalk clause
      task 0.5 records; create with a colliding name; delete confirmation and
      cancellation; workspace add, rename, and unlist; **workspace folder add and
      remove, including a duplicate folder path and a folder whose identity
      resolution fails**; a persistence write that fails and is
      retried, one superseded by a newer generation, **a load whose adoption is
      superseded by a live mutation (M-4, driven)**, and a close-time flush whose
      failure must abort close; a workspace scope filter change superseded before
      its settle timer fires with `filter_animation_active` settling exactly once;
      and entering and leaving focused-folder mode including the four DnD gates
      that read drilldown emptiness and a focused folder that disappears from disk
      while focused. Cover the sidebar **state extremes** the UI rules require, and
      assert the geometry contracts (header controls visible, only the item region
      scrolls, no horizontal scrollbar) where the case touches them.
- [x] 10.9 `make performance-smoke` clean. Record the Criterion comparison **only
      if the machine is quiet**; slot 4's comparison was uninterpretable under a
      saturated CPU, and an uninterpretable number is worse than a deferred one.
      Note that the scan-pressure benchmark references the relocated
      `model::workspace_scan` path (task 3.4), so a bench compile break here is a
      relocation defect, not a performance one.
- [x] 10.10 `make test-prop` if any property target is touched — it is gated
      behind `required-features = ["property-tests"]`, so no default lane runs it.
- [x] 10.11 The mandatory proof lanes for `ui/` and widget-test changes, each from
      a **clean artifact root**: `make visual-geometry-smoke`,
      `make accessibility-smoke`, `make visual-smoke`. Order these **after all
      source, documentation, and rules edits**: the accessibility policy gate
      fingerprints the *contents* of accessibility-relevant files, so any edit
      after a lane runs voids the proof and the lane must be rerun. The sidebar is
      accessibility- and geometry-dense — row names, descriptions, set positions,
      expanded state, context-menu keyboard parity, and the
      no-horizontal-scrollbar and header-visibility contracts — so consult
      `docs/accessibility-matrix.md` for the rows this change must cover and update
      them. A stale case directory from a previous run can make the visual
      geometry root summary report evidence the current binary did not produce; if
      a lane fails wholesale, suspect a stale shared `target/` artifact before
      suspecting the change.
- [x] 10.12 `make builder-diagnostics-smoke` and `make check-blueprint` if any
      `.blp`/`.ui` template or template child moved. The `imp.rs` files stay
      called presentation surfaces precisely so template children do not move;
      confirm that held.
- [~] 10.13 **Live and manual proof — DEFERRED FOR USER AVAILABILITY, planned that
      way from the start.** Slot 4 established that isolating an app's state does
      not isolate its window: a real Wayland launch maps a surface and takes focus
      regardless of `XDG_*` isolation, and it interrupted the user's session.
      **Do not start a live launch to discharge this item.** Record the exact
      remaining scope in `evidence/live-run.md` and mark this task `[~]`.

      **The acceptance gap this leaves must be accepted by the user, not granted
      by this change.** `.agents/rules/widget-wiring.md` states that widget-green
      plus a live warning is a *failed* fix for this subtree, so shipping without
      the walkthrough is an acknowledged gap in acceptance rather than a complete
      acceptance. Record it as awaiting the user's decision — either they run the
      walkthrough, or they accept the gap explicitly — and do not write "accepted"
      into the matrix, the programme record, or this file on the change's own
      authority. Scope to record:

      - `make run` against **restored workspaces** — expand and collapse a deep
        tree, drag to reorder folders, rename and delete a file, enter and leave
        focused-folder mode, toggle the sidebar while it animates, and resize —
        watching stderr for `Trying to measure GtkBox ...`, pixman
        `*** BUG *** ... Invalid rectangle`, `Gtk-CRITICAL`, `Gtk-WARNING`, and
        `GLib-GObject-WARNING`. `.agents/rules/widget-wiring.md` names the sidebar
        explicitly as the subtree needing a real `make run` cycle with restored
        workspaces, so **widget-green is necessary and not sufficient for this
        row**, and this deferral is the one acceptance gap the change ships with.

      Everything else display-dependent has a headless path: the smoke lanes run
      isolated `mutter --headless`, and `scripts/run-widget-tests.sh --headless`
      self-supervises into one. If a live drive is ever scheduled, use targeted
      AT-SPI rather than synthetic global input, which types into whatever the
      compositor focuses and is unverifiable.
- [x] 10.14 **Cold-read check**: with this change's conversation set aside, read
      `ui/sidebar/mod.rs` alone and answer, without opening a coordination module:
      "what happens when the user expands a folder", "what happens when a file
      changes on disk", "what happens when the user renames a file that has a
      note", "what happens when the user reorders workspace folders", and "what
      happens when the workspace scope filter changes". Slot 5a recorded that the
      first two **cannot** be answered from the pre-migration wrapper; that is the
      before-state this task measures against. If any answer needs a second file,
      the facade is not narrating.
- [x] 10.15 A tail simplify pass **after** full verification, recorded in A.15
      rather than run as speculative cleanup: duplicated inline `match` arms,
      tuple-returning seams that should return named values, and any
      `is_current`-shaped predicate whose real question is "may this completion
      act" (5a's `may_publish` lesson). Re-run the affected lanes after any edit,
      because the accessibility fingerprint voids on content change.
- [x] 10.16 `openspec validate migrate-workspace-tree-workflow-readability
      --strict` passing.

## 11. Handoff

- [x] 11.1 Confirm the programme record and the matrix agree:
      `WFR-WORKSPACE-TREE` is `migrated` with a complete `Migrated Workflow Roles`
      subsection naming real paths, slot 5b's ledger line is `complete` with its
      artifact cell corrected, `WFR-AUTOMATION-SPINE` is carried onto slot 6's
      outstanding line and remaining-scope row, and
      `make check-workflow-boundaries` passes. Report the count of pure
      mutation-scoped policy modules before and after (**10** before). Record in
      B.1.
- [x] 11.2 Hand slot 6 (`WFR-MINIMAP`) and slot 7 (the residual sweep) the facts
      they need, in B.2:

      - the named operations on this row's facade that other workflows should call
        rather than reach into, including whatever replaces the retired
        `workspace_filter_animation_active` read;
      - **how the nested role home read as its first adopter**, and whether the
        three dissolutions were the right call or should have been escalations, and
        whether the row's largest file being unclassified in the inherited map is
        a hazard to expect again;
      - the facade-budget outcome and, if it moved, what that costs slot 6 —
        which the matrix names as the slot most likely to test 370;
      - whether the no-materialization rule needed anything the six recorded code
        facts did not cover, and whether the child-collection half generalizes;
      - the corrected per-row census method, **with the unit stated on every
        figure and the units error this change committed named as the reason**,
        and every pooled population named with its sharing rows;
      - the two invisible seam classes this row surfaced, so slot 6 and 7 look for
        them: an **ungated `pub` `_for_benchmark` seam** that no gate-site grep
        finds and that breaks a bench when evidence narrows, and a
        **process-global counter presented as per-row evidence**;
      - the action-catalog **owner-string** decision (directory versus role
        module) and the fact that `check-automation-docs` proves agreement rather
        than truth, so a stale owner can pass;
      - that the five dangling matrix pointers from 5a's archiving were repaired
        here, and that **this change's own pointers become archive-form at its
        archive time** — the step 5a missed;
      - the `WFR-SHELL-LAYOUT` decisions honoured here that slot 7 owns: the
        sidebar animation blocker, `WorkspaceSidebarWidthPreset`'s new home, and
        the still-open `recent_documents.loading` ungated read from slot 3b;
      - the **six** production `.imp().` reach-throughs in `ui/automation.rs` this
        change deliberately left alone, with their owning rows;
      - the `services/file_tree.rs` survivor outcomes;
      - the data-safety verdicts, and the `sidecar_resolved` coverage gap
        boundary recorded in task 8.6 with `WFR-NOTES-BOOKMARKS` as owner;
      - the retroactive-amendment cost now standing at **nine** rows plus three
        changes' mutation evidence, and the not-a-confirmation streak's current
        length;
      - the reminder to run the **rustdoc gate** before shipping a facade, since
        it is CI-only.

      Confirm explicitly that slot 4's two `[~]` items, slot 4's three B.3
      simplify candidates, and slot 5a's `[~]` live and manual proof are **still
      theirs or still user-gated**, and were neither absorbed nor discharged here.
- [x] 11.3 Confirm the programme's remaining scope after this change: slots 6 and
      7 only. State plainly which rows remain unmigrated and that
      `WFR-AUTOMATION-SPINE` is still `pending` by design.

---

## Appendix A — orientation record

Fill each subsection as its tasks complete. Every pointer here and in the matrix
must be in live `openspec/changes/migrate-workspace-tree-workflow-readability/evidence/...`
form until the change is archived.

### A.1 Gate evidence (task 0.1)
Verified mechanically on a clean tree, not read from the proposal:

- `openspec/changes/archive/` contains slots 1, 2a, 2b, 3a, 3b, 4, and 5a
  (`2026-08-25-*` through `2026-08-27-*`).
- `openspec/specs/` holds all five required capabilities:
  `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
  `gtk-adapter-module-boundaries`, `mutation-testing`, `dbus-automation-spine`.
- `docs/workflow-readability-matrix.md` marks the **nine** rows migrated, and
  `WFR-WORKSPACE-TREE` was still `pending` at start (**and still is at end** — see the
  recorded deviation).
- `docs/next/workflow-readability.md` marked slot 5a complete and slot 5b outstanding.
- `make check-workflow-boundaries` passed, reporting **10** pure mutation-scoped policy
  modules — the same count at start and end, because this change's two relocations
  merged into the existing `ui/sidebar/policy.rs` rather than adding a new one.

`tier-3` two-proof rule satisfied nine rows over.


### A.2 Premise re-verification, row-scoped, with the direction of every correction (task 0.3)
Full working in [`census-reverification.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/census-reverification.md).

- **Size: 23 files, 11,741 production lines**, reproducing every one of the 23
  per-file figures exactly. Correction direction: **upward** to the matrix cell
  (+527 / +3 files vs 5a's 11,214 / 20). **Slot 5a's cell was correct** — four of its
  figures had zero production growth and grew only inside `#[cfg(test)]`, recorded
  affirmatively so a later slot does not "re-correct" a correct cell.
- **`policy.rs (190)` was genuinely stale in both units** (134 production / 261 raw at
  authoring). Corrected.
- **Seams: 60 functions / 111 gate sites — byte-exact** against the matrix cell,
  unchanged. Plus **two ungated `_for_benchmark` seams named for the first time**, both
  outside the census because neither is behind `test-utils`.
- **Override storage: still no module statics** — eight test-only fields on production
  state structs, re-confirmed at their current lines, plus a `thread_local!` counter and
  the destructive `take_touched_rows`.
- **Consumer counts by import**, with substring false positives named: persistence
  exactly 2 (no services/model/bench); scan 3 + **two bench references** that the move
  broke and this change re-pointed.


### A.3 Confirmations of slot 5a's decisions, and the two re-measurements (section 2)
Full working in [`shared-ownership-decisions.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/shared-ownership-decisions.md).

Role home **confirmed: option (2), nested** — this change is its first adopter, and it is
**fully** exercised: the canonical home holds the facade, the single `policy.rs`, the
single `evidence.rs`, `seams.rs`, and `test_policy.rs`, while the per-section coordination
roles are nested under `workspace_section/`. Module
classification confirmed with four deltas: `tree_loading.rs` (the row's **largest**
file) was never classified by 5a and becomes a **third** dissolution; folder membership
needs its own coordination module; `watch.rs` keeps its name; and the called
presentation surfaces are **nine**, not six.

`journal` verdict **confirmed** (`workspaces.json` is not a journal → `execution` with
latest-generation supersession), and applied once more to expansion state, which is not
persisted at all. Excluded scope and closed boundaries confirmed. Facade budget
**re-measured** — see A.11.


### A.4 Reconciled stage trace: stage orders, primitives, callback resumptions (task 0.4)
Full working in [`stage-traces.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/stage-traces.md).

**12 stage orders / 28 deferral primitives / 16 non-primitive resumptions = 44
resumption points**, subtotals summing exactly. Against the matrix's recorded **five
inversions** that is a **8.8x** floor correction — the widest in the programme (5a
computed 7.6x from 38).

**The twelfth candidate is its own stage order**: workspace folder add and remove. Two
entry points, the membership family's only off-GTK stage and only self-restarting
retry, and a two-sided terminal. Consequence: **`list_execution.rs` must not own folder
membership** — it needs its own `execution` module.

Slot 5a's attribution was low in **six** places, all in the direction of more,
including a `spawn_blocking_then` 5a itself landed but never counted. Both of 5a's
named reconciliation moves are **confirmed**.


### A.5 Contracts as implemented today, verbatim (task 0.6)
Full working in [`durability-contracts.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/durability-contracts.md).

All ten contracts recorded verbatim with file:line ranges as a before-any-move
snapshot. **None of the protected blocks was moved by this change**, and all ten
anchor files were verified untouched by `git diff` — see
[`tree-behavior-equivalence.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/tree-behavior-equivalence.md).

Three findings from the pass, all corrected here:

1. **`.agents/rules/ui.md` named a phantom symbol.** `restore_materialized_state` has
   never existed in the codebase; the rule, slot 5a's snapshot, and this change's own
   task list had all been citing it. The one real implementing site is
   `schedule_child_state_restore`, and it **does** satisfy apply-time reads. Rule fixed,
   and deliberately no longer names a file, because migrations rename the owner.
2. **The second rename-entry cleanup loop is in `connect_unbind`**, not `connect_bind`.
   Task premises corrected.
3. **Its six extra resets are intentional** — the unbind-side reset of state
   `connect_bind` sets affirmatively — so a move must preserve the asymmetry rather than
   "fix" it. Recorded in the task list.


### A.6 Amendment basis and the nine-row retroactive re-check (tasks 1.1, 1.3)
Basis in [`amendment-basis.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/amendment-basis.md); re-check in
[`retroactive-recheck.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/retroactive-recheck.md) and in the matrix's new
`### Slot 5b amendment re-check` subsection.

All four bases confirmed from the live artifacts, and both deltas verified **pure
additions** (0 removed non-blank lines across all three requirements). The mutation
floor was measured from the tool rather than recalled: **34 field-deletion mutants**,
12 of them in `services/file_tree.rs`.

**Fourth consecutive not-a-confirmation: eight gaps.** G1 (`WFR-DRAFT-RECOVERY` has an
**undeclared** `retirement.rs` role module) and G8 (**twelve** dangling evidence
pointers, not the five the task list expected) are both **invisible to every gate**.
G7 is a **false parity claim in the live matrix**, found while writing the amendment
that forbids exactly that conflation. G1, G7, and all twelve pointers are fixed in the
live matrix; G2 and G3 are recorded with G3 deliberately **not** renamed, because
renaming a stable correct module is the churn the same amendment forbids.


### A.7 Widget-test and production reach-through, in scope and out (tasks 0.7, 6.6, 6.7)
Full working in [`widget-test-reach-through-migration.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/widget-test-reach-through-migration.md).

**The inherited figures were wrong, in the direction of more work.** Re-derived: **929**
total `.imp().<field>` sites across the five files, **295** row-owned, and **79**
in-scope runtime reads/writes — **not the inherited 45**. TemplateChild handles are
**179**, not 113, plus a separate **37-site** `RefCell<Option<Widget>>` bucket the
archive had wrongly pooled into "TemplateChild". Three separable causes are documented,
the main one being a same-line-only grep that missed rustfmt-wrapped multi-line chains.

**13 writes** found, all in `workspace_section.rs`, each with the existing configuration
seam that should drive it instead named.

**Neither 6.6 nor 6.7 landed**, because both depend on the evidence surface. No widget
test was modified, so the lane's risk profile is unchanged from `origin/main`.
`workspace_filter_animation_active` has **zero** widget-test sites — its only two
readers are the production reach-throughs at `ui/automation.rs:766` and `:927`, both
confirmed exact and both still open.


### A.8 Coordination role mapping per stage order, and the `journal` verdict (tasks 2.2, 2.3, 5.1, 5.2)
The mapping is **decided and recorded** but **not implemented** — no coordination module
was created. Recorded in
[`shared-ownership-decisions.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/shared-ownership-decisions.md) §2.2, with two
corrections to the inherited map that the next slot must implement rather than
rediscover:

1. **Folder membership needs its own `execution` module.** Task 0.4 established that
   workspace folder add/remove is its **own** stage order, so `list_execution.rs` must
   not own it. Both routes — the header dialog (`dialogs.rs:71`) and the row request
   (`workspace_section/mod.rs:315`) — need that named destination.
2. **`tree_loading.rs` must be classified.** 5a's map gave `scan_admission.rs` and
   `scan_execution.rs` no named source and never classified the row's largest file at
   all, under a heading reading "Every module classified".

`journal` verdict **confirmed, not re-litigated**: `workspaces.json` is not a journal —
no generation in the file, no stale-record cleanup, a failed write leaves the previous
file intact, and the read-back is an ordinary next-launch load. So `execution` with
latest-generation supersession, named `persist_execution.rs`. Applied once more to
expansion state, which turns out not to be persisted at all, so the question does not
arise for it.


### A.9 Data-safety passes and the candidate verdicts (tasks 0.8, 8.1–8.7)
Full working in [`data-safety.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/data-safety.md), which is the most consequential
artifact of this change.

**Pass 1 found seven confirmed findings**, not one — exactly what the programme record
said a tier-3 slot must budget for.

**Fixed here (2):** the tree-file confirmed delete that removed **by path with no
identity recheck** (HIGH; recursive for directories; the same file's own contract says
"never delete by path alone"), fixed by extracting the decision as **pure policy** so it
carries mutation coverage; and the **one-line draft re-stamp** in `update_tab_path` for
a rename that silently stranded the user's unsaved edits on the crash path (HIGH;
`Skip(Unavailable)` is the one arm with neither notification nor restore).

**Handed on with named owning rows and durable homes (5):** the ledger's pre-rename
crash window (`WFR-NOTES-BOOKMARKS`); the file monitor never re-armed after rename,
which silently overwrites external edits (`WFR-DOCUMENT-LOAD`); the sidecar migration's
unguarded read-merge-write (`WFR-NOTES-BOOKMARKS`); the delete teardown that runs before
a cancellable close confirmation (`WFR-SHELL-LAYOUT`); and — **a tree file, handed on
with its reason stated** — close proceeding over an in-flight pre-persist workspace
mutation, whose fix changes a persistence invariant in the same step as relocating its
owner.

**Clean and proven, not merely unflagged:** rename acquires `TargetWriteGuard` on every
path in resolved-key order and refuses the self-deadlocking symlink case; the
destination-collision refusal is `RENAME_NOREPLACE`, one syscall, not a TOCTOU
`exists()`; prefix matching is correct in both consumers; section teardown cannot panic
or write stale state; and the close-flush contract holds on all four questions,
including that a **flush failure aborts close**.

**Pass 2 (task 8.7) ran**, over `git diff origin/main`, and found **six** findings —
every one introduced by this change or by a fix inside it, the same shape slot 5a's pass 2
found. The **fix cycle** then found five more, two of them data-safety, including that the
pass-2 CRITICAL fix was **inert**: the bit its pure policy read was set by every rebuild
rather than by a load adoption. **Seven confirmed data-safety defects fixed in total** —
two from pass 1, three from pass 2, two from the fix cycle — with the counting rule stated
once at the end of `evidence/data-safety.md`.


### A.10 Automation no-widening proof and the honoured ownership decisions (tasks 7.5, 7.6)
Pre-change surface captured exactly in
[`automation-no-widening.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/automation-no-widening.md): the ten `window.workspace`
fields with their types and documented meanings, the three readiness blockers with their
current sources, the `workspace-refresh-complete` predicate, and the six blocker lists —
**including the asymmetry at `model/automation.rs:342-343`, where one list omits
`workspace-filter-animation`**. That asymmetry is part of the contract and a refactor
that "tidied" it would be a widening.

**The projection landed**: all ten `window.workspace` fields now come from
`ui/sidebar/evidence.rs`, and both production reach-throughs are retired. The object is
registered in `scripts/check-automation-docs.py`'s `EVIDENCE_PROJECTIONS`, so the ten
documented rows are **gated** rather than merely written — proved by injecting a rename on
each side and observing findings both ways, then reverting.

**The contract is unwidened**: no schema field added, removed, renamed, or retyped;
`make check-automation-docs --self-test` passes; and the live D-Bus capture of
`window.workspace` under `make automation-smoke` matches the documented contract field for
field. One value semantics change is deliberate and recorded: `scope_workspace_name` now
reports `null` rather than `""` when the scoped workspace is absent from the file, which
makes the documented "if any" honest beside a non-null `scope_workspace_id`.

**Ownership decisions honoured, not re-decided**: `workspace-sidebar-animation` is
`WFR-SHELL-LAYOUT`'s because the blocker follows the animation rather than the row name;
the palette's `command-palette-index` disjunct stays a direct call; and the two recorded
**absences** are the status quo, not gaps to fill.


### A.11 Facade re-projection and final measurement (tasks 2.1, 9.2)
Full working in [`facade-measurements.md`](openspec/changes/migrate-workspace-tree-workflow-readability/evidence/facade-measurements.md).

**The three recorded subtrahend ranges were stale by exactly +9 each while every one of
their sizes was unchanged** — so 5a's ranges had drifted and 5a's arithmetic was right.
The fourth subtrahend had no recorded range and was measured here for the first time:
the `:102-178` inspection block, 77 lines reducing to ~24.

Re-projection: **≈349 of 370** independently, versus **≈358–360** for 5a's figure
re-based onto the now-confirmed twelfth stage order. Both fit; the conservative one was
planned against. **Escalation path step 1 (delegate harder) suffices — no in-change
escalation, no census-row split.**

**Measured after this change: the facade is 292 of 370, down from 415**, with **78** lines
of headroom. `WorkspaceSidebarWidthPreset` leaving (103 lines) was the largest single
subtrahend; the narration then landed at ~70 lines rather than the ~147 projected, and the
four duplicated visible-section walks were **delegated** to one named
`with_first_visible_section` operation rather than merely compressed. The projection
(≈349, conservatively ≈358–360) was beaten, not merely met. An interim revision of this
section recorded 316, measured before the last delegation and before the narration.

**Other facades protected**: all five re-measured, all reproducing their recorded
figures, none pushed over — `ui/search_panel/mod.rs` still sits at exactly 369 with
**not one line added**.


### A.12 The three dissolutions, part by part, with destinations (task 5.3)
**Performed**, each part given a destination:

- **`tree_loading.rs` (1,269 raw)** → the process-global scan permit, limit, high-water,
  and admission retry to `scan_admission.rs`; the child scan worker, child-store
  identity/mirror/splice, batched reconciliation, directory-state clearing, and the
  deferred expansion restore to `scan_execution.rs`; the drag-hover empty child model and
  its two seams to `reorder_execution.rs`. `build_children_model` stays in
  `scan_execution.rs` and is named there as the materialization entry point the evidence
  surface must never reach.
- **`tree_index.rs` (969 raw)** → `scan_execution.rs` **entirely**, which corrects the
  recorded "pure index arithmetic → `policy.rs`" half: re-reading the file found no pure
  arithmetic left, because the splice planning and common prefix/suffix logic already live
  in `services::file_tree`.
- **`watch_targets.rs` (337 raw)** → the two generation newtypes to `ui/sidebar/seams.rs`.
  That move also **closed an encapsulation gap** — the mirror bookkeeping had been
  advancing the target generation by writing its tuple field directly, which became a
  privacy error and is now a `next()` method. It **keeps** its mirror arithmetic and its
  snapshot, so it is **partially** dissolved and is recorded as such rather than as done;
  the fix cycle additionally classified what it *is* (a plain data structure owned by the
  `watch` role, neither a role module nor a presentation surface).

The pre-convention `ui/sidebar/workspaces.rs` (864) also dissolved **entirely**, into the
four canonical-home `execution` roles.


### A.13 Lane consequences of the module renames (tasks 9.11, 9.13, 10.11)
**Six modules were renamed** (`actions`→`file_execution`, `folders`→`folder_execution`,
`peek`→`peek_execution`, `refresh`→`refresh_execution`, `dnd`→`reorder_execution`,
`tree_loading`→`scan_execution`), so both coupling hazards this section exists for were
live, and both were checked rather than assumed:

- **The 15 action-catalog owner strings** (`services/action_catalog/mod.rs`, the literal
  `"sidebar/workspace_section"`) are **still accurate**, because the owner is recorded as
  the **directory**, which did not move. Task 9.11's decision (owner = directory vs owning
  role module) therefore resolves to "directory, unchanged", and no string needed editing.
  Note for whoever revisits it: `make check-automation-docs` proves the two sides
  **agree**, not that either is true, so a stale owner string would pass the gate.
- **`scripts/accessibility_warning_allowlist.py`** re-read directly rather than trusted:
  its only key is still `editor_page::load::execution`. **No coupling exists**, so the
  conditional confirmation resolves to "nothing to update". The one new
  `tracing::warn!` this change adds (the delete safety refusal) is in a module the
  allowlist does not key on, and it fires only in a race that no smoke lane drives.

Docs re-pointed for the two deleted `model/` modules and the new `width_preset.rs`:
`AGENTS.md`'s module tree and its workspace auto-refresh paragraph, `README.md`'s crate
layout, and `crates/lushtext-core/src/ui/sidebar/AGENTS.md`'s Responsibilities and its
scan-flight Local Contract. `docs/end-user-coverage.md` needed **no** change, because no smoke expectation moved.
`docs/automation-reference.md` gained the ten `window.workspace` Evidence Projection Map
rows, and `docs/accessibility-matrix.md`'s direct-call baseline was re-audited in the fix
cycle: its `workspace_section/folders.rs` row was a **dangling** pointer at a module this
change renamed, and re-running the documented audit command found the whole baseline
already empty. `docs/automation.md` needed no change, because no automation field moved.


### A.14 Cold-read result, five questions (task 10.14)
**Run against the landed facade.** The two questions slot 5a's before-state could not
answer from the pre-migration wrapper — "what happens when the user expands a folder" and
"what happens when a file changes on disk" — are now answerable from
`ui/sidebar/mod.rs`'s narration plus one hop into the named role it delegates to
(`workspace_section/folder_execution.rs` and `scan_admission.rs`/`scan_execution.rs` for
the first, `watch.rs` and `refresh_execution.rs` for the second). The facade names the
inversions rather than hiding them, which is the property the cold read is measuring.


### A.15 Tail simplify pass, after full verification (task 10.15)
**Run after full verification**, over the landed structural work. Its findings are the
four recorded in `evidence/tree-behavior-equivalence.md`; the two most characteristic are
of exactly the kind this task looks for:

- **a tuple-returning seam replaced by a named value**: the watch install's loose
  `(section_weak, generation, lifetime)` tuple became `WorkspaceWatchTicket`;
- **an `is_current`-shaped predicate whose real question was "may this completion act"**:
  the two sequential generation comparisons became one `disposition()` returning
  `Install` / `Retire` / `Restart` — 5a's `may_publish` lesson applied.


## Appendix B — handoff

### B.1 Programme and matrix agreement (task 11.1)
**They agree, and what they agree on is that the row is migrated.**

- `docs/workflow-readability-matrix.md`: `WFR-WORKSPACE-TREE` is **`migrated`**, with its
  `Current size`, `Owned pure policy`, `Seams`, `Seam value object`, `Evidence surface`,
  `Slot`, and `Status` cells re-derived in the fix cycle, its per-row notes naming the
  role home, the dissolutions, the two modules that are neither a role nor a presentation
  surface, and the four outstanding follow-ups; and the `Policy Module Census` rows for
  both relocations marked done with their parity numbers.
- `docs/next/workflow-readability.md`: slot 5b's ledger line reads **complete**, naming
  this change in its artifact cell, with the "Baseline after slot 5b" table describing
  what actually landed. The status paragraph counts **ten** migrated workflows.
- `make check-workflow-boundaries` **passes**, and its cross-check that the programme
  record's slot ledger agrees with the matrix is what makes the two consistent by
  construction rather than by inspection.

**Pure mutation-scoped policy modules: 10 before, 10 after.** The count is deliberately
unchanged: both relocations merged into the workflow's existing `ui/sidebar/policy.rs`
rather than creating a new module, because the convention allows exactly one `policy.rs`
per workflow and `ui/**/policy.rs` is the only path the mutation scope reaches.


### B.2 To slots 6 and 7 (task 11.2)
**First, the honest headline: `WFR-WORKSPACE-TREE` is migrated.** This change landed the
amendments, both relocations, the seam, the facade, the nested coordination roles, the
three dissolutions, `evidence.rs`, the seam retirement, the automation projection and its
drift-gate registration, seven data-safety fixes, and the whole decision record. What it
leaves is named in the "Remaining follow-up" list at the head of this file, not hidden.

**Corrections to inherited figures — all in the direction of more work.** Do not trust a
handed-on number without re-deriving it; four separate ones were wrong here:

| Inherited | Actual | Cause |
| --- | --- | --- |
| 11 stage orders, 38 resumption points | **12 / 44** | six missed attributions, one of them a `spawn_blocking_then` slot 5a itself landed |
| "five" materialization facts | **six** | the source's own prose contradicted its own table |
| 45 in-scope widget reach-throughs | **79** (of 929 total; 179 TemplateChild, not 113) | a same-line-only grep missed rustfmt-wrapped multi-line chains |
| 11 `file_tree.rs` field-deletion survivors | **12 generated**, 11 surviving | generated and surviving are different quantities |
| 5 dangling matrix pointers | **12**, across four archived changes | nobody had listed the directories |

**Two seam classes that no grep finds** — look for both:

1. an **ungated `pub` `_for_benchmark` seam** that no `test-utils` gate-site grep finds
   and that breaks the bench target when evidence visibility narrows. Two exist; one is
   the **service's**, not the tree row's. Both are still undisposed.
2. a **process-global counter presented as per-row evidence**:
   `ACTIVE_WORKSPACE_SCAN_TASKS` / `WORKSPACE_SCAN_TASK_HIGH_WATER` guard a
   process-wide limit of 4 across **all** sections. The surface must either account
   per-section or **name the scope honestly in the field's own name and doc** — a
   window with zero workspaces would otherwise report scans belonging elsewhere.

**Mutation lessons, both of which cost real time here:**

- **`make mutants-diff` silently proves nothing on an uncommitted worktree.** It builds
  its diff with a three-dot **commit range**, so working-tree edits are invisible; it
  exits **0** having tested zero mutants. `git add -N` does **not** fix it. Generate the
  diff yourself and pass it as an argument.
- **Do not edit any file in the mutation scope while a run is in flight.** A mid-run
  `cargo fmt` shifted line numbers and produced a false MISSED that took a hand-check to
  disprove — and the hand-check itself nearly went wrong, because reproducing an
  operator mutation requires letting Rust's precedence apply (`a || b && c` is
  `a || (b && c)`, **not** `(a || b) && c`).
- **`--re` does not bound a run; `--in-diff` does.** The unfilterable floor is **34**
  field-deletion mutants, measured from the tool.

**Named operations to call rather than reach into**, unchanged and still calls:
`migrate_note_sidecars_after_rename`, `show_local_history_for_path`,
`resolve_notes_for_editor`, `notes_evidence()`. The tree row's own
`workspace_filter_animation_active` is **still reached through `.imp()`** at
`ui/automation.rs:766` and `:927`; retiring it still needs this row's evidence surface.

**The six production `.imp()` reach-throughs deliberately left alone**, with owners:
`:518`/`:519` (`window.imp().tab_view`, `WFR-SHELL-LAYOUT`, slot 7) and
`:1144`/`:1151`/`:1169`/`:1231` (editor/minimap, `WFR-MINIMAP`, slot 6). Re-derived here;
5a's handoff recorded pre-fix numbers.

**`WFR-SHELL-LAYOUT` decisions honoured, for slot 7:** `workspace-sidebar-animation` is
that row's because the blocker follows the animation, not the row name;
`WorkspaceSidebarWidthPreset` now lives at `ui/sidebar/width_preset.rs` with its
ownership stated in its own module doc; and slot 3b's `recent_documents.loading` ungated
read is **still open**.

**Data safety — five confirmed findings are waiting, each with an owner**, in
`evidence/data-safety.md`. Two are `WFR-NOTES-BOOKMARKS`'s, one `WFR-DOCUMENT-LOAD`'s,
one `WFR-SHELL-LAYOUT`'s, and **one is the tree row's own** (close proceeding over an
in-flight pre-persist workspace mutation) deliberately deferred to land immediately
after the structural move, when the relocated module makes its driven race test cheap.
The `sidecar_resolved` coverage-gap boundary remains `WFR-NOTES-BOOKMARKS`'s, and this
change confirmed the tree-side rename path does **not** become a fourth driver.

**Budget notes:** the **one** new seam task 6.4 budgeted (the load-worker delay for
M-4's driven race test) is **unspent** — the next slot may still add exactly one, and
must justify it individually at its definition. The facade's narration budget **improved
to ~248 lines** because the 103-line width preset already left.

**Retroactive-amendment cost now stands at nine rows plus three changes' mutation
evidence, and the not-a-confirmation streak is at four.** Two of this re-check's eight
findings were invisible to every gate and required reading the code and the filesystem.
Budget the re-check as real work, not a formality — and note that **this change's own
evidence pointers must be rewritten to archive form at archive time**, the step four
prior changes missed.

**New lessons from the structural migration itself:**

- **A dissolution plan derived from a module's own doc must be re-derived from its
  imports before it is executed.** `tree_index.rs` was recorded as splitting between
  `policy.rs` (pure arithmetic) and `scan_execution.rs` (cache). Its doc said
  "path/index bookkeeping", which is accurate — but bookkeeping over `gio::ListStore`
  is not pure arithmetic, and the file held **none**. One destination, not two.
- **Extracting a decision *into* the mutation scope does not bring its coverage with
  it.** `workspace_scope_kind_name` arrived from the automation adapter with **2
  survivors**, because its only assertions lived in a *widget* test — outside the
  mutation lane's test surface. Budget a unit test for every extraction.
- **A guard that cannot fire is worse than no guard.** This surface's first draft had a
  `disposed_sections_skipped` field behind a `try_get()` predicate; the section's
  `dispose()` never calls `dispose_template()`, so it could never fire. Removed, and the
  surface is disposal-safe for the stronger reason that it reads **no** `TemplateChild`.
  Check that a defensive path is *reachable* before claiming it handles a hazard.
- **Prefer a real production drive over a new actuation seam.** The four evidence proofs
  use `expand_folders()`, a dropdown `set_selected`, a rename, and an unlist. Zero new
  seams; the budgeted one is still unspent.
- **Action-catalog owners: keep them at the directory.** Nine modules inside
  `sidebar/workspace_section/` were renamed and **zero** of the 15 owner strings went
  stale. The directory choice is stable against exactly this kind of change.
- **`super::` path renames dominate a rename diff and hide real edits.** Verify a
  preservation anchor's diff explicitly — `row_factory.rs`'s entire production diff is
  five module paths, which is what let the `GtkTreeExpander` block be certified
  untouched.

**Run the rustdoc gate by hand before shipping a facade.** It is CI-only, and a nested
role home makes `private_intra_doc_links` more tempting, not less.

**Still theirs, neither absorbed nor discharged here:** slot 4's two `[~]` items and its
three B.3 simplify candidates (verified by path to be outside this row), and slot 5a's
`[~]` live and manual proof. This change adds its own `[~]` live walkthrough, recorded in
`evidence/live-run.md` as **awaiting the user's decision**.

