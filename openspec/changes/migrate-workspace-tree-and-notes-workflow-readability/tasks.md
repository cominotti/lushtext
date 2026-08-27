> ## Deviation recorded during implementation: slot 5 split into 5a and 5b
>
> **What happened.** Task 0.6's mandatory `data-safety` pass, run before any code,
> found **eleven** findings rather than the one-per-slot the proposal budgeted for
> — including a normal-usage **data-destruction** bug: inline rename validated
> only empty-or-unchanged, and the platform rename silently replaces a regular
> destination, so renaming a file to an existing sibling's name destroyed that
> file's contents with no prompt, no warning, and no undo.
> `.agents/rules/preexisting-blockers.md` has no exceptions, so **seven fixes
> landed first**, each with a regression test proved to fail without its fix. Two
> of those tests initially passed against the broken code as well, which cost two
> counted configuration seams to make the race reachable.
>
> **What was completed.** `WFR-NOTES-BOOKMARKS` is fully migrated. Both spec
> amendments landed with the eight-row retroactive re-check paid. All of section 0,
> 1, 2, 3, 7, and 8 is done, plus the tree row's `policy.rs`, `seams.rs`, and
> `test_policy.rs` — which exist because reifying the seam and extracting the
> rename policy **were** the fixes for two of the confirmed defects.
>
> **What moved to slot 5b.** `WFR-WORKSPACE-TREE`'s structural migration:
> sections 4, 5, and the tree half of 6, 8, 9, and 10. Those tasks are marked
> `[~]` rather than `[x]`. The choice was between a **partially** migrated tier-3
> row — facade rewritten, 58 seam functions half-retired, a matrix row claiming
> roles that do not all exist — and an honestly unmigrated one, in the workflow
> that renames and deletes the user's documents. The convention's Completion Rule
> answers that, so the row stays `pending`.
>
> The rationale, and what 5b inherits as decisions rather than questions, is in
> `docs/next/workflow-readability.md`, "Why slot 5 split into 5a and 5b".

## 0. Gates, orientation, and premise re-verification

- [x] 0.1 **Slot-4 gate — blocking.** This change may not begin until slot 4 is
      complete in all four rows. Verify mechanically on a clean tree rather than
      reading it from the proposal: `openspec/changes/archive/` contains the slot
      1, 2a, 2b, 3a, 3b, and 4 changes; `openspec/specs/` holds
      `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
      `gtk-adapter-module-boundaries`, `mutation-testing`, and
      `dbus-automation-spine`; `docs/workflow-readability-matrix.md` marks
      `WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`,
      `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`,
      `WFR-LOCAL-HISTORY`, and `WFR-DRAFT-RECOVERY` `migrated` with complete
      `Migrated Workflow Roles` subsections naming paths that exist; the slot
      ledger in `docs/next/workflow-readability.md` marks slots 1 through 4
      complete and slot 5 outstanding; and `make check-workflow-boundaries`
      passes. Both of this slot's rows are `tier-3`, so the two-proof rule applies
      to each. Record in A.1.
- [x] 0.2 Read `docs/next/workflow-readability.md` end to end, including all five
      "Convention friction slot N hit" sections, and
      `docs/workflow-readability-matrix.md`'s Settled Conventions, Facade size
      budget, Evidence-surface reentrancy, Cross-cutting eligibility, Evidence
      pointer form, Completion Rule, and all four amendment re-check subsections.
      Then read slot 4's archived `tasks.md` Appendix B — **this change is its
      named recipient** — and slot 2a's friction section for the
      `NoteSourceRefreshCoordinator` deferral. Note the five capability specs this
      change consumes and the two it amends.
- [x] 0.3 **Premise re-verification — before any code, once per row.** Slot 4's
      amendment made this a stated obligation, and slot 4's own re-check found two
      of four migrated rows non-compliant, so treat it as real work. For **each**
      of `WFR-NOTES-BOOKMARKS` and `WFR-WORKSPACE-TREE`, produce a row-scoped
      figure and name what the census cell had pooled:

      - **Size**: production lines only, excluding `#[cfg(test)]` modules —
        including a co-located test module that lives in **its own file** behind
        `#[cfg(test)] mod tests;`, which a naive per-file scan counts as
        production. Do not count shared services, cross-cutting modules, or
        neighbour files the workflow only calls. Authoring measured
        `ui/sidebar/**` at **11,364 production lines across 21 files** (`mod.rs`
        406, `imp.rs` 246, `callbacks.rs` 219, `dialogs.rs` 205,
        `workspaces.rs` 843, `file_tree_item.rs` 150, and `workspace_section/`
        `tree_loading.rs` 1,269, `tree_index.rs` 844, `folders.rs` 835,
        `context_menus.rs` 809, `dnd.rs` 769, `mod.rs` 744, `peek.rs` 728,
        `refresh.rs` 666, `watch.rs` 583, `actions.rs` 534, `imp.rs` 508,
        `row_factory.rs` 463, `watch_targets.rs` 264, `row_accessibility.rs` 199,
        `icon_presentation.rs` 80) and `ui/window/notes/**` at **4,365 total**
        (`browser.rs` 1,749, `editors.rs` 929, `mod.rs` 892, `bookmarks.rs` 795,
        none carrying a `#[cfg(test)]` module) plus `startup_data.rs` 435. Treat
        every one as an **upper bound to be corrected**, and resolve two specific
        questions: whether `ui/sidebar/file_tree_item.rs` (150) counts at all —
        the matrix lists it in "Surfaces With No Coordination Tier", so the
        default answer is **no** — and whether `startup_data.rs` counts, which is
        task 2.2's decision.
      - **Seams**, per kind, with the gate-site count and the unit stated.
        Authoring counted **58 `*_for_test` functions across 106
        `#[cfg(feature = "test-utils")]` sites** in `ui/sidebar/**`
        (`workspace_section/mod.rs` 15/23, `watch.rs` 15/24, `refresh.rs` 9/10,
        `dialogs.rs` 6/6, `dnd.rs` 6/12, `tree_loading.rs` 4/10,
        `tree_index.rs` 3/3, plus gate-only sites in `workspace_section/imp.rs`
        8, `watch_targets.rs` 7, `folders.rs` 3) and a first classification of
        **22 inspection / 8 configuration / 19 actuation / 9 probe**. This is the
        largest seam population in the programme — slot 4's largest single row
        held 28 functions across 55 sites. For notes, authoring counted **7
        functions across 15 sites** in `ui/window/notes/**` (`mod.rs` 1/7,
        `browser.rs` 3/3, `bookmarks.rs` 2/4, `editors.rs` 1/1), **zero** in
        `startup_data.rs`, plus service-side seams in `services/palette/notes.rs`
        (4/6), `services/workspace_manager.rs` (3/9),
        `services/workspace_watch.rs` (4/4), and
        `services/format_upgrade/**`. Re-derive and re-classify; the census
        tuples (`24/7/29/5 = 65 fns, 116 sites` and `2/4/4/0 = 10 fns, 16 sites,
        2 override statics`) predate the four-kind classification and are not
        row-scoped.
      - **Test-only override statics and fields**, which the census counts
        separately. Authoring found them **inside imp state structs** rather than
        as module statics on the tree side — `RefreshRuntimeState`'s
        `test_reconcile_batch_delay` / `test_scan_delay` / `test_empty_probe_reads`
        and `WatchRuntimeState`'s `test_start_delay` / `test_drop_delay` /
        `test_worker_starts` / `test_last_poll_notices` / `test_disabled`
        (`workspace_section/imp.rs`), plus a `tree_loading.rs` thread-local
        counter and `watch_targets.rs`'s `touched_rows`. On the notes side they
        are module statics (`NOTES_BROWSER_SOURCE_ENTRY_LIMIT_FOR_TEST`,
        `BOOKMARK_EXCERPT_PREVIEW_DELAY_MS`, `NOTES_BROWSER_QUERY_DELAY_MS`,
        `NOTE_SOURCE_DELAY_MS`, and the `format_upgrade` legacy registry
        override). **A test-only field on a production state struct is a
        configuration seam that no `static` grep finds** — record it as one.
      - **Pure policy consumer counts** for `model/workspace_scan.rs`,
        `model/workspace_persistence.rs`, `model/workspace.rs`, `model/note.rs`,
        `model/bookmark.rs`, `model/sidecar_identity.rs`, `model/folder_note.rs`,
        and `model/document_note.rs`, counted as **owning workflows** rather than
        referencing files, with substring false positives named. Slot 3b lost
        time to six `file_load` substring hits; expect the same from `note`,
        `bookmark`, and `workspace`, which appear in callback names, field names,
        and test function names throughout `ui/`.
      - **The shared population each corrected cell had pooled**, named with the
        rows that share it. Slot 4 already named one from the other side:
        `services/palette/notes.rs` is **2,163 production lines of a 3,428-line
        file** shared with migrated `WFR-COMMAND-PALETTE`, and
        `services/palette/tests.rs` (1,223) is that module's separate-file test
        module. Authoring's first read of that split found roughly **130
        production lines browser-only, ~300 palette-only, and ~1,700 genuinely
        shared** — verify and record it, because this is the figure slot 6 or 7
        would otherwise re-derive.

      Write both rows to
      `openspec/changes/migrate-workspace-tree-and-notes-workflow-readability/evidence/census-reverification.md`
      and summarise in A.2. Correct the matrix cells in task 9.
- [x] 0.4 **Read the code before changing it, and expect the inversion counts to
      be badly wrong.** For each workflow, write the current ordered stages and
      **every** control-flow inversion from the code, with the resumption point,
      into `evidence/stage-traces.md`. Five consecutive slots found the census
      counts to be floors; here the gap is the widest yet. The matrix records
      **five** inversions for `WFR-WORKSPACE-TREE` and **four** for
      `WFR-NOTES-BOOKMARKS`. Authoring's first read of the code found, as a
      lower bound:

      - **`WFR-WORKSPACE-TREE`: eleven stage orders and at least 27 primitive
        deferral sites** — 11 `spawn_blocking_then`, 8 `idle_add_local_once`, 4
        `timeout_add_local(_once)`, 3 `Debounce`, 1 `SupersedingTimer` — plus
        dialog-response, `FileDialog`, DnD-lifetime, and selection callback
        resumptions, which are **not** in that 27. Per stage order: workspace-list
        load 2, directory scan/expand 7, watch install plus mailbox reconcile 5,
        targeted refresh 2, folder-reorder DnD 1 deferred plus 3 drag callbacks,
        persistence 3, create/rename/delete 7, `Space` peek 1 plus 3 signal
        paths, workspace add/rename/unlist 4, **workspace scope filter** (see
        below) and **focused-folder drilldown** (see below).

        **Two stage orders authoring's first pass missed, both confirmed in the
        code, and both of which this row owns:**

        1. **Workspace scope filter.** `ui/sidebar/workspaces.rs:166`
           `animate_workspace_filter_change` runs a revealer fade and settles
           through `workspace_filter_settle_timer` — **that is the one
           `SupersedingTimer` in the count above**, so its primitive was being
           counted while its stage order was not named. This row also owns the
           `filter_animation_active` evidence field and the
           `workspace-filter-animation` readiness blocker that project from it
           (tasks 6.1, 8.1, 8.2), so leaving the stage order unnamed would
           narrate a projected field with no stage behind it.
        2. **Focused-folder drilldown.** `workspace_section/folders.rs:247`
           `focus_folder` plus the `drilldown_stack`, entered from
           `row_factory.rs:107`, with **four DnD gates keyed on drilldown
           emptiness**. The live spec already names "focused-folder mode" as a
           required sidebar state extreme, so this is a documented mode, not an
           incidental path.

        **Close the arithmetic rather than reporting both figures.** The
        per-stage-order numbers above sum to 32 against a stated primitive total
        of 27, and the trace must reconcile that: attribute every primitive to
        exactly one stage order, count non-primitive callback resumptions
        (dialog, `FileDialog`, drag, selection) separately from primitives, and
        name any path shared between two stage orders once rather than in both.
        Report the reconciled totals; a trace whose subtotals do not sum is not
        a trace.
      - **`WFR-NOTES-BOOKMARKS`: five stage orders and roughly 30 inversions** —
        browser 9 plus 3 in the closed-file bookmark preview, bookmark
        toggle/edit 8, note editors 9 (document) and 10 (folder) with four shared
        save-sensitivity inversions, sidecar migration 3 **including the process
        boundary itself**, and the startup format gate 4 plus a user-driven retry
        loop.

      Two of these deserve to be named in the facade narration rather than
      counted: the **cross-process** resumption, where
      `migrate_note_sidecars_after_rename` records a ledger entry and control
      resumes in `reconcile_pending_migrations_on_startup` **on a later app
      launch** — the longest-lived inversion in the codebase — and the
      **deferred expansion restore**, where a callback must read the live
      expansion set at apply time because a stale snapshot resurrects a user's
      collapse. Correct the `Workflow Stage Traces` entries in task 9.
- [x] 0.5 **Record the contracts this change must preserve exactly**, before
      touching anything near them, into `evidence/durability-contracts.md` with a
      before/after section:

      - **Sidecar migration ordering and its retry ledger.** The rename path
        records pending work for all three kinds, then runs bookmarks, document
        notes, and folder notes in a fixed order under `run_tracked_kind`, with
        `MAX_MIGRATION_ATTEMPTS` bounding retries and startup reconcile finishing
        anything left. Record the current call order verbatim.
      - **Format-upgrade apply re-scans rather than trusting the dialog
        snapshot.** `run_startup_format_apply` re-runs `scan` and `build_plan` in
        the worker before applying, and a partial failure re-presents the dialog
        with the previous error rather than proceeding. That re-scan is a safety
        property, not redundancy.
      - **Workspace persistence latest-generation semantics**: debounce, one
        active write, a newer snapshot waiting behind it, bounded retry backoff,
        a current failure awaiting explicit retry, close bypassing debounce
        without falsely settling readiness, and close-time failure aborting close.
        `model/workspace_persistence.rs` encodes it and the readiness blocker
        documents it.
      - **The sidebar's local contracts** as `ui/sidebar/AGENTS.md` states them:
        one scan flight per materialized child store with lifetime/store/target/
        scan-generation agreement before publication; watcher targets as an
        incremental mirror of flattened-row splices rather than full model
        rescans, with at most one lifecycle worker per section; the GTK-free
        coalescing mailbox with its shared 1,024-path cap and one notice per
        poll; a pending full refresh dominating targeted paths; and DnD hover
        owned by the transparent row-level shield with any idle collapse being
        defensive only.
      - **Expansion-state authority**: `expanded_paths` is live per-section
        state kept current by row transitions, accepted reconciliation
        retirement, and rename prefix rewrites; the full model derivation is
        reserved for bootstrap, pre-replacement capture, and the test oracle; and
        deferred restore callbacks read the set **at apply time**.
      - **File-operation semantics**: create's unique-name policy, rename's
        empty/unchanged cancellation, the inline-rename focus-out double-fire
        guard, and prefix matching (not equality) for directory operations
        against open tabs.
      - **The `GtkTreeExpander` internal-gesture disable for file rows**
        (`workspace_section/row_factory.rs:336-341`). `connect_bind` walks
        `observe_controllers()` and sets the expander's internal
        `GtkGestureClick` to `PropagationPhase::None` for file rows and `Bubble`
        for directory rows, because that gesture otherwise intercepts clicks at
        BUBBLE phase and `GtkListView::activate` never fires. `.agents/rules/ui.md`
        records this as a **three-iteration lesson** with two rejected fixes
        (`single-click-activate=true` changed the UX; a CAPTURE-phase gesture was
        fragile and failed for the first file). It runs on **every** bind,
        including recycling. Record the current code verbatim: a role move that
        drops or reorders it reintroduces a bug the project has already paid for
        three times.
      - **The peek key controller's phase and gating** (`peek.rs:329`). The
        `EventControllerKey` is attached to the list view in
        `PropagationPhase::Capture` and gated against focused controls that must
        keep their own keys, because `GtkListView` keyboard focus lands on a
        realized row rather than on the list wrapper. A handler that assumes
        `list_view.has_focus()`, or that only works when a test emits the key
        directly on the list widget, passes synthetically and does nothing for a
        real user.
- [x] 0.6 Invoke the `data-safety` skill in explicit mode over the intended diff
      **before** implementing. Both rows are tier-3; this row's file operations
      rename and delete the user's own documents, and the notes row rewrites app
      data at startup. Slot 2b found two confirmed pre-existing defects, 3b one,
      and slot 4 one — **four slots for four** — so budget for findings and treat
      `.agents/rules/preexisting-blockers.md` as binding: a confirmed finding is
      fixed in this work stream, not recorded as debt, even though the non-goals
      say "no behavior change". Two candidates authoring already noted are in
      task 7. Record in A.9.
- [x] 0.7 **Catalogue the test reach-through by field name, not by line, and
      scope it honestly.** Authoring counted **190 ungated `.imp().` sites** in
      this slot's widget tests: `crates/lushtext/tests/widget/sidebar.rs` 32
      across 8 fields, `workspace_section.rs` **158 across 23 fields**, and
      `file_tree_item.rs` **0**. `sidebar.rs`'s 32 bucket the same way as the
      section's: 9 `sidebar` and 5 `tab_view` are widget/owner handles reached
      from the window, 7 `new_workspace_button`, 4 `workspace_list_revealer`, 3
      `workspace_filter_dropdown`, 3 `outer_scrolled_window`, and 3
      `new_workspace_box` are template children, and **2 `sections` are the
      workflow-state reads** — the per-workspace section collection this row's
      evidence surface must cover under task 4.6's child-collection rule. Notes-side: `window.rs` holds **16 ungated
      `imp().startup_data_flow.completed` reads** — the workflow's largest untyped
      seam — plus 6 `imp().notes_menu_button` sites, while the notes browser is
      already observed through a typed snapshot.

      **Not all 190 are in scope, and saying which is part of the task.** Roughly
      113 of the 158 are `TemplateChild` widget handles (`refresh_button` 35,
      `file_tree_view` 30, `add_folder_button` 12, `collapse_button` 10, …):
      those are widget access, not workflow state, and the convention's target is
      state observation. The ~45 that **are** in scope are the private runtime
      reads — `watch_runtime.watcher` / `.poll_source_id` (12),
      `top_level_store` (10), `tree_model` (8), `drilldown_stack` (5),
      `refresh_runtime.pending_full_reload` / `.pending_paths` (2),
      `original_folders` (2), `is_new_item` (2), `workspace_folder_ids` (1), and
      the three callback slots — plus the 16 `startup_data_flow.completed` reads.
      Categorize every in-scope site as *evidence read*, *real drive through an
      existing seam*, or *needs a counted actuation seam*, record the
      out-of-scope classification with its reason, and record all of it in A.7.
      Follow slot 3a's finding: **an ungated `imp()` write is usually a real
      drive in disguise** — reach for an existing configuration seam plus a real
      drive before adding a counted seam.
- [x] 0.8 **Production code has the same problem here, which no previous slot
      hit.** `ui/automation.rs` reads `imp.sidebar.imp()
      .workspace_filter_animation_active` at two sites (the readiness blocker and
      the workspace snapshot). That is a production widget-internals reach-through
      across a workflow boundary, and it must become a named accessor or an
      evidence projection in task 8. Sweep for others of the same shape
      (`\.imp\(\)\.` in `ui/` outside a widget's own module) and record the
      population before changing any of it.
- [x] 0.9 **Make new files visible to the diff-aware gates before running any of
      them.** Every slot in this programme adds new role directories, and
      `git diff origin/main` does not mention an untracked file. Slot 4 hit this
      twice: `make mutants-diff` could not see its new policy modules, and
      `make check-visual-proof-policy` **passed** while its changed-files list
      omitted 24 new files, only starting to fail once they were visible. Run
      `git add -N` on every new file and directory **early**, before the first
      diff-aware gate, and treat a green diff-aware gate on a tree with untracked
      new files as unproven. The fix is to keep the files visible and re-run the
      lane, never to reset the index and take the green.
- [x] 0.10 **Confirm what this change does not inherit.** Slot 4's B.3 deferred
      three simplify candidates; all three are in `drafts/journal.rs`,
      `local_history/preview_execution.rs`, and `ui/window/imp.rs`, and all three
      are **slot 7's** — verify none has migrated into this slot's files and
      record the confirmation. Slot 4's two `[~]` acceptance items (the
      live-session paned proof and the quiet-machine `bench-compare`) stay slot
      4's, user-availability-gated: this change neither ticks nor re-plans them.

## 1. Apply the convention amendments and pay the retroactive re-check

- [x] 1.1 Confirm each amendment's basis from the code and the specs before
      amending anything.

      - For **(a)**, quote the two pre-convention requirements that mandate module
        *topics* — "Window notes are organized by existing workflows" and
        "Workspace-section wiring has focused owners" — beside the role
        requirement that mandates one *role* per module, and show that a
        migration of either directory currently appears required to violate one
        of them. Confirm that neither `bookmarks`, `editors`, `browser`,
        `row_factory`, `context_menus`, nor `row_accessibility` is a bounded role
        name.
      - For **(c)**, confirm that the role taxonomy sentence enumerates exactly
        five names and that **no existing spec or rule text treats a presentation
        or called surface as a role** — a grep of `openspec/specs/` and
        `.agents/rules/` finds none — while slot 4 nevertheless shipped exactly
        such a module (`ui/editor_page/local_history.rs`, its per-tab capture
        surface), recorded in the matrix and in its own module doc rather than as
        a role. The amendment states the taxonomy's **scope** to match that
        practice; confirm it does not widen the five-name set and does not add a
        sixth role.
      - For **(b)**, confirm from the spec text that the two permitted role homes
        are described per directory and that neither covers a workflow owning a
        directory **and a widget subdirectory of it**. Confirm that
        `ui/**/policy.rs` in `.cargo/mutants.toml` reaches both candidate
        locations, and re-verify it after any move.
      - For the **evidence-surface** amendment, confirm the hazard from the code
        rather than from reasoning: `tree_index.rs`'s `find_store_for_dir` and
        `visible_child_stores` call `row.children()` — the latter with **no
        `is_expanded()` filter** — which runs the `GtkTreeListModel` create
        function, populates a child store, and **starts a background scan**;
        `refresh.rs`'s `expanded_store_index` is safe **only** because of an
        `is_expanded()` guard; the deferred restore path calls
        `set_expanded(true)`, which materializes children and fires the
        `notify::expanded` hook that queues a **watcher restart**;
        `derive_expanded_paths_from_model` does not materialize but **increments
        the capture counters that are themselves asserted as evidence**, so an
        observer calling it corrupts the metric it observes; and `find_dir_row`
        mutates its cache on a nominal read. Record all five.
- [x] 1.2 Apply this change's `gtk-adapter-module-boundaries` delta: the
      classification statement on both pre-convention decomposition requirements,
      the **taxonomy-scope** statement that a called presentation surface is not
      a role and owns no `policy.rs` or `evidence.rs`, and the nested-role-home
      sentence — all three on/beside the role requirement. Apply the
      `workflow-evidence-surfaces` delta: the no-materialization statement and the
      child-collection statement on the evidence-surface requirement. **Nothing
      beyond those three statements may be absorbed.** The facade line budget, the
      bounded coordination role set, the seam value-object shape, and the
      evidence-surface visibility rule are not amended by this change.
- [x] 1.3 **Retroactive-amendment obligation — eight rows, checked
      individually.** Under section 8, re-check `WFR-SEARCH-REPLACE`,
      `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`, `WFR-DOCUMENT-LOAD`,
      `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, and
      `WFR-DRAFT-RECOVERY` against **all four** statements. Three are *expected*
      to be confirmations — no migrated row lives in a directory governed by the
      two topical requirements, none has a nested role home, and none reads a
      lazily materialized model — but slot 3b's amendment found one of three rows
      lacking its proof and slot 4's found **two of four** rows non-compliant, so
      "it must already hold" is not a discharge. Per row, record:

      - whether any of its modules carries a topic name without a declared role;
      - **whether it owns a module that is none of the five roles, and whether
        that module is recorded as a called presentation surface with its
        ownership in its own module doc *and* named in its matrix row.** This one
        is **not** expected to be a pure confirmation:
        `WFR-LOCAL-HISTORY` owns exactly such a module
        (`ui/editor_page/local_history.rs`), and while slot 4 recorded it in both
        places, the other seven rows must be checked for an unrecorded one rather
        than assumed clean. If a row has one that is unrecorded, record it **in
        this change**;
      - whether its role home is flat, per-workflow subdirectory, or nested;
      - whether its evidence accessor reads any lazily created toolkit collection
        or any counter that its own surface reports.

      Fill any gap **in this change**. Record each verdict in the matrix as a new
      `### Slot 5 amendment re-check` subsection.
- [x] 1.4 Re-confirm in that same subsection that the other settled conventions
      are untouched: role file names, the bounded coordination role set, the
      facade budget number, the seam value-object shape, the evidence-surface
      visibility rule, the reentrancy constraint, cross-cutting eligibility, the
      row-cell re-derivation obligation, and the evidence pointer form.
- [x] 1.5 Update standing guidance where a reader would look for the new
      statements: `.agents/rules/rust.md`'s Workflow Vocabulary section for the
      nested role home and the role/topic reconciliation,
      `.agents/rules/widget-wiring.md`'s evidence-surface rules for the
      no-materialization and child-collection statements, and
      `.agents/rules/documentation.md` if either changes what a
      workflow-structure change must update. Run `make check-agent-docs` and
      `make check-agent-skills`.

## 2. Structural and shared-ownership decisions that must precede both migrations

Each of these is a boundary that would otherwise be decided twice, from two
sides, or a decision that would force facade work to be redone. Decide each
**before** touching the workflow that would absorb it, and record the decision
with its reason in `evidence/shared-ownership-decisions.md` (summary in A.3).

- [x] 2.1 **`ui/window/documents.rs`'s rename join — the seam between this
      slot's two rows.** A completed rename calls
      `migrate_note_sidecars_after_rename`. Decide the shape of that call as a
      **named operation on the notes facade**, invoked from the tree side, and
      record which row owns the ordering guarantee (the sidecar migration must not
      begin before the rename's own cache, watch, and expansion updates settle).
      This is the reason the notes row migrates first; do not let the tree row
      absorb the migration entry point.
- [x] 2.2 **`ui/window/startup_data.rs` (435 lines) — ownership, with a third
      option the handoff did not anticipate.** Slot 4 decided it belongs to
      **neither** restore row and that its census home is `WFR-NOTES-BOOKMARKS`.
      Authoring's read shows it is more entangled than that: after the
      format-upgrade gate resolves, `continue_startup_data_flow` calls, in order,
      `reconcile_pending_migrations_on_startup` (notes),
      `sidebar.load_workspaces()` (**this slot's other row**),
      `refresh_workspace_scope_consumers`, `flush_pending_activation_opens`,
      `load_session_and_drafts` (migrated `WFR-SESSION-RESTORE`), and
      `start_autosave_timer` (migrated `WFR-DRAFT-RECOVERY`). It therefore calls
      into **four owning workflows across six ordered calls** from one function.
      Decide between:

      1. a coordination role module inside the notes role home, on the grounds
         that the format-upgrade gate is notes-family app-data work and the rest
         is release ordering;
      2. a **called surface** at `ui/window/` whose ownership is recorded in its
         own module doc, the shape slot 4 used for `document_identity.rs`;
      3. **cross-cutting, owned by neither**, recorded in the matrix's
         Cross-Cutting Coordination section with its four owning callers named.

      Decide by owning workflows, not by call count. If the answer is (3), the
      notes row's file set loses this file and **that is a census correction about
      ownership rather than counts** — the class slot 4 flagged as more dangerous
      than a wrong number, because a name invites trust. Whatever the outcome,
      `load_session_and_drafts` and `start_autosave_timer` stay **calls** into
      migrated facades, and the gate's one-shot `completed` latch keeps its
      current semantics.
- [x] 2.3 **`ui/window/imp.rs`'s `StartupDataFlowState` — and its unbounded
      queue.** The struct holds `completed`, `running`, and
      `pending_activation_paths: RefCell<Vec<PathBuf>>`, which accumulates
      desktop/CLI open paths while the gate is pending and is drained once. It has
      **no cap**. Record whether that is a real boundedness gap under this
      repository's bounded-work programme (a user can only activate so many
      files, but the queue is fed by an external interface), and route the verdict
      through task 7 rather than deciding it inside a naming task.
- [x] 2.4 **`NoteSourceRefreshCoordinator` — the retirement slot 2a deferred, and
      the blocker this change dissolves.** Two independent instances exist:
      `command_palette_note_refreshes` on the window imp, serving **migrated**
      `WFR-COMMAND-PALETTE`, and `source_refreshes` inside `NotesBrowserState`,
      serving the browser. Slot 2a's stated blocker was that deduping the type
      changes `NotesBrowserRuntimeSnapshot`'s shape — **that snapshot is this
      row's, and this change folds it into the row's evidence surface anyway**, so
      the blocker is dissolved by this slot's own work rather than deferred a
      third time.

      Authoring's comparison, to be verified rather than trusted:
      `submit`/`finish`/`invalidate`/`is_current`/`has_work`/`snapshot` are
      otherwise equivalent to `services::single_flight::SingleFlightCoordinator`;
      the differences are that the shared type is generic over the request, its
      snapshot carries two additional high-water fields, it additionally exposes
      `clear_pending()` and `active_generation()`, and its cancellation type
      differs **by name only**. All the state that actually differs between the
      two instances lives in the surrounding structs, not in the coordinator. Two
      sibling types in the same service — the browser query coordinator and the
      bookmark-excerpt preview coordinator — are **already aliases** over the
      shared coordinator, so this is the last unaliased one in the workflow.

      Decide, and if it is retired: the palette's instance changes type too, which
      touches a **migrated** row. That is permitted **only** as a type-level
      substitution with the palette's evidence surface, its exported snapshot
      fields, and its readiness blockers proved unchanged — never as a
      restructuring of a migrated workflow. Name the property-test and widget-test
      call sites that move with it.
- [x] 2.5 **Decide the `WFR-WORKSPACE-TREE` role home by collision analysis, and
      do it before any facade text is written.** `ui/sidebar/` hosts one workflow
      but is a **nested** pair of directories. Enumerate every module's role
      candidate first: the orchestrator (`mod.rs` facade, `workspaces.rs`,
      `callbacks.rs`, `dialogs.rs`, `imp.rs`) and the section
      (`watch.rs` — **already a bounded role name**, `refresh.rs`,
      `tree_loading.rs`, `tree_index.rs`, `watch_targets.rs` — **already pure and
      policy-shaped**, `folders.rs`, `actions.rs`, `peek.rs`, `dnd.rs`,
      `context_menus.rs`, `row_factory.rs`, `row_accessibility.rs`,
      `icon_presentation.rs`, `imp.rs`). Then choose between:

      1. all roles flat in `ui/sidebar/` with `workspace_section/` recorded
         wholly as called presentation surfaces — which is dishonest while
         `watch.rs`, `refresh.rs`, and `tree_loading.rs` are coordination;
      2. **canonical role home `ui/sidebar/` with nested coordination role
         modules inside `workspace_section/`**, which the delta in task 1.2
         states, and one `policy.rs` plus one `evidence.rs` at the canonical
         home;
      3. a per-workflow subdirectory, which buys nothing because the directory
         hosts one workflow.

      Whichever is chosen, **exactly one `policy.rs` and one `evidence.rs` exist
      for the row**, and every module in both directories is classified as exactly
      one of: the narrative facade, a bounded coordination role, pure policy,
      seam value objects, evidence, or a **called presentation surface, which is
      not a role** and therefore takes no role name and owns no policy or
      evidence. The presentation modules keep their behavior obligations from the
      "Workspace-section wiring has focused owners" requirement with their
      ownership recorded in their own module docs and named in the matrix row.
      Record the analysis in A.11.
- [x] 2.6 **Decide, early, whether `WFR-WORKSPACE-TREE` is one row or two — and
      resolve the facade budget question before writing the facade rather than
      after.** Authoring's read found **eleven ordered stage orders** in this one
      row once the workspace scope filter and the focused-folder drilldown are
      counted (task 0.4), and task 0.4's reconciled trace is the number this
      decision uses rather than authoring's. Four slots agree that stage-order
      count is what stresses the 370-line budget, and the exemplar's *two* stage
      orders sit one line under it, so an eleven-stage-order facade is the
      programme's first honest test of the number. The options, in order of
      preference:

      1. **One facade, delegated hard.** Apply slot 2b's exact sequence —
         delegate every stage body, compress each inversion to one line, fold
         module-ownership detail into the role table and the shared-state table —
         and measure. This is the first thing to try and it may work; the palette
         narrates two stage orders in 335.
      2. **Escalate the budget in-change with the measured count.** Raising the
         number is a convention amendment costing a **ten-row** retroactive
         re-check (eight migrated rows plus this change's two). Make the case
         explicitly with the honest measurement, or make the narration fit. Do
         neither by editing the budget line quietly.
      3. **Split the census row**, separating the workspace-list/persistence
         workflow from the file-tree/watch/file-operations workflow. This is
         available **only** with evidence that the two halves genuinely have
         separate entry points, separate state groups, and separate seam
         populations — for which there is a real prima facie case (the sidebar
         imp versus the section imp; `dialogs.rs`'s six dialog seams versus the
         section's fifty-two; the persistence pipeline versus the scan/watch
         pipeline) — and it is recorded as a **census correction with that
         evidence**, given a new stable `WFR-*` row id, and **both** resulting
         rows migrated in this change so the split leaves no new unmigrated row
         behind. It MUST NOT be used as budget avoidance for one workflow that
         simply narrates a lot.

      Decide before section 4 begins and record the decision with its
      measurements. If option 3 is taken, the ledger line, the remaining-scope
      table, and the risk-tier table all gain the new row in task 9.
- [x] 2.7 **Check the `journal` role name once, for both rows, before creating any
      module.** The test is slot 3a's, re-applied by 3b and confirmed by slot 4 for
      three durable rows at once: *does a later stage of the same workflow read the
      record back*, not *does it touch the disk*. Apply it separately to:

      - the **notes migration ledger** — pending entries written on rename and
        read back by startup reconcile **in a later process run**, with a bounded
        attempt count; expected to pass, and the strongest `journal` fit in the
        programme so far;
      - **note and bookmark sidecars** — written on save, read back on the next
        open, browser build, or palette source refresh; decide whether that is a
        journal or ordinary durable service persistence, and say which stage
        reads it back;
      - the **format-upgrade backup** — written before apply and read back only
        by recovery; decide explicitly rather than by analogy;
      - **workspace persistence** (`workspaces.json`) — written on debounce and
        read back at the **next launch** by `load_workspaces`; check `journal`
        against it and against the alternative that this is an `execution` plus
        latest-generation `admission` shape, because slot 4 established that a
        `journal` owns the gate that serializes its own writes.

      Per slot 2b's definition, the mutual-exclusion gate serializing a record's
      writes and any reservation those writes take live **inside** the journal,
      not in a separate `admission`. If a stage order genuinely fails the test,
      say so and pick from the bounded set rather than stretching a name. Record
      the mapping per stage order in A.8, and amend
      `gtk-adapter-module-boundaries`' role set **only** if a genuinely novel
      coordination job appears — an escalation, not an absorption. Note in
      passing that the nine tree stage orders will need stage-order-qualified
      names, and that `watch.rs` is already correctly named: **qualify only the
      new modules**, per slot 2b's narrow reading, and do not rename a stable
      correct sibling for symmetry.
- [x] 2.8 **Decide the buried-pure-policy question for the shared services
      explicitly**, per slot 2b's `services/search_backup.rs` precedent, and
      record that a `services -> ui` relocation is **forbidden outright**:
      `services/file_tree.rs`, `workspace_manager.rs`, `workspace_watch.rs`,
      `file_peek.rs`, `note_storage.rs`, `bookmark_service.rs`,
      `bookmark_excerpt.rs`, `folder_note_service.rs`,
      `document_note_service.rs`, `migration_ledger.rs` (cross-cutting, staying),
      and `services/format_upgrade/**`. Behavior unchanged; the decision is
      whether any pure decision inside them belongs to one of this slot's rows,
      and the answer for a service with a `model/` or second-service consumer is
      no.
- [x] 2.9 **Confirm the closed boundaries and do not re-open them.**
      `model/workspace_search.rs` (closed by 2b), `model/file_load.rs` (closed by
      3b), `model/buffer_replacement.rs`, `model/editor_memory.rs`,
      `model/migration_ledger.rs`, `ui/plain_disposal.rs`, `ui/buffer_snapshot.rs`,
      `services/single_flight.rs`, and `services/sync.rs` are cross-cutting,
      exempt, or decided. Record the confirmation so a reader does not think the
      question is open.
- [x] 2.10 **Record what is excluded by scope, where a reader will hit the
      adjacency.** `WFR-SHELL-LAYOUT` (slot 7) keeps the workspace sidebar
      show/hide animation and the recent-documents surface;
      `ui/sidebar/file_tree_item.rs` is a surface with no coordination tier;
      `WFR-MINIMAP` is slot 6. `ui/window/local_history/restore_execution.rs`'s
      two `resolve_notes_for_editor` calls and `local_history/journal.rs`'s
      `MigrationKind` record **stay calls** into a migrated row. The command
      palette's note source is migrated: this change may substitute its
      coordinator type under task 2.4 and must not otherwise restructure it.

## 3. `WFR-NOTES-BOOKMARKS` — migrate first, because the tree's rename path calls it

- [x] 3.1 From the code, record this workflow's cohesive coordination jobs and
      apply the cohesion test 3b recorded — *is the job cohesive enough that a
      reader would look for it under its own name* — rather than grouping by
      adjacency. Candidates from authoring's read: the browser's bounded source
      build plus its query (two stage orders of the same shape, so **expect
      stage-order-qualified execution modules**), the bookmark lifecycle with its
      debounced sidecar persistence, the note editors' open/save pipelines with
      their save-sensitivity rerun path, and the migration ledger from task 2.7.
      The closed-file bookmark-excerpt preview is a fifth candidate; decide
      whether it is its own coordination job or part of the browser's.
- [x] 3.2 **Role home**: `ui/window/notes/` hosts exactly one workflow, so flat
      role names in that directory are available and `mod.rs` is the facade. Confirm
      that no sibling workflow under `ui/window/` needs those names, and record the
      choice in the row. Re-express the existing focused siblings under the task
      1.2 statement: each of `browser.rs`, `bookmarks.rs`, and `editors.rs`
      becomes either a role-named coordination module or a recorded called
      presentation surface carrying no role, **with its behavior obligations from
      the "Window notes are
      organized by existing workflows" requirement unchanged**. Note that
      `browser.rs` currently holds **two different surfaces** — the browser dialog
      and the command-palette note-source refresh — which is a role collision in
      one file, not merely a long file.
- [x] 3.3 Extract this workflow's pure decisions into
      `ui/window/notes/policy.rs`. This is expected to be a **gain from zero**,
      not a relocation: all five notes domain modules stay in `model/` (task 3.7).
      Authoring found these already-pure and extractable as-is — the folder-note
      target selection over zero/one/many folders, the open-editor snapshot heap
      weight, the bookmark target-line parse, three user-message mappings for
      bookmark edit/unavailable/excerpt formatting, the raw-excerpt formatter with
      its preview offsets, the startup dialog's heading/body selection and its
      four plan-group summary strings, the browser's fourteen mode label and
      description methods, and the worker-side guarded source outcome with its
      reservation shrink — plus these entangled with `self.imp()` and therefore
      higher value: the current folder-note open target and action availability,
      the bookmark menu-label decision, the open-editor snapshot **capacity and
      byte arithmetic**, the browser limit-label composition, the
      selection-preservation remap, and **three duplicated mode-to-status-string
      matches** that a single policy function should own.

      Apply slot 4's three hazards while extracting: do not create a
      **tautological** predicate that proves nothing to the compiler and forces a
      dead default at the call site; **bind a `borrow()` to a local before a
      `match`**, because a match scrutinee's temporaries live for the whole match
      while an `if` condition's drop before the block; and **pin every policy
      constant to a concrete literal in the units a reader would sanity-check**,
      because an assertion comparing a value against the constant it came from
      cannot detect the constant changing — slot 4's single most common survivor.
- [x] 3.4 **Reify `NotesBrowserTicket`** in `ui/window/notes/seams.rs` — reusing
      slot 4's precedent module name, not inventing another. The bundle is the
      `(generation, mode, disposed)` triple compared clause-by-clause at the
      source-load and query completions, with a degenerate two-clause variant at
      the bookmark-preview completion. Construct it once at submit time, validate
      it as a unit, and decide explicitly whether the preview completion takes the
      same type with a documented weaker predicate (the shape 3b used for
      `installation_is_current`) or a distinct one. The `mode` field is the reason
      this seam exists: a value that means "the mode this request was issued for"
      must not be comparable against "the mode the browser is in now" by accident.
- [x] 3.5 Build `ui/window/notes/evidence.rs` at the narrowest visibility its
      readers require, folding in **every** pre-convention typed observation so no
      second path remains: `NotesBrowserRuntimeSnapshot` and its nested source,
      query, and preview coordinator snapshots; `OpenEditorNoteSnapshots`; and the
      note-save snapshot count whose current test getter has a **side effect** (it
      prunes) — that pruning must not survive into an evidence read. Discharge the
      three stated proofs plus the two new ones:

      - **tight borrow**: compute every derived scalar and drop each `Ref` before
        the struct literal;
      - **disposed widget**: any field derived from a `TemplateChild` reads
        through `try_get()` and answers honestly for a disposed dialog or window;
      - **reentrancy**: a test that drives the workflow through each operation
        taking a mutable borrow of the state the accessor reads, reads the surface
        **after** each one, and asserts repeated reads of unchanged state are
        identical. A test that reads the surface *while* a borrow is held is the
        failure the constraint prevents, not a proof of it;
      - **no materialization**: reading the surface starts no worker, arms no
        timer, and creates no lazily built collection;
      - **child collection**: any field aggregated over open editors or over the
        browser's rows is bounded and answers honestly at zero.
- [x] 3.6 Migrate this row's seams. Retire every inspection function into the
      surface with no remaining callers; collapse the configuration seams —
      including the browser source entry limit, the bookmark excerpt preview
      delay, and the two service-side delays if their owner is decided to be this
      row rather than the service — into **one** `notes/test_policy.rs` value
      entirely behind `#[cfg(feature = "test-utils")]`, keeping every public
      setter name; and classify each actuation seam and worker-delay hook,
      preserving them as programme-level deferrals with **zero added** unless a
      new one is counted and justified individually. Migrate the 16 ungated
      `startup_data_flow.completed` reads according to task 2.2's ownership
      decision: they become evidence reads on whichever surface owns the gate.
- [x] 3.7 **Confirm the five domain modules stay, and record why with the
      evidence.** Authoring found that `model/note.rs`, `model/bookmark.rs`,
      `model/sidecar_identity.rs`, `model/folder_note.rs`, and
      `model/document_note.rs` are **each consumed by at least one GTK-free
      service**, so relocating any of them under `ui/` would invert dependency
      direction — forbidden outright, the same ground on which 2b settled
      `workspace_search.rs` and 3b settled `file_load.rs`.
      `model/sidecar_identity.rs` is the strongest case: it is consumed by note,
      bookmark, document-note, draft, local-history, search-backup, and
      format-upgrade code, which makes it a **cross-workflow kernel**, not this
      row's policy. Verify each consumer set by import or path rather than by
      substring, name the false-positive families, and update the
      `Policy Module Census` in task 9. **Never manufacture a local `policy.rs`
      by forking part of one of these.**
- [x] 3.8 Run mutation on the new `policy.rs` **before** writing this row's
      evidence file, per slot 4's finding that extracting a decision does not
      automatically test it: expect survivors on every `-> bool` predicate, every
      accessor, and every method whose only observable effect is a side effect.
      Record generated/killed/missed/unviable with the exact `make mutants-diff`
      invocation and **file-level anchors, never line-precise ones**, in
      `evidence/mutation-notes-policy.md`, and report it as a **gain from zero**
      with no parity claim attached, because nothing relocated. Expect the
      focused-run floor from `cargo-mutants` 27's field-deletion mutants and do
      not attribute its pre-existing survivors to this change.
- [x] 3.9 Prove behavior equivalence for this row, each case asserting the
      user-visible outcome and the resulting record, in
      `evidence/notes-behavior-equivalence.md`: browser with no notes, one, many,
      a query with no matches, and a truncated source; **rapid repeated mode
      switching that must not let a stale completion publish**, which is the seam
      value object's whole purpose; bookmark toggle and label edit with no editor,
      an unsaved editor, one bookmark, and many; a debounced save superseded by a
      newer one and a save that fails; document-note and folder-note open, save,
      discard, and the folder chooser's zero/one/many paths; the first Edit ->
      Render activation for an existing non-empty note and for an initially empty
      one, per the dialog edit/render geometry rule; a rename that migrates
      sidecars inline and one whose migration is deferred to the ledger and
      reconciled on the next launch, including an attempt-cap exhaustion; and the
      startup format gate for equal, older-upgradeable, and newer app data,
      including an apply that partially fails and re-presents the dialog. Cover the
      state extremes the UI rules require for collection surfaces, and assert the
      grouped-row copy the decision-dialog rule requires for the format-upgrade
      dialog.

## 4. `WFR-WORKSPACE-TREE` — the programme's largest row, migrated last

- [~] 4.1 From the code, record this workflow's cohesive coordination jobs across
      the stage orders task 0.4 enumerated, and map each to a bounded role name
      with a stage-order qualifier where the shape repeats. Authoring's read
      suggests, to be confirmed rather than assumed: the child-store scan
      lifecycle with its global admission permit and batched reconciliation; the
      watcher lifecycle, mirror, and mailbox drain (`watch.rs`, **already
      correctly named**); refresh coalescing and planning; the persistence
      pipeline with its latest-generation state and close-time flush; the file
      operations; the **workspace scope filter** fade and its settle timer, whose
      `filter_animation_active` state this row projects to automation; and the
      **focused-folder drilldown** stack, whose emptiness four DnD gates read.
      `watch_targets.rs` is already pure and GTK-free and is a
      **policy or seam candidate**, not a coordination module. Apply the cohesion
      test per job, and do not split a state machine whose forward and abort
      phases belong together — the judgement 3b recorded.
- [~] 4.2 Implement the role home decided in task 2.5 and the row shape decided in
      task 2.6. Every module in `ui/sidebar/` and `ui/sidebar/workspace_section/`
      carries the classification task 2.5 assigned it — one role, or called
      presentation surface with no role; the presentation modules
      (`row_factory.rs`, `context_menus.rs`, `row_accessibility.rs`,
      `icon_presentation.rs`, and the two `imp.rs` files) keep their existing
      behavior obligations with ownership recorded in their own module docs and
      named in the matrix row; and `ui/sidebar/AGENTS.md` is updated in
      the same breath so its Responsibilities, Local Contracts, and Editing Rules
      describe the migrated shape rather than the pre-migration one.
- [~] 4.3 Extract this workflow's pure decisions into one `policy.rs` at the
      canonical role home. Authoring found a large, unusually clean population.
      Already-pure free functions that only need relocation: refresh-directory
      minimization and its common-prefix/suffix helpers, the desired-folder-rows
      computation, the changed-path-to-owning-directory resolution (which needs a
      key-set view rather than a `ListStore`), the DnD post-drop index and
      before/after edge computations plus the hover verdict and the payload
      encode/decode, the watch target-for-folder and start-error message, the peek
      metadata/size/time formatters, the icon selection pair, the context-menu key
      predicate and header-background hit test with their declarative spec tables,
      the row accessible-description builder, and the file-row open/active visual
      state. Decisions currently inline in GTK adapters, which are the highest
      value: **refresh coalescing** (full versus paths, cap-overflow promotion to
      full, manual versus auto debounce, drop-when-already-full), the
      **full-versus-directories refresh verdict**, the desired-versus-current
      top-level row diff that drives the splice window, the **readiness
      predicate** over its scalar inputs, the **expansion transition rule**
      (collapse prunes descendants, with its ambiguity fallback) and the rename
      prefix rewrite, rename validation and the unique-name creation policy, the
      persistence error-to-message mapping and terminal-effect routing, and the
      auto-expand-versus-remembered-intent decision.

      Apply the same three hazards as task 3.3. Pin the caps and budgets this row
      owns — the shared mailbox path cap, the reconciliation batch size, the scan
      admission ceiling, the persist debounce — to concrete literals with the
      user-facing reason beside them, and assert against those literals rather
      than against the constants.
- [~] 4.4 **Reify `WorkspaceWatchTicket`** in the row's `seams.rs`, carrying
      `{targets_generation, lifetime_generation}`. Today the pair travels into the
      watch-install worker as a loose `(section_weak, generation, lifetime)` tuple
      and is compared clause-by-clause in the completion closure, where a lifetime
      mismatch retires the watcher and a generation mismatch re-enters the install.
      Construct it once at dispatch, validate it as a unit, and keep the two
      distinct consequences distinguishable — a single `bool` predicate that
      collapses "this section is gone" into "this generation is stale" would lose
      the retire-versus-restart decision.
- [~] 4.5 **Re-audit the scan side rather than re-inventing it.**
      `model/workspace_scan.rs` already reifies `WorkspaceScanTicket` with a
      one-active/one-latest flight and its metrics. Audit it against the
      two-boundary rule, confirm it is constructed once per admitted scan and
      validated as a unit, and record it as an existing seam value object that
      needed no change — or reify what the audit finds. Do the same for
      `watch_targets.rs`'s two generation newtypes and its snapshot.
- [~] 4.6 Build one `evidence.rs` for the row, at the narrowest visibility its
      readers require, folding in **every** pre-convention typed observation:
      `WorkspaceScanPressureEvidence`, `WatchTargetSnapshot`,
      `WorkspaceWatchMailboxSnapshot` as the row reads it,
      `SidebarFileRowStateSnapshot`, the refresh and reconciliation metrics, the
      child-cache rebuild metrics, the scan admission active/high-water counters,
      the expansion capture metrics, and the DnD hover fallback count. Discharge
      the three stated proofs, and then the two this change's delta adds, which
      **are the point of this row**:

      - **no materialization.** The surface MUST NOT call any accessor that runs
        the `GtkTreeListModel` create function or otherwise populates a child
        store, and MUST NOT call the full model derivation, which increments the
        very capture counters the surface reports. Prove it, do not assert it:
        read the surface with rows collapsed and with rows expanded and show that
        the scan admission counter, the child-store registry, the watcher
        generation, and the expansion capture metrics are **identical before and
        after** the read, and that no worker started. Record it in
        `evidence/evidence-surface-materialization.md`.
      - **child collection.** Fields aggregated across sections are bounded,
        answer honestly with zero workspaces, and **skip a disposed section
        rather than panicking on it**. Slot 4's disposal proof caught a real panic
        on a template-child deref; this row has N sections plus a window, so write
        the disposal proof before believing the surface is safe.
- [~] 4.7 Migrate this row's seams — the largest population in the programme.
      Retire every inspection function into the surface with no remaining callers,
      including the destructive-read "take touched rows" seam, whose reset must be
      separated from its observation. Collapse the configuration seams into
      **one** `test_policy.rs` value entirely behind
      `#[cfg(feature = "test-utils")]`, keeping every public setter name; this
      includes the eight test-only **fields inside `RefreshRuntimeState` and
      `WatchRuntimeState`**, which no `static` grep finds and which currently make
      production state structs carry test storage — and `WatchRuntimeState`'s
      permanent restart-suppression flag, whose meaning must be preserved exactly.
      Classify each actuation seam (the six dialog bypasses, the watcher
      merge/disconnect/poll/stop drives, the refresh queue/apply drives, the DnD
      hover simulations) as a programme-level deferral, preserve the oracles and
      probes (the readiness-predicate oracle, the derived-expansion oracle, the
      indicator-would-show predicates) as lifecycle probes with their reason, and
      add **zero** new seams unless one is counted and justified individually.
      Record before/after counts per kind.
- [~] 4.8 Migrate the in-scope widget-test reach-through from task 0.7: the
      private runtime reads become evidence reads, and any write becomes a real
      drive through an existing configuration seam wherever possible. Record the
      out-of-scope `TemplateChild` handle population and its reason so a later
      slot does not read the omission as an oversight. Do not weaken a test to
      make a seam retirement possible: if a test needs a fact the surface does not
      expose, **extend the surface**, never add a second narrow getter.
- [~] 4.9 Prove behavior equivalence for this row, each case asserting the
      user-visible outcome, in `evidence/tree-behavior-equivalence.md`: a
      workspace with zero, one, and many folders; an empty workspace preserved as
      a real section; a deep tree with long paths and the no-horizontal-scrollbar
      contract; expand and collapse, and **a user collapse racing a deferred
      restore callback**, which must not be resurrected; a directory scan
      superseded by a newer one and one whose section is gone when it resumes; a
      scan refused by admission and retried; a watcher install superseded during
      its worker (both the retire and the restart consequences); a watcher whose
      start fails terminally, which must settle readiness as unavailable rather
      than pending forever; a mailbox overflow that promotes targeted paths to one
      full refresh; a targeted in-place refresh after create, rename, and delete,
      including a directory rename matched by prefix against open tabs; a pending
      full refresh dominating queued targeted paths; folder-reorder DnD including
      an invalid drop position and a hover that must not expand a folder,
      materialize descendants, or restart a watch; `Space` peek including a stale
      request and a path that changed; inline rename including empty, unchanged,
      duplicate, and the focus-out double-fire guard; create with a colliding
      name; delete confirmation and cancellation; workspace add, rename, and
      unlist; a persistence write that fails and is retried, one superseded by a
      newer generation, and a close-time flush whose failure must abort close; a
      **workspace scope filter change** superseded by a newer one before its
      settle timer fires, with `filter_animation_active` settling exactly once;
      **entering and leaving focused-folder mode**, including the four DnD gates
      that must read drilldown emptiness consistently and a drilldown whose
      focused folder disappears from disk while focused; a **double-click on a
      file row opening a tab while a double-click on a directory row expands
      it**, which is the `GtkTreeExpander` gesture contract below; and `Space`
      peek reached by keyboard focus on a realized row rather than on the list
      view.
      Cover the sidebar state extremes the UI rules require, and assert the
      geometry contracts (header controls visible, only the item region scrolls,
      no horizontal scrollbar) where the case touches them.
- [~] 4.10 Run mutation on the new `policy.rs` before writing this row's evidence
      file, and record generated/killed/missed/unviable with the exact invocation
      and file-level anchors in
      `evidence/mutation-workspace-tree-policy.md`, reporting the extraction gain
      separately from task 5's relocation parity.

## 5. Policy relocation, mutation parity, and the inherited survivors

      **Line figures in this section are deliberately raw file totals including
      co-located tests**, unlike task 0.3's production-only re-derivation: a
      relocation moves the tests with the module, so the raw total is the size of
      the thing being moved. Do not "correct" them against 0.3.
- [~] 5.1 **Relocate `model/workspace_persistence.rs` (338 lines, raw total)** into the
      row's `policy.rs` with its co-located tests. Authoring found its consumer
      set is exactly `ui/sidebar/imp.rs` and `ui/sidebar/workspaces.rs` — no
      `services`, no `model`, and no bench consumer — so it is freely relocatable
      and is the programme's cleanest relocation so far. Verify that consumer set
      by import rather than substring before moving.
- [~] 5.2 **Relocate `model/workspace_scan.rs` (231 lines, raw total)**, with one
      complication to handle rather than discover: besides three `ui/sidebar`
      consumers, `crates/lushtext-core/benches/benchmarks.rs` references the
      public `model::workspace_scan` path at **two distinct references**. A move
      is therefore a
      **public-path break for the bench target**, which is exactly why slot 3a's
      `save_admission` relocation kept a precisely scoped `pub` subset. Decide
      between updating the bench imports and keeping a scoped re-export, and
      record the reason. Confirm that no `services` or `model` consumer exists,
      which would forbid the move outright.
- [~] 5.3 **Prove mutation parity for both relocations**, before and after, with
      the exact `make mutants-diff` invocation and file-level anchors, in
      `evidence/mutation-workspace-tree-policy.md`. Both sources are currently in
      `model/**` and therefore **already inside `examine_globs`**, so unlike slot
      4's two relocations there **is** a before-count and parity is a real claim
      rather than a gain: state the generated and killed counts on both sides and
      account for every difference. A relocation whose mutants are no longer
      generated is a coverage regression, not an acceptable consequence of the
      move.
- [~] 5.4 Confirm `model/workspace.rs` stays in `model/` as domain, with its
      consumer count re-derived as **owning workflows** and any `services`
      consumer named. Update the `Policy Module Census` rows for all three
      workspace modules in task 9, and leave a pointer where a reader following
      the old census snapshot would otherwise think a decision is still open.
- [x] 5.5 Confirm every new `policy.rs` is reachable by `examine_globs` through
      the literal `ui/**/policy.rs` convention — **including at a nested role
      home** — and imports no `gtk4`, `glib`, `gio`, `libadwaita`, or
      `sourceview5`. `make check-workflow-boundaries` enforces both halves; run it
      after every move rather than once at the end.
- [~] 5.6 **Triage the 11 pre-existing surviving field-deletion mutants in
      `services/file_tree.rs`**, which slot 4 explicitly handed to this slot as
      baseline rather than regression. Follow `.agents/rules/build.md`'s order and
      do not skip to the last step: first decide whether each mutant represents a
      real missed behavior; then add or tighten deterministic tests; then consider
      a small refactor that makes the behavior testable; and only then an
      exclusion, which must be narrow enough that nearby behavior still mutates
      and must carry a project-specific rationale. Record each of the 11 with its
      verdict in `evidence/mutation-file-tree-survivors.md`. Note the file already
      carries one narrowly scoped `exclude_re` entry for a symlink match guard;
      use it as the shape for any new exclusion, and do not widen it.
- [~] 5.7 Remember the focused-run floor while triaging: `cargo-mutants` 27's
      `--re` filter **does not apply to struct-field-deletion mutants**, so a
      "focused" run of a handful of policy mutants also runs every field-deletion
      mutant in scope. Report the floor explicitly and do not attribute its
      pre-existing survivors to this change.

## 6. Cross-row seam and reach-through work

- [~] 6.1 Retire the production `.imp()` reach-through **that belongs to this
      slot's two rows**, and hand the rest on rather than absorbing it. In scope:
      the automation module's two reads of the sidebar's filter-animation cell
      become a named accessor on the sidebar facade or a projection from the row's
      evidence surface, with the readiness blocker and the snapshot field keeping
      identical values.

      **Out of scope, recorded and handed on.** `ui/automation.rs` holds six more
      production `.imp().` reach-throughs owned by other rows: two `tab_view`
      reads at `:518` and `:519` (`WFR-SHELL-LAYOUT` / the tab workflow, slot 7)
      and four editor/minimap reads at `:1137`, `:1144`, `:1162`, and `:1224`
      (`WFR-MINIMAP`, slot 6). Record all six with their owning row in A.7 and
      name them in the B.2 handoff. Do **not** fix them here: each is a projection
      decision for the row that owns it, and fixing one from outside is how a
      migrated row acquires a change nobody planned.
- [x] 6.2 Reconcile the shared widget-test harness configuration. The widget
      harness currently calls four notes-side configuration setters and several
      tree-side ones at start-up; after the collapse into two test-policy values,
      the harness must set the same behavior through the new owners with no
      test-visible timing change. Keep the shared wait helpers from
      `crates/lushtext/tests/widget/common.rs` — `wait_until`, `flush_events`,
      `flush_after_delay`, `present_window` — and **do not add a private copy**
      or change a working helper's mechanism.
- [x] 6.3 Record the reach-through migration, before and after, per file and per
      field, in `evidence/widget-test-reach-through-migration.md`, with the
      out-of-scope population and its reason stated in the same file.

## 7. Data-safety candidates and the pass over this diff

- [x] 7.1 **Candidate: the unbounded `pending_activation_paths` queue** from task
      2.3. Decide with evidence whether an external activation interface can grow
      it without bound before the startup gate resolves, and whether a bound is
      required by this repository's bounded-work programme. Record the verdict
      either way; a "no" with evidence is a complete answer.
- [x] 7.2 **Candidate: the format-upgrade retry loop.** A partial apply failure
      re-presents the dialog with the previous error and the user may retry
      indefinitely. Confirm that each retry re-scans and re-plans rather than
      reapplying a stale plan, that a failed apply cannot leave app data in a
      state the next scan misclassifies, and that the backup written before apply
      is still recoverable after a failed second attempt.
- [x] 7.3 **Candidate: the detached cleanup thread on inline-create cancel.**
      Cancelling a new item spawns a plain detached thread to remove the temporary
      item with no completion path. Decide whether that can race a later create
      or rename of the same path, and whether it belongs on the guarded worker
      path the rest of the workflow uses.
- [x] 7.4 **Candidate: file-operation ordering against the watcher and the
      sidecars.** A rename updates the expansion set, clears directory state, sets
      the row path, refreshes the watch row, updates the item cache, fires the
      rename callbacks, and then triggers sidecar migration. Confirm from the code
      that no ordering here can lose a sidecar, resurrect a stale watch target, or
      leave the expansion set describing a path that no longer exists — and that
      the migration ledger covers the window where it could.
- [x] 7.5 Run the `data-safety` skill in explicit mode again **over the finished
      diff**, and record both passes with every finding and its disposition in
      `evidence/data-safety.md`. A confirmed finding is fixed in this change with a
      regression test proven to fail without the fix.

## 8. Automation: project two whole snapshot objects from evidence without widening

- [x] 8.1 Identify this slot's exported surface exactly, from
      `model/automation.rs` and `docs/automation-reference.md` rather than from
      memory, and record the pre-change values: the `window.workspace` object
      (`scope_kind`, `scope_workspace_id`, `scope_workspace_name`,
      `workspace_count`, `folder_count`, `scoped_folder_count`, `no_workspaces`,
      `persistence_inflight`, `persistence_dirty`, `filter_animation_active`); the
      `window.notes` object (`notes_menu_open`, `active_document_file_backed`,
      `active_document_bookmark_count`, `active_line_has_bookmark`,
      `document_note_available`, `folder_note_available`); the
      `workspace-persist`, `workspace-tree-refresh`, and
      `workspace-filter-animation` readiness blockers; the
      `workspace-refresh-complete` predicate and the `workspace-refresh` workflow
      id; and every predicate that lists one of those blockers, which includes
      `app-startup`, `recovery-restore-complete`, `visual-geometry-settled`, and
      `accessibility-settled`. **This is the largest projection surface any slot
      has owned** — two whole objects at once.
- [x] 8.2 Make those fields project from the two new evidence surfaces instead of
      re-deriving from widgets, keeping their names, types, and semantics
      **unchanged**. Where a readiness blocker needs one bool, read it through a
      cheap facade accessor identical by construction rather than building a whole
      surface per poll — the pattern 3a, 3b, and slot 4 all used and documented.
      Note that the tree row's readiness predicate is itself a pure function over
      scalars and belongs in `policy.rs`, so the blocker and the surface can be
      identical by construction rather than by inspection.
- [x] 8.3 **Decide two ownership questions rather than assuming them.**

      1. `workspace-sidebar-animation` is fed by the **window's** sidebar
         transition settle, not by the sidebar widget. The non-goals assign the
         show/hide animation to `WFR-SHELL-LAYOUT` (slot 7); confirm that the
         blocker follows the animation rather than the workspace row, and record
         the decision where slot 7 will find it. A blocker whose name starts
         "workspace" is not thereby this row's — the reusable form of slot 3a's
         "a field whose name contains save is not thereby save-workflow state".
      2. The palette's `command-palette-index` blocker currently includes the
         **notes** row's command-palette source coordinator as a disjunct, and the
         code comment there already names it as `WFR-NOTES-BOOKMARKS` surface
         area. Decide whether that disjunct now reads through the notes evidence
         surface or stays a direct call, and record why; it must not become a
         second derivation of the same fact.

      Record also the two **absences** so a later slot does not read them as
      gaps to fill silently: the notes browser dialog's own coordinators have no
      readiness blocker, and the startup format-upgrade flow has none either.
      Adding one would be widening; leaving them is the status quo this change
      preserves.
- [x] 8.4 **(tree half → 5b: `window.workspace` remains unprojected.)** Add `Evidence Projection Map` rows in `docs/automation-reference.md`,
      keyed by evidence type and attributed by the binding each field is read
      through. Three distinct pieces of gate work hide behind that one sentence,
      and they are **not** the same job:

      1. **`window.workspace` and `window.notes` are both new projecting
         objects.** Verified at authoring: the map holds rows for
         `window.content_search`, `window.command_palette`, `tabs[]`, and
         `window.local_history` only — **no `workspace` and no `notes` row
         exists**. Registering a new projecting object is different work from
         extending attribution, and it is the bulk of this task.
      2. **The dual-binding case.** The documented field id
         `snapshot-field-active-document-file-backed` is bound to **both**
         `notes.active_document_file_backed` and
         `local_history.active_document_file_backed`, so one documented row would
         map to two objects. 3b taught the gate per-binding attribution because
         two surfaces projected into one `tabs` object; this is the mirror case —
         one documented field across two objects. If the gate cannot express it,
         extend it, and prove the extension by confirming it still **rejects a
         real rename** rather than by assertion.
      3. **A decision, not a mechanical edit: `local_history` *derives* that
         field rather than projecting it.** The map projects only
         `browse_available` and `availability` from `LocalHistoryEvidence`;
         `local_history.active_document_file_backed` is derived directly in the
         snapshot function. If this change makes `notes.active_document_file_backed`
         project from the notes surface, the same user-visible fact is then
         reached two ways — which is exactly what task 8.3's second question
         forbids for the palette disjunct. Decide explicitly, and note the cost
         of each option: re-sourcing the local-history field touches a **migrated
         row** (permitted only as a proved-neutral projection change, with that
         row's exported fields and readiness blockers diffed to zero), while
         leaving both deriving keeps a documented inconsistency this change
         introduced. Whichever is chosen, state the rule that resolves it — the
         two objects report the *same* fact about the *same* active document —
         and record it where slot 6 and 7 will find it.
- [x] 8.5 Confirm every other new evidence field — generations, tickets, admission
      and mailbox counters, expansion sets, retained weights, queue depths,
      truncation state — is internal and reaches no snapshot. Confirm no note
      body, bookmark id, sidecar identity key, note id, or file path beyond the
      already-bounded scope fields can reach the schema; the existing redaction
      tests are the contract, and `docs/automation-reference.md`'s privacy
      boundary explicitly excludes note and bookmark identifiers.
- [x] 8.6 **(the two-tree `automation-smoke` capture-and-diff was NOT run; see `evidence/automation-no-widening.md` for what was proved instead and why the gap is narrow for this row.)** Prove no widening rather than asserting it: run `make automation-smoke`
      on a pre-change tree and on the changed tree under isolated headless Mutter
      and a private D-Bus session with the same fixtures, diff the `workspace` and
      `notes` objects, the action catalog, and **all** readiness predicates to zero
      differences, and record the normalizations applied and why each is about the
      fixture rather than the contract. Write to
      `evidence/automation-no-widening.md`. **Keep the comparison worktree's path
      short** — slot 4 lost a run to `libmutter-ERROR: Failed to create socket`
      under a deep scratch path, a message that says nothing about path length.
- [x] 8.7 Carry `WFR-AUTOMATION-SPINE` forward as `(partial)`: on slot 5's
      complete ledger line and on slot 6's outstanding line, and in slot 6's
      remaining-scope row. It stays `pending` in the matrix rather than
      `migrated`, because it continues per migrated workflow; marking it
      `migrated` to satisfy the gate would be a false claim.
- [x] 8.8 Run `make check-automation-docs` and, if the client changed,
      `make automation-client-self-test`.

## 9. Facades, matrix, and record completion

- [x] 9.1 Write each facade's module-doc narration **from the code**, not from the
      census trace, naming every inversion and the point where control resumes.
      Delegate every stage: a facade owns no timer, no admission bookkeeping, no
      generation counter, and no widget mutation. **Each facade carries a "State
      this workflow shares with others" table**, the form the load facade
      established — it is how a reader learns that the startup gate calls four
      workflows, that the file-row state snapshot is pushed down from the window,
      and that the sidebar's structure-changed signal drives the palette's file
      index, without opening those files. Populate each from task 2's decisions,
      and count the tables against the budget like any other narration. Name the
      two inversions that most need naming: the **cross-process** sidecar
      migration resumption and the **deferred expansion restore** that must read
      live state at apply time.
- [x] 9.2 **Measure each facade against 370 physical lines** and record both in
      `evidence/facade-measurements.md`, together with the task 2.6 decision and
      the measurements that produced it. If the tree facade does not fit after the
      full delegation sequence, escalate in-change with the measured count as task
      2.6 requires; do not edit the budget line quietly.
- [x] 9.3 **Protect the other facades' headroom.** `ui/search_panel/mod.rs` sits
      at 369 of 370: do not add a physical line to it. Re-measure the palette
      (335), save (223), load (271), and the four slot-4 facades (167, 165, 216,
      310) and confirm none is pushed over.
- [x] 9.4 **Run the rustdoc lint gate.** It is in neither `make check` nor
      `make pre-commit` nor `make check-policy`; CI's `Lint` job enforces it and
      slot 3a shipped this exact failure. New `pub` facades naming their own
      private coordination modules and `pub(crate)` seam types are precisely the
      `rustdoc::private_intra_doc_links` shape, and a **nested** role home makes
      the temptation worse. The command is in `.agents/rules/build.md`; the fix is
      always to drop the link and keep the name in backticks, never to widen
      visibility.
- [x] 9.5 **(tree half → 5b: only `WFR-NOTES-BOOKMARKS` gained a `Migrated Workflow Roles` subsection.)** Add a `### WFR-*` subsection under `Migrated Workflow Roles` for each
      migrated row — two, or three if task 2.6 split the tree row — naming its
      facade with its measured size, its coordination modules, its policy, its
      evidence, and its mutation-parity evidence pointer, plus the role home it
      chose (flat, per-workflow subdirectory, or nested). **Pointers in live
      `openspec/changes/migrate-workspace-tree-and-notes-workflow-readability/evidence/<file>.md`
      form**; an archive-prefixed pointer fails the gate immediately because the
      archive directory does not exist yet, and rewriting them is part of
      archiving.
- [x] 9.6 **(tree half → 5b: the `WFR-WORKSPACE-TREE` row is updated with what landed, not migrated.)** Update each row's `Current size`, **`Entry points`**,
      `Seams (i/c/a/p)`, `Seam value object`, `Evidence surface`,
      `Owned pure policy`, `Risk`, and `Status` cells from tasks 0.3 through 8,
      naming the pooled populations the old cells had shared and the rows that
      share them. `Entry points` is not optional: two consecutive slots found
      omissions there, and this row's cell must account for the startup gate, the
      window's scope-consumer refresh, the DnD drop, the context-menu routes into
      local history and notes, the close-time persistence flush, the **workspace
      scope filter change** (dropdown and window-driven), and **entering and
      leaving focused-folder mode** — the last two being the stage orders task 0.4
      added, whose entry points the census cell omits along with them.
- [x] 9.7 **(tree half → 5b: `WorkspaceWatchTicket` is still `required`.)** Update the `Seam Value Objects` section for every seam reified or
      re-audited — `NotesBrowserTicket` and `WorkspaceWatchTicket` move from
      `required` to `done`, and the scan-side and watch-target values are recorded
      as audited — and update the `Workflow Stage Traces` entries so each names
      the real stage-order and inversion counts from task 0.4 rather than the
      census floor. **The two floors this change corrects are the widest in the
      programme**; say so, with both numbers.
- [x] 9.8 **(tree half → 5b: neither workspace module relocated, so the census records them as still outstanding.)** Update the `Policy Module Census`: the two relocated workspace modules
      move out of the "Additional single-workflow modules" table with their
      outcomes recorded, `model/workspace.rs` and the five notes domain modules
      are confirmed as domain and staying with re-derived owning-workflow counts,
      and `model/sidecar_identity.rs` is recorded as the **cross-workflow kernel**
      task 3.7 found it to be, with its seven consuming workflows named. Leave
      pointers where a reader following the old snapshot would otherwise think a
      decision is still open.
- [x] 9.9 Advance `docs/next/workflow-readability.md`: flip slot 5's ledger line
      to `complete` with `WFR-AUTOMATION-SPINE (partial)`, add
      `WFR-AUTOMATION-SPINE` to slot 6's outstanding line and remaining-scope row,
      record the change name in the slot/name table, update the status paragraph
      and the remaining-scope table, and add a **"Baseline after slot 5"** table
      reporting workflows migrated, share of `ui/` + `model/` migrated with the
      corrected footprints, relocation candidates remaining, seams addressed,
      seams reified, long signatures shortened, automation projections, facade
      budget outcomes, role names and homes used, and every convention change.
      **The relocation-candidate line finally moves**: this is the first slot
      since 3a to relocate anything, and it relocates two.
- [x] 9.10 Add a **"Convention friction slot 5 hit, recorded for slots 6 and
      7"** section. Candidates already visible: the first nested role home; the
      first row whose facade had to be measured against a nine-stage-order
      narration, and whichever of delegate/escalate/split task 2.6 chose; the
      first evidence surface over a lazily materialized model and over a
      variable-sized child collection; whether the two topical decomposition
      requirements reconciled with the role contract as cleanly as expected;
      whether the census cells were wrong again and in which direction; the
      inversion floors being off by roughly six times on one row; the
      retroactive-amendment cost now standing at **ten** rows; how the
      `NoteSourceRefreshCoordinator` substitution across a migrated row was
      proved neutral; the verdict on each of the four data-safety candidates; and
      the `services/file_tree.rs` survivor triage outcome. Record the cost warning
      for slot 6 explicitly.
- [x] 9.11 Update `AGENTS.md`, `README.md`, `ui/sidebar/AGENTS.md`, and any
      `.agents/rules/*.md`, `.agents/skills/**`, or `docs/**` reference naming a
      moved path or a retired seam — `.agents/rules/ui.md`'s File Tree and
      Multi-Workspace Sidebar sections name several of these modules by path, and
      `docs/end-user-coverage.md` names the focused workspace-row-states lane.
      **Grep `scripts/accessibility_warning_allowlist.py` for every renamed module
      that logs**: it keys on module paths, so a rename silently turns an expected
      `tracing::error!` into an "unexpected warning". If a coupling exists, update
      it and re-verify it still **rejects** both an unrelated path and the stale
      module name so it has not become a blanket match. Confirm the same for
      `scripts/prepare-command-palette-notes-fixture.py` and
      `scripts/run-format-upgrade-manual-test.sh`, which are coupled to this
      family's fixtures and app-data layout.

## 10. Verification

- [x] 10.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries`, `check-automation-docs`,
      `check-accessibility-policy`, `check-visual-proof-policy`,
      `check-filesystem-boundary` — the last because this row mutates the user's
      own files and the notes row writes sidecars and app data — and
      `check-blueprint` if any template moved.
- [x] 10.2 The rustdoc lint gate from task 9.4, clean. Recorded as its own line
      because it is CI-only and has already been shipped broken once.
- [x] 10.3 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines**, and **no retry relied upon**. A recovered flake is a blocker under
      `.agents/rules/preexisting-blockers.md`: root-cause it, fix the cause, and
      rerun in isolation. Record before/after project test counts in
      `evidence/test-counts.md`; the count must not decrease.

      This change adds work to `crates/lushtext/tests/widget/workspace_section.rs`
      (5,846 lines, 123 tests) and `window.rs` (19,125 lines) — the two heaviest
      widget modules in the tree — so a load-amplified timeout there is the
      expected shape. Classify the wait first (synchronous UI flip versus async
      `spawn_blocking_then` or realization), then fix the cause per the
      `gtk-testing` skill's Flake Discipline: adequate budget for async waits,
      correct predicate, **shared** helper and never a private copy, and rerun in
      isolation to separate a real break from load. A load-amplified flake is
      still a real fragility.
- [x] 10.4 `make test-workspace-row-states` clean — the focused idempotent
      workspace file-row state lane, which exists precisely for this row's
      surface.
- [x] 10.5 `make mutants-diff` clean, with the four evidence files from tasks 3.8,
      4.10, 5.3, and 5.6 attached, every survivor accounted for, **relocation
      parity reported separately from gain-from-zero**, and the field-deletion
      floor stated. Confirm every new `policy.rs` is reachable by `examine_globs`
      and imports no GTK-family crate.
- [x] 10.6 **(tree half partial: the four landed tree fixes are proved in `evidence/tree-behavior-equivalence.md`; task 4.9's full state-extreme battery → 5b.)** The behavior-equivalence batteries from tasks 3.9 and 4.9, each case
      asserting the user-visible outcome and the resulting record or tree state.
- [x] 10.7 `make command-palette-notes-smoke` clean — the focused Notes lane with
      all note kinds, which asserts the Notes separators and representative rows
      through AT-SPI. Confirm `scripts/prepare-command-palette-notes-fixture.py`
      still passes unmodified; if it needs a change, that is a signal the fixture
      or sidecar layout moved, which the non-goals forbid — investigate rather
      than loosening the assertion.
- [x] 10.8 `make performance-smoke` clean. Its fixtures already cover malformed
      metadata, pending migrations, and duplicate sidecars, all of which this slot
      owns. Record the Criterion comparison **only if the machine is quiet**;
      slot 4's comparison was uninterpretable under a saturated CPU, and an
      uninterpretable number is worse than a deferred one.
- [x] 10.9 `make test-prop` if any property target is touched — it is gated
      behind `required-features = ["property-tests"]`, so no default lane runs it,
      and the palette property target constructs the notes source coordinator
      directly, which task 2.4 may change.
- [x] 10.10 The mandatory proof lanes for `ui/` and widget-test changes, each from
      a **clean artifact root**: `make visual-geometry-smoke`,
      `make accessibility-smoke`, `make visual-smoke`. Order these **after all
      source, documentation, and rules edits**: the accessibility policy gate
      fingerprints the *contents* of accessibility-relevant files, so any edit
      after a lane runs voids the proof and the lane must be rerun. The sidebar is
      accessibility- and geometry-dense — row names, descriptions, set positions,
      expanded state, context-menu keyboard parity, and the
      no-horizontal-scrollbar and header-visibility contracts — so consult
      `docs/accessibility-matrix.md` for the rows this change must cover and
      update them. If a lane fails wholesale, suspect a stale shared `target/`
      artifact before suspecting the change.
- [x] 10.11 `make builder-diagnostics-smoke` if any `.blp`/`.ui` template or
      template child moved, and `make check-blueprint` alongside it.
- [~] 10.12 **Live and manual proof — DEFERRED FOR USER AVAILABILITY, planned
      that way from the start.** Slot 4 established that isolating an app's state
      does not isolate its window: a real Wayland launch maps a surface and takes
      focus regardless of `XDG_*` isolation, and it interrupted the user's
      session. **Do not start a live launch to discharge this item.** Record the
      exact remaining scope in `evidence/live-run.md` and mark this task `[~]`:

      - `make run` against restored workspaces — expand and collapse a deep tree,
        drag to reorder folders, rename and delete a file, toggle the sidebar
        while it animates, and resize — watching stderr for
        `Trying to measure GtkBox ...`, pixman `Invalid rectangle`,
        `Gtk-CRITICAL`, `Gtk-WARNING`, and `GLib-GObject-WARNING`. The sidebar is
        the subtree `.agents/rules/widget-wiring.md` names explicitly as needing a
        real `make run` cycle with restored workspaces, so widget-green is
        necessary and not sufficient here.
      - `make run-format-upgrade-newer-manual-test` and
        `make run-format-upgrade-older-manual-test` — the future-version and
        upgradeable-old-version startup dialogs, whose grouped-row copy and
        default response are user-facing.
      - `make run-command-palette-notes-manual-test` — the Notes palette fixtures
        for manual review.

      Everything else display-dependent has a headless path: the smoke lanes run
      isolated `mutter --headless`, and `scripts/run-widget-tests.sh --headless`
      self-supervises into one. If a live drive is ever scheduled, use targeted
      AT-SPI rather than synthetic global input, which types into whatever the
      compositor focuses and is unverifiable.
- [x] 10.13 Cold-read check: with this change's conversation set aside, read each
      facade alone and confirm you can answer "what happens when the user expands
      a folder", "what happens when a file changes on disk", "what happens when
      the user renames a file that has a note", "what happens when the app starts
      with older app data", and "what happens when the user searches the notes
      browser and switches mode mid-search" without opening a coordination
      module. If any answer needs a second file, the facade is not narrating.
- [x] 10.14 `openspec validate migrate-workspace-tree-and-notes-workflow-readability
      --strict` passing.

## 11. Handoff

- [x] 11.1 Confirm the programme record and the matrix agree: every row this
      change migrated is `migrated` with a complete `Migrated Workflow Roles`
      subsection naming real paths, slot 5's ledger line is `complete`,
      `WFR-AUTOMATION-SPINE` is carried onto slot 6's outstanding line and
      remaining-scope row, and `make check-workflow-boundaries` passes. Report the
      count of pure mutation-scoped policy modules before and after. Record in
      B.1.
- [x] 11.2 Hand slot 6 (`WFR-MINIMAP`) and slot 7 (the residual sweep) the facts
      they need, in B.2: the named operations on this slot's facades that other
      workflows should call rather than reaching into; the facade-budget outcome
      and, if it moved, what that costs slot 6; the nested role home's first
      adopter and how it read; whether the evidence-surface materialization rule
      was needed anywhere else; the corrected per-row census method and every
      pooled population named with its sharing rows — including the palette/notes
      service split, so slot 7 does not re-derive it; the `WFR-SHELL-LAYOUT`
      decisions this change made about the sidebar animation blocker and the
      filter animation, since slot 7 owns that row; the
      `services/file_tree.rs` survivor outcomes; **the six production `.imp().`
      reach-throughs in `ui/automation.rs` this change deliberately left alone**,
      two `tab_view` reads at `:518`/`:519` for slot 7 and four editor/minimap
      reads at `:1137`/`:1144`/`:1162`/`:1224` for slot 6, each a projection
      decision for the row that owns it; the `snapshot-field-active-document-file-backed`
      dual-binding decision and whether `local_history` still derives it; the
      retroactive-amendment cost now standing at **ten** rows; and the reminder to run the rustdoc gate
      before shipping a facade. Confirm explicitly that slot 4's two `[~]` items
      and slot 4's three B.3 simplify candidates are **still slot 7's or still
      user-gated**, and were neither absorbed nor discharged here.

---

## Appendix A — orientation record

**Scope note that governs this whole appendix.** This change migrated
`WFR-NOTES-BOOKMARKS` and fixed seven confirmed pre-existing data-safety defects.
`WFR-WORKSPACE-TREE`'s structural migration moved to **slot 5b**; the reason is
recorded in `docs/next/workflow-readability.md`, "Why slot 5 split into 5a and
5b". Tree-row orientation work that was completed is recorded below and is
inherited by 5b as decisions rather than questions.

### A.1 Gate evidence (task 0.1)

Verified mechanically on a clean tree at `dfc42b6`:

- `openspec/changes/archive/` contains slots 1, 2a, 2b, 3a, 3b, and 4
  (`2026-08-25-normalize-workflow-readability-boundaries`,
  `2026-08-25-migrate-command-palette-workflow-readability`,
  `2026-08-25-complete-search-replace-workflow-readability`,
  `2026-08-26-migrate-document-save-workflow-readability`,
  `2026-08-26-migrate-document-load-workflow-readability`,
  `2026-08-27-migrate-user-content-restore-workflow-readability`).
- `openspec/specs/` holds all five consumed capabilities:
  `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
  `gtk-adapter-module-boundaries`, `mutation-testing`, `dbus-automation-spine`.
- All eight rows are `migrated` with complete `Migrated Workflow Roles`
  subsections naming existing paths; the ledger marked slots 1–4 complete and
  slot 5 outstanding.
- `make check-workflow-boundaries` passed, reporting **8** pure mutation-scoped
  policy modules.

Both of this slot's rows are `tier-3`, so the two-proof rule applies to each; it
is satisfied **eight rows over**.

### A.2 Premise re-verification, two rows (task 0.3)

Full record in `evidence/census-reverification.md`. Summary:

| Row | Census cell | Re-derived, row-scoped |
| --- | --- | --- |
| `WFR-NOTES-BOOKMARKS` | 22 files, 12,521 lines (ui 4,977 / model 770 / services 6,774) | **4 files, 4,365 production lines**. `services` pooled seven service files plus a separate-file test module; `ui` pooled `startup_data.rs` (435), which this change decided is **cross-cutting** |
| `WFR-WORKSPACE-TREE` | 28 files, 16,947 lines (ui 11,682 / model 1,368 / services 3,897) | **20 files, 11,214 production lines**. Authoring's 11,364/21 upper bound is confirmed exactly; the correction is excluding `file_tree_item.rs` (150), which the matrix lists under "Surfaces With No Coordination Tier" |
| notes seams | `2/4/4/0 = 10 fns, 16 sites, 2 override statics` | **7 fns / 15 gate sites**; the rest are service-side, shared with the palette row |
| tree seams | `24/7/29/5 = 65 fns, 116 sites` | **58 fns / 106 gate sites** across 10 files — the largest seam population in the programme. The census pooled `workspace_manager.rs` (3/9) and `workspace_watch.rs` (4/4) |

Pooled populations named with their sharing rows, including the
`services/palette/notes.rs` split re-derived by item span (**~180 browser-only,
~140 palette-only, ~1,840 shared**), which corrects authoring's ~130/~300/~1,700
guess in both directions. Test-only override storage: the notes side has module
statics; **the tree side has none at all** — its overrides are test-only *fields
on production state structs*, which no `static` grep finds.

### A.3 Shared-ownership and structural decisions (tasks 2.1–2.10)

Full record in `evidence/shared-ownership-decisions.md`. Outcomes:

| Task | Decision |
| --- | --- |
| 2.1 rename join | `migrate_note_sidecars_after_rename` stays a **named operation on the notes facade**, delegating to `notes/journal.rs`. The **ordering guarantee is the tree row's** |
| 2.2 `startup_data.rs` | **option (3), cross-cutting, owned by neither row.** Decided by owning workflows: `continue_startup_data_flow` calls into **five**, not the four the handoff anticipated |
| 2.3 `StartupDataFlowState` queue | recorded, routed to 7.1 |
| 2.4 `NoteSourceRefreshCoordinator` | **retired** onto `SingleFlightCoordinator` as three aliases; ~100 duplicated lines removed. One authoring claim was **stronger** than stated — the cancellation types were already the same type |
| 2.5 tree role home | **option (2), nested**: canonical `ui/sidebar/` plus coordination in `workspace_section/`. Every module classified; `tree_index.rs` and `watch_targets.rs` **dissolved** rather than renamed, which is what avoided an escalation for an `index` role |
| 2.6 one row or two | **one row, one facade, delegated hard**, projected at ≈351 of 370. No escalation, no split. **Not exercised** — see the scope note |
| 2.7 `journal` | fits **one** of four records (the migration ledger). The other three rejected with the reading stage named |
| 2.8 buried service policy | all ten services **stay**; `services -> ui` forbidden outright |
| 2.9 closed boundaries | confirmed, none re-opened |
| 2.10 excluded scope | recorded, including `WorkspaceSidebarWidthPreset` as `WFR-SHELL-LAYOUT`'s |

### A.4 Current ordered stages and real inversion counts, two workflows (task 0.4)

Full record in `evidence/stage-traces.md`, with every primitive attributed to
exactly one stage order so the subtotals sum.

- `WFR-WORKSPACE-TREE`: **11 stage orders, 27 deferral primitives** (11
  `spawn_blocking_then`, 8 `idle_add_local_once`, 4 `timeout_add_local(_once)`, 3
  `Debounce`, 1 `SupersedingTimer`) **plus 11 non-primitive callback
  resumptions** = 38 resumption points. Census floor: **5**. Off by ~7.6x.
  Authoring's unreconciled per-stage subtotals summed to 32 against a stated 27;
  the reconciliation reassigned `tree_loading.rs:143` from the scan order to the
  DnD shield and named `folders.rs:471` once as shared.
- `WFR-NOTES-BOOKMARKS`: **5 stage orders, 21 primitives plus 7 callback
  resumptions** = 28. Census floor: **4**. Off by 7x.
- The two stage orders authoring missed on the tree row (**workspace scope
  filter**, **focused-folder drilldown**) are both confirmed in the code, and the
  filter's `SupersedingTimer` was already inside the counted 27 — so the
  arithmetic looked consistent while the narration was missing.
- The two inversions named in facade narration rather than counted: the
  **cross-process** sidecar migration resumption, and the **deferred expansion
  restore** that must read live state at apply time.

### A.5 Contracts as implemented today (task 0.5)

Full record with verbatim quotes in `evidence/durability-contracts.md`: the
sidecar migration order and its retry ledger; the format-upgrade apply's
worker-side re-scan; workspace persistence's latest-generation semantics; the
five `ui/sidebar/AGENTS.md` local contracts; expansion-state authority; the
file-operation semantics; the `GtkTreeExpander` internal-gesture disable at
`row_factory.rs:325-343` (unchanged, byte-identical, and the module is classified
as a called presentation surface **precisely so** no role move touches it); and
the peek key controller's Capture phase plus its `focus_allows_peek_shortcuts()`
gate (unchanged).

### A.6 Amendment basis and the eight-row retroactive re-check (tasks 1.1, 1.3)

Basis confirmed from the code and the specs before amending:

- **(a)** both topical requirements mandate module *topics*; the role requirement
  mandates one *role*; neither `bookmarks`, `editors`, `browser`, `row_factory`,
  `context_menus`, nor `row_accessibility` is a bounded role name.
- **(b)** the taxonomy sentence enumerates exactly five names, and a grep of
  `openspec/specs/` and `.agents/rules/` finds **no** text treating a
  presentation or called surface as a role — while slot 4 shipped exactly such a
  module.
- **(c)** the two permitted role homes are described per directory; neither covers
  a workflow owning a directory **and a widget subdirectory of it**.
  `ui/**/policy.rs` reaches both candidate locations.
- **evidence surface**: all five code facts confirmed from the code, not from
  reasoning. Recorded in `evidence/evidence-surface-materialization.md`.

Both spec deltas were verified to be **pure additions** — a per-requirement diff
against the live specs shows zero removed non-blank lines.

The eight-row re-check is recorded as `### Slot 5a amendment re-check` in
`docs/workflow-readability-matrix.md`. **Statement (b) found six of eight rows
non-compliant**, all recording widget-projection modules under the
undefined label "adapter detail"; the gap is filled in this change in both
required places, and the label is retired repository-wide.

### A.7 Widget-test reach-through sites, in-scope and out-of-scope (tasks 0.7, 4.8, 6.3)

Full record in `evidence/widget-test-reach-through-migration.md`. Authoring's 190
ungated `.imp().` sites confirmed exactly (32 / 158 / 0 / plus the notes-side
window subset). **113 of the 158 are `TemplateChild` widget handles and are out of
scope**, with the reason recorded; the 45 in-scope tree-side runtime reads move to
5b. Notes-side: 4 typed inspection surfaces retired to zero.

`ui/automation.rs` holds exactly **8** production `.imp().` sites. Two are this
slot's rows' and are **left for 5b** (they need that row's evidence surface to
project from); six belong to `WFR-SHELL-LAYOUT` (`:518`, `:519`) and `WFR-MINIMAP`
(`:1137`, `:1144`, `:1162`, `:1224`) and were deliberately not touched. A sweep
found no other cross-workflow-boundary reach in `ui/`.

### A.8 Coordination role mapping per stage order, and the `journal` verdicts (tasks 2.7, 3.1, 4.1)

Notes row, as implemented:

| Stage order | Role module |
| --- | --- |
| Notes browser source build (browser **and** command palette) | `source_execution.rs` — `execution`, stage-order-qualified |
| Notes browser query | `query_execution.rs` — `execution`, stage-order-qualified |
| Bookmark lifecycle + editor note resolution | `bookmark_execution.rs` — `execution` |
| Document and folder note editors | `editor_execution.rs` — `execution` |
| Sidecar migration on rename | `journal.rs` — **`journal`** |

The closed-file bookmark-excerpt preview was decided **part of the bookmark
lifecycle**, not a fifth coordination job: it is the same coordinator family, the
same row identity revalidation, and a reader looking for "what happens when I
select a bookmark row" looks under bookmarks.

`journal` verdicts, per record: migration ledger **yes**; note and bookmark
sidecars **no** (authoritative user content, no generation, no stale-record
cleanup; read back by the browser source build and `resolve_notes_for_editor`);
format-upgrade backup **no** (read back only by manual recovery, and not this
row's); `workspaces.json` **no** (loaded at next launch like any settings file;
named `persist_execution.rs` in the 5b plan).

Tree row role mapping is recorded in `evidence/shared-ownership-decisions.md`
§2.5 and is inherited by 5b.

### A.9 Data-safety passes and the four candidates (tasks 0.6, 7.1–7.5)

Full record in `evidence/data-safety.md`. **Eleven findings from one explicit
pass; no domain came back clean.** Seven fixed here, each with a regression test
**proved to fail without its fix**; four recorded with severity, site, and owning
row.

The four named candidates: 7.1 **not a data-safety defect** (bounded-work gap,
handed to slot 7 with the drain's re-entry correctness proved); 7.2 **not a
defect** on all three questions, with three adjacent `format_upgrade` gaps
recorded; 7.3 **confirmed defect, fixed**; 7.4 three separate answers — sidecars
**cannot** be lost by the single-rename path, the watch target **could** be
stranded (confirmed, fixed), and the expansion set's transient staleness is **not**
data loss.

### A.10 Automation no-widening proof and the two ownership decisions (tasks 8.3, 8.6)

Full record in `evidence/automation-no-widening.md`, including exactly what was
**not** run (the two-tree `make automation-smoke` capture-and-diff) and why the
gap is narrow for this row.

8.3's two ownership questions: `workspace-sidebar-animation` **follows the
animation, not the row name** — it is fed by the window's sidebar transition
settle, so it is `WFR-SHELL-LAYOUT`'s (slot 7), which is the reusable form of slot
3a's "a field whose name contains *save* is not thereby save-workflow state". The
palette's `command-palette-index` disjunct **stays a direct call** to
`command_palette_note_refreshes.has_work()`: routing it through the notes evidence
surface would make a readiness poll build a whole surface, and the fact is already
reached exactly one way.

Both **absences** recorded so they are not read as gaps: the browser dialog's
coordinators have no readiness blocker, and the startup format-upgrade flow has
none.

### A.11 Role-home collision analysis and the one-row-versus-split decision (tasks 2.5, 2.6)

In `evidence/shared-ownership-decisions.md` §2.5–2.6 and
`evidence/facade-measurements.md`. Recorded for 5b, not exercised here.

### A.12 Facade measurements (task 9.2)

`evidence/facade-measurements.md`. Notes facade **178 of 370**. Three of eight
previously recorded facade sizes were stale and are corrected.

### A.13 Lane consequences of the module renames (tasks 9.11, 10.10)

- `scripts/accessibility_warning_allowlist.py` keys on exactly **one** module
  path, `editor_page::load::execution`. **No coupling to this slot's modules
  exists**, so task 9.11's conditional confirmation holds and no update was
  needed. Re-verified by reading the file, not assumed from the proposal.
- `scripts/prepare-command-palette-notes-fixture.py` keys on the **app-data
  directory layout** (`bookmarks/`, `folder-notes/`, `document-notes/`), not on
  module paths, and this change did not touch that layout. Confirmed unmodified.
- `scripts/run-format-upgrade-manual-test.sh` targets `startup_data.rs`, which is
  unmodified.
- `docs/end-user-coverage.md` names `make test-workspace-row-states`, the tree
  row's focused lane. Unchanged, because the tree row's surface is unchanged.

### A.14 Cold-read result, five questions (task 10.13)

Answered from `ui/window/notes/mod.rs` alone, without opening a coordination
module, for the three questions that are this row's:

- *"what happens when the user renames a file that has a note"* — the migration
  stage order names it in three steps and states that the ordering guarantee
  belongs to the tree row, that pending ledger state is recorded **before** the
  first move, and that control can resume on a later app launch. **Yes.**
- *"what happens when the user searches the notes browser and switches mode
  mid-search"* — the browser stage order names `begin_mode` advancing the source
  and query generations, and the inversions list names both worker completions and
  the ticket that validates them. **Yes.**
- *"what happens when the app starts with older app data"* — the facade
  **deliberately does not answer this**, and the shared-state table says why:
  the startup gate is cross-cutting `ui/window/startup_data.rs`, owned by neither
  row, ordering five workflows. That is the correct answer rather than a gap.

The two tree-row questions (*"what happens when the user expands a folder"*,
*"what happens when a file changes on disk"*) **cannot** be answered from
`ui/sidebar/mod.rs`, which is still the pre-migration widget wrapper. That is the
honest state of the unmigrated row.

### A.15 Tail simplify pass, after full verification

Applied during implementation rather than as a separate pass, and recorded here:

- **Eight duplicated inline `match mode` arms** across `browser.rs` collapsed
  into five new `NotesBrowserModeExt` methods (`source_limit_status_message`,
  `source_recovery_status_message`, `source_failure_message`,
  `open_action_label`, `unselected_value_text`). Three of the eight were literal
  duplicates of each other — the same two strings matched twice in adjacent
  functions.
- **Two tuple-returning test seams collapsed into one** named operation returning
  a named value, removing a 4-tuple whose fields were positional.
- **~100 lines of duplicated single-flight logic** removed by the coordinator
  retirement.
- `is_current` on the notes ticket renamed **`may_publish`**, because Clippy's
  `wrong_self_convention` was right for the wrong reason: the method is not
  asking "is this ticket current", it is deciding whether a completion may
  publish, and the new name says which.

## Appendix B — handoff

### B.1 Programme and matrix agreement (task 11.1)

- `WFR-NOTES-BOOKMARKS` is `migrated` in `docs/workflow-readability-matrix.md`
  with a complete `Migrated Workflow Roles` subsection naming paths that exist.
- `WFR-WORKSPACE-TREE` remains `pending`, and the ledger's **slot 5b** line lists
  it as outstanding. The matrix and the record agree, which
  `make check-workflow-boundaries` verifies.
- Slot 5a's ledger line is `complete` with `WFR-AUTOMATION-SPINE (partial)`;
  `WFR-AUTOMATION-SPINE` is carried onto slot 5b's outstanding line and remains
  `pending` in the matrix rather than `migrated`, because it continues per
  migrated workflow.
- **Pure mutation-scoped policy modules: 8 before, 10 after**
  (`ui/window/notes/policy.rs` and `ui/sidebar/policy.rs`).

### B.2 To slots 5b, 6 and 7 (task 11.2)

**Named operations on this slot's facades, to call rather than reach into:**
`migrate_note_sidecars_after_rename`, `reconcile_pending_migrations_on_startup`,
`resolve_notes_for_editor`, `resolve_notes_for_open_editors`,
`reset_notes_after_save_as`, `wire_note_callbacks`, `refresh_notes_menu_state`,
`refresh_command_palette_note_source_debounced`, `flush_bookmarks_for_editor`,
`flush_all_pending_bookmarks`, and `notes_evidence()`.

**Facade budget:** held at 178 of 370; the line was **not** edited and no
escalation was proposed. Five stage orders fit comfortably, which is evidence that
**stage-order count alone is not what stresses the budget** — the exemplar's 369
comes from twelve prose inversions plus a large value-type surface in the same
file. Slot 5b's eleven-stage-order projection of ≈351 stands recorded.

**The nested role home has no adopter yet.** Slot 5b is its first; the collision
analysis, the module-by-module classification, and the two dissolutions
(`tree_index.rs`, `watch_targets.rs`) are already decided in
`evidence/shared-ownership-decisions.md` §2.5.

**Whether the evidence-surface materialization rule was needed elsewhere:** no.
The eight-row re-check confirms only `GtkTreeListModel` creates children on
demand, so `WFR-WORKSPACE-TREE` is the only row the rule bites. Its five offending
code facts are verified and recorded in
`evidence/evidence-surface-materialization.md` so 5b inherits them.

**Corrected per-row census method and every pooled population**, including the
`services/palette/notes.rs` split (**~180 browser-only / ~140 palette-only /
~1,840 shared**) so slot 7 does not re-derive it.

**`WFR-SHELL-LAYOUT` decisions this change made** (slot 7 owns that row):
`workspace-sidebar-animation` follows the animation, not the row name;
`WorkspaceSidebarWidthPreset` is `workspace-sidebar-width-policy`'s value and
should move out of `ui/sidebar/mod.rs` when 5b writes that facade; and the
`recent_documents.loading` ungated test read slot 3b left is still open.

**The six production `.imp().` reach-throughs in `ui/automation.rs` this change
deliberately left alone**: `:518` and `:519` (`window.imp().tab_view`) for slot 7,
and `:1137`, `:1144`, `:1162`, `:1224` (editor/minimap) for slot 6. Each is a
projection decision for the row that owns it. **Plus two more**, `:766` and
`:927`, which are `WFR-WORKSPACE-TREE`'s and go to **5b** with the row.

**`services/file_tree.rs`'s 11 pre-existing surviving field-deletion mutants are
not triaged and go to 5b** with the row that owns them. They are baseline, not
regressions.

**The `snapshot-field-active-document-file-backed` dual binding is resolved by
neither object projecting it**, and `local_history` no longer *derives* it
independently — both objects call one shared helper. The rule is recorded above
the Evidence Projection Map.

**The retroactive-amendment cost now stands at nine per-row re-checks**, and the
not-a-confirmation streak is **three consecutive amendments**. Slot 6's minimap
amendment should budget for real work.

**Run the rustdoc gate before shipping a facade.** It is CI-only, in neither
`make check` nor `make pre-commit` nor `make check-policy`.

**Confirmed still outstanding elsewhere, neither absorbed nor discharged here:**
slot 4's two `[~]` items (the live-session `make run` paned proof and the
quiet-machine `bench-compare`) remain slot 4's and user-availability-gated; slot
4's three B.3 simplify candidates (`drafts/journal.rs`,
`local_history/preview_execution.rs`, and the `current_window_width` duplication
in `ui/window/imp.rs`) remain **slot 7's** — task 0.10 verified none has migrated
into this slot's files.

**Four data-safety findings are recorded and unfixed**, with owners: H-7 (external
rename strands sidecars — a **product decision** about what a rename means), M-3
(premature teardown survives a refused close — slot 7), M-5/M-6/M-7
(`format_upgrade` gaps), M-8 (`recovery_metadata` treats a transient read failure
as corruption), M-9 (ledger retry route), M-10, M-11. See
`evidence/data-safety.md` for severity, site, and a concrete fix for each.
