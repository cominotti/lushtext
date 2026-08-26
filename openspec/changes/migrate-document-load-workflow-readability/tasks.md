## 0. Gates, orientation, and premise re-verification

- [x] 0.1 **Slot-3a gate — blocking.** This change may not begin until 3a is
      archived. Confirm mechanically: `openspec/changes/archive/*-migrate-document-save-workflow-readability/`
      exists with its `gtk-adapter-module-boundaries` delta merged into
      `openspec/specs/`; `docs/workflow-readability-matrix.md` marks
      `WFR-DOCUMENT-SAVE` `migrated` with a complete `Migrated Workflow Roles`
      subsection naming its per-workflow role home;
      `docs/next/workflow-readability.md`'s ledger marks slots 1, 2a, 2b, and 3a
      `complete` and slot 3b `outstanding`; and `make check-workflow-boundaries`
      passes on a clean tree. Three dependencies make this blocking rather than
      polite: the two changes share `ui/editor_page/load_save.rs`, 3a's delta
      establishes the per-workflow role home this change reuses, and 3a's task 3.2
      hands over the shared-field ownership decisions this change's load side
      consumes. Also confirm the slot-2a deliverables are present: the declared
      facade budget line, the stage-order qualification rule, and the working
      evidence-to-snapshot drift check in `scripts/check-automation-docs.py`.
- [x] 0.2 Read `docs/next/workflow-readability.md` end to end, including the
      "Baseline after slot 3a" table and the "Convention friction slot 3a hit"
      section — that section is written for this change specifically. Read
      `docs/workflow-readability-matrix.md` rows `WFR-DOCUMENT-LOAD`,
      `WFR-DOCUMENT-SAVE`, `WFR-AUTOMATION-SPINE`, `WFR-SESSION-RESTORE`,
      `WFR-DRAFT-RECOVERY`, `WFR-BUFFER-REPLACEMENT`, `WFR-BUFFER-SNAPSHOT`,
      `WFR-PLAIN-DISPOSAL`, `WFR-EDITOR-MEMORY`, and `WFR-SHELL-LAYOUT`, plus the
      `Measurement Definitions`, `Policy Module Census`, `Test Seam Census`,
      `Seam Value Objects`, `Settled Conventions`, `Facade size budget`,
      `Migrated Workflow Roles`, and `Completion Rule` sections. Read all five
      capability specs. Read 3a's handoff (its task 12.2 output).
- [x] 0.3 **Premise re-verification — before any code.** Every slot so far found
      stale matrix facts, and this row carries the largest known undercount.
      Re-derive each figure below and record it in Appendix A.2, including
      "unchanged" where it is unchanged. Correct the matrix in task 8 and work
      from the corrected numbers, not from this file.
      - The residual shape of `load_save.rs` after 3a: line count, which functions
        remain, and whether 3a left any save-side residue behind (it should not
        have, but check rather than assume). **Take the line count from 3a's task
        12.2 handoff or by measuring the post-3a file — not from either authoring
        estimate.** Authoring produced two (≈1,046 from this side, ≈1,088 from
        3a's) because the halves interleave rather than occupying clean ranges;
        neither is authoritative and the post-3a measurement settles it.
        Confirm too that `ViewInteractivityState` and
        `set_file_path_for_pending_load` are among the residue, since 3a assigns
        both to this change.
      - `model/file_load.rs`'s full reference set. The census cell says **4
        consumers**. The verified set is **6 production files** — `model/mod.rs`
        (the module declaration), `model/save_admission.rs`,
        `services/editor_io.rs`, `ui/plain_disposal.rs`,
        `ui/editor_page/load_save.rs`, `ui/editor_page/load_runtime.rs` — plus 3
        test/bench files. Re-confirm it, and in particular re-confirm the
        `services/editor_io.rs` reference, because that single consumer is what
        decides task 2.
        **Six grep false-positive families to avoid re-deriving.** An earlier
        authoring pass reported nine `ui/` consumers; six were name collisions
        with unrelated symbols, not references to this module. Record these so no
        future session repeats the overcount: `file_load_active` in
        `ui/automation.rs`; `connect_file_loaded` in `ui/window/notes/mod.rs` and
        `ui/window/focus_indexing.rs`; `file_loaded_callbacks` in
        `ui/editor_page/imp.rs` and `ui/editor_page/mod.rs`; and a test function
        name in `ui/window/drafts.rs`. Match on the import or path, not on the
        substring `file_load`.
      - The row's `Current size` cell (`10 files, 5,301 lines (ui 3,265 / model
        661 / services 1,375)`) against the post-3a tree.
      - The row's `Seams (i/c/a/p)` cell (`23/7/3/1 = 34 fns, 55 sites, 3 override
        statics`). Authoring counted, load-relevant: roughly 11 of
        `load_save.rs`'s 18 `_for_test` functions on the load side,
        `load_runtime.rs` 5 with 13 gate sites and two probe statics beyond its
        coordinator, `services/editor_io.rs` 14 functions and 22 gate sites mixed
        save/load with seven override statics, `ui/window/dialogs.rs` 7 including
        three chooser-bound open seams, `ui/window/documents.rs` 3. Some of that
        is shared with `WFR-DOCUMENT-SAVE` (3a, now migrated) and
        `WFR-DRAFT-RECOVERY` (slot 4). Produce a **row-scoped** count naming the
        shared population, as slot 2a had to for the palette.
      - The four inversions the stage trace records. Slot 2a's finding stands:
        **census inversion counts are floors, not totals.** Narrate from the code.
- [x] 0.4 **Read the code before changing it** and write the current ordered stages
      into Appendix A.4: the load half of `load_save.rs` — including the chunked
      install slicing free functions (`schedule_install_slice`,
      `run_install_slice`, `delete_buffer_slice`, `run_existing_clear_slice`,
      `run_text_insert_slice`, `finish_chunked_install`, `abort_chunked_install`,
      `run_cancelled_clear_slice`, `clear_installation_owner` and the
      `ChunkedLoadInstall` type) — `load_runtime.rs`, `model/file_load.rs`, the
      load and decode paths in `services/editor_io.rs`, and the window-side
      invocations in `ui/window/documents.rs`, `dialogs.rs` (open chooser),
      `encoding.rs` (reopen with encoding), `recent_open.rs`, and
      `session_restore.rs`. Name **every** inversion and its resumption point,
      including the per-slice resumption and the abort/cancel resumptions the
      census trace does not enumerate.
- [x] 0.5 **Record the paragraph-boundary contract before touching the slicing.**
      `.agents/rules/rust.md` states it normatively: bounded install and clear
      slices must end on paragraph boundaries, because GTK validates whole
      paragraphs and a slice stopping mid-paragraph re-lays-out everything already
      installed in that paragraph on every later slice — the quadratic behavior
      that once froze crash recovery of a 33 MB single-line draft for minutes. A
      paragraph larger than the slice budget installs or clears in one turn. Quote
      the current implementation of that rule in Appendix A.5 and state which
      module owns it after the split. **This is a performance contract with a
      user-visible failure mode; it is not refactorable detail.**
- [x] 0.6 Invoke the `data-safety` skill in explicit mode over the intended diff
      surface before writing code, and again in task 9 over the actual diff.
      Record both in Appendix A.9. Slot 2b's readability pass over a durable path
      produced two confirmed pre-existing data-loss findings and
      `.agents/rules/preexisting-blockers.md` made fixing them non-negotiable
      despite that change's "no behavior change" non-goal. Load installs content
      into a live buffer under cancellation, so the same expectation applies:
      **budget for findings and do not treat the non-goal as permission to defer
      one.**
- [x] 0.7 Grep this workflow's tests for `\.imp()\.` reach-through, not only for
      `_for_test`. Authoring found `crates/lushtext/tests/widget/editor_page.rs`
      writing `page.imp().load_state.set(EditorLoadState::…)` directly at several
      sites and `crates/lushtext/tests/widget/open_popover.rs` reading
      `recent_documents.loading`. These are ungated, appear in no seam census, and
      shape production field layout. Produce the full site list with a per-site
      categorization (inspection → migrates to evidence; actuation → classified
      and preserved) in Appendix A.7.

## 1. Promote the evidence-surface reentrancy constraint

- [x] 1.1 Confirm the constraint's basis from the code before amending anything,
      and record it in Appendix A.6: that one accessor reads the whole surface,
      that the exemplar's `evidence.rs` module doc already records the shared-borrow
      constraint, and that slot 2b obeyed it when adding ten fields and recorded
      that it should become stated convention rather than a per-workflow note.
- [x] 1.2 Apply this change's `workflow-evidence-surfaces` delta: state the
      constraint normatively and require the proof test as the pattern. **Get the
      test's shape right** — it is easy to state it backwards, and stating it
      backwards mandates the exact panicking read the constraint forbids. The test
      **drives the workflow through each operation that takes a mutable borrow of
      the state the accessor reads, reads the evidence surface *after* each such
      operation, and asserts that repeated reads of unchanged state are
      identical.** It does not read the surface *while* a mutable borrow is held.
      Slot 2b's test (`crates/lushtext/tests/widget/search_panel.rs:3437-3457`) is
      the reference implementation; read it before writing this workflow's. That
      is the delta's whole content — do not touch the visibility rule, the seam
      taxonomy, the projection requirement, or the facade budget.
- [x] 1.3 **Retroactive-amendment obligation, and expect this one to be real
      work.** The amended requirement's content is a proof obligation, so a
      confirmation is not enough: re-check every row the matrix marks `migrated`
      and, where the proof test is missing **or has the wrong shape**, write or
      fix it in this change. At this point that is `WFR-SEARCH-REPLACE` (slot 2b
      wrote the reference test at
      `crates/lushtext/tests/widget/search_panel.rs:3437-3457` — verify it still
      covers every operation taking a mutable borrow), `WFR-COMMAND-PALETTE`, and
      `WFR-DOCUMENT-SAVE` (3a's task 7.3 should have written one — verify both its
      existence and that it reads *after* each mutating operation rather than
      during one). Record
      the verdict **per row** in the matrix's amendment section, following slot
      2b's format, and state for each whether the test existed, was extended, or
      was written here. Leaving a migrated row without the proof means two
      generations of the convention coexist, which the rule forbids.
- [x] 1.4 Re-confirm in the same section that the other settled conventions are
      unchanged by this amendment: the facade budget stays 370 with every migrated
      facade measured against it, the bounded coordination role set is unchanged,
      the seam value-object shape is unchanged, the evidence-surface visibility
      rule is unchanged, and 3a's per-workflow role home is unchanged.
- [x] 1.5 Update standing guidance so the constraint is discoverable where a
      developer will hit it: the evidence-surface section of
      `.agents/rules/widget-wiring.md`, `.agents/rules/rust.md` if it enumerates
      evidence-surface rules, the matrix's `Settled Conventions`, and any
      `.agents/skills/**` reference describing evidence surfaces. Run
      `make check-agent-docs` and `make check-agent-skills`.

## 2. Decide `model/file_load.rs` — the decision the census deferred here

- [x] 2.1 Re-derive the full reference set per task 0.3 rather than trusting the
      census cell's "4 consumers". Group consumers by layer and name what each
      uses, in the table form slot 2b used for `model/workspace_search.rs`.
- [x] 2.2 Record the decision: **it stays in `model/`.** `services/editor_io.rs`
      depends on it, so relocating it under `ui/` would invert dependency direction
      (`services -> ui`), which the convention forbids outright. It is also not
      single-workflow policy: its three `ui/` consumers span **two** workflows —
      `ui/plain_disposal.rs` belongs to `WFR-PLAIN-DISPOSAL` (cross-cutting, slot
      7) and the two `ui/editor_page/` files to `WFR-DOCUMENT-LOAD` — and
      cross-cutting eligibility counts **owning workflows**, not consuming files,
      so two owners clears the bar on its own. It is already pure, already
      mutation-scoped through `model/**`, and
      already carries co-located unit tests, so the move would trade a
      dependency-direction violation for nothing. If the re-derived reference set
      contradicts this — for instance if the `services/` reference turns out to be
      a re-export rather than a dependency — record the actual finding and decide
      from it, but do not relocate without proving no service or `model/` sibling
      depends on it.
- [x] 2.3 Correct the matrix: move the row out of "Additional single-workflow
      modules the census found" / the deferred-decision note into "Modules
      confirmed as domain and staying in `model/`", with the corrected consumer
      list and the dependency-direction reason. Keep a short pointer at the old
      location so a reader following the census snapshot does not conclude the
      decision is still open. **This decision is closed; state that a later slot
      must not re-open it**, as slot 2b did for `workspace_search.rs`.
- [x] 2.4 Confirm no `.agents/rules/*.md`, skill, or doc asserts this module is
      pending relocation, and fix any that does. Also update the census's
      post-relocation arithmetic if it counts `file_load.rs` as a pending mover.

## 3. Extract the load workflow into role-named modules

- [x] 3.1 From the code, record the load workflow's cohesive coordination jobs and
      map each to a bounded role name (`admission`, `execution`, `retirement`,
      `watch`, `journal`). Authoring's expectation, to be confirmed or corrected:
      `load_runtime.rs`'s coordinator, admission drain, and disposal-wakeup is an
      **admission** job; the dispatch, worker read/decode completion, and bounded
      install slicing is an **execution** job; the cancellation and abort paths
      that dispose the decoded payload off-GTK may be a **retirement** job, since
      that name means destroying a payload the workflow is finished with — but
      check whether it is cohesive enough to be its own module or belongs inside
      execution. Check `journal` and record why it does not fit: load keeps no
      durable record a later stage reads back. If a genuinely novel job appears,
      **stop and escalate**: adding a role name is a spec amendment costing a
      re-check of four migrated rows, and this change already carries one
      amendment.
- [x] 3.2 If two coordination modules of the same shape are needed, apply slot
      2a's stage-order qualification rule and slot 2b's narrow reading of it:
      qualify only the **new** module whose fitting name is already spent, and do
      not rename stable siblings for symmetry.
- [x] 3.3 Create `ui/editor_page/load/` using the per-workflow role home 3a's
      delta permits: facade `mod.rs`, the coordination modules, `policy.rs`,
      `evidence.rs`, and a `test_policy.rs` entirely behind
      `#[cfg(feature = "test-utils")]`. Declare it in `ui/editor_page/mod.rs`.
      Retire `load_runtime.rs` — `runtime` is the name the convention rejects.
      **Remove `load_save.rs`**: after this change the file the programme cites as
      "1,795 lines holding two workflows" no longer exists, which is worth stating
      in the record.
- [x] 3.4 Move the install slicing into coordination as a whole, keeping the
      paragraph-boundary rule from task 0.5 intact and its pure arithmetic in
      `policy.rs`. The 314 lines of slicing free functions are stage **body**, not
      narration — slot 2a's finding that facade length comes from bodies rather
      than stages is the reason this belongs in coordination and the facade
      narrates it in one stage with its per-slice resumption named.
- [x] 3.5 Take ownership of the load side of the shared fields 3a recorded: the
      load cancellation the save path triggers, `size_check` ("size classification
      from the last file load", read by the save path), `ViewInteractivityState`
      (which sat in 3a's save cluster while being a field of the load-side
      `LoadInstallationState`, so it collects here), and
      `set_file_path_for_pending_load`. Each needs one owner here and, where the
      save side still reads it, a named crossing operation rather than shared
      mutable state — the shape slot 2b used for the three fields its two Replace
      All modules shared. Record the owner and the crossing operation per field in
      Appendix A.10.
      **The restore-position group is the exception and MUST NOT move into
      `ui/editor_page/load/`.** `set_restore_position` / `apply_restore_position`
      and their cursor and top-line fields are neither save nor load state: beyond
      both workflows they are owned by `ui/window/session_persistence.rs`
      (`WFR-SESSION-RESTORE`, slot 4), `ui/window/search.rs` (`WFR-EDITOR-FIND`,
      slot 7), and `ui/window/notes/bookmarks.rs` plus
      `ui/editor_page/bookmarks.rs` (`WFR-NOTES-BOOKMARKS`, slot 5). Cross-cutting
      eligibility counts **owning workflows**, and there are five, so the group
      stays in a shared `ui/editor_page/` location with its ownership recorded,
      and the load workflow reaches it through a named operation — exactly as 3a
      left it. Add it to task 3.7's boundary list.
- [x] 3.6 Make the window side delegate, following slot 2b's window-side fix. The
      open, reopen-with-encoding, recent-document, sidebar-activation, and
      session-restore invocations in `ui/window/documents.rs`, `dialogs.rs`,
      `encoding.rs`, `recent_open.rs`, and `session_restore.rs` must call one named
      load operation per step instead of re-reading and re-mutating editor load
      state inline. Keep the generation and cancellation guards on the editor side
      where they already live. Record every site that changes and whether a guard
      moved (it should not).
- [x] 3.7 Confirm the boundaries this change must not cross, and record them:
      `model/buffer_replacement.rs` (`WFR-BUFFER-REPLACEMENT`, cross-cutting, slot
      4), `ui/buffer_snapshot` (`WFR-BUFFER-SNAPSHOT`, cross-cutting, slot 7),
      `ui/plain_disposal` and `model/plain_disposal.rs` (`WFR-PLAIN-DISPOSAL`,
      cross-cutting, slot 7), `model/editor_memory.rs` (exempt, no slot),
      `services/draft_service.rs` and `ui/window/drafts.rs`
      (`WFR-DRAFT-RECOVERY`, slot 4), `ui/window/session_persistence.rs`
      (`WFR-SESSION-RESTORE`, slot 4), the save workflow 3a migrated, and **the
      restore-position group from task 3.5**, whose five owning workflows make it
      cross-cutting editor-page state rather than load state. Load **calls** these;
      it does not own or restructure them.
- [x] 3.8 Confirm the extraction is behavior-neutral at the API level: no changed
      call order, no new or removed `spawn_blocking_then` boundary, no changed
      slice budget or timer interval, no changed error surface. Where a rename
      lands on a cross-module operation, name it for the workflow intent per the
      intent-first naming rule, and record the old → new mapping so reviewers can
      diff behavior rather than names.

## 4. Reify the freshness seam

- [x] 4.1 Introduce `LoadRequestTicket` carrying `{load_generation, cancel_token}`
      with an `is_current(&editor)` predicate matching `SaveCompletionTicket`'s
      shape, per the matrix's Seam Value Objects section. The pair is already
      grouped inside `load_runtime`'s request type and then exploded back into
      loose parameters at both dispatch sites and compared clause-by-clause at the
      completion; after this task it is constructed **once** at the workflow entry
      point and validated as a unit.
- [x] 4.2 Decide whether the ticket needs a `*Facts` companion or the
      `is_current(&editor)` variant is right, and record the reason. The matrix
      prescribes the `&editor` variant; confirm from the code that every clause the
      completion compares is live editor state rather than dispatch-time
      expectation. If any clause is dispatch-time expectation, it belongs on the
      ticket and the shape may need `Facts` — say so rather than forcing the
      prescribed shape onto a mismatched seam.
- [x] 4.3 Check for a second unreified bundle while you are here: the install path
      threads installation state, phase, and abort disposition across several
      functions. The seam rule triggers on a bundle crossing **two or more**
      function boundaries or reconstructed at two or more call sites; a bundle used
      by exactly one private helper does not qualify. Apply the rule rather than
      reifying everything, and record which bundles were considered and rejected.
- [x] 4.4 Confirm no `#[expect(clippy::too_many_arguments)]` is introduced anywhere
      in this workflow, and that the workspace count (1 after 3a, the domain
      catalog constructor) does not rise. Treat such a suppression on a
      cross-module workflow boundary as an unreified seam, not an accepted
      exception.
- [x] 4.5 Report **seams reified** as this change's primary unit, per the record's
      instruction, and report long signatures only as a secondary figure stating
      which definition it uses (receiver-counted 88 or strict 43).

## 5. Pure policy and the open-popover snapshot decision

- [x] 5.1 Extract the load workflow's pure decisions from the GTK adapter into
      `ui/editor_page/load/policy.rs`: the chunked-versus-direct install threshold
      (`requires_chunked_install` and its inputs), the slice-size and
      paragraph-boundary arithmetic from task 0.5, the install-phase and
      abort-disposition classification, and the load-freshness predicate. Confirm
      the module contains no `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5`
      import — that purity is what keeps it in the mutation scope, and
      `make check-workflow-boundaries` fails on a single such import. **Keep it
      `pub(crate)` throughout**: unlike the save policy 3a relocated, this module
      has no benchmark consumer, so nothing justifies widening it. Do not copy the
      palette policy module's 22-`pub`-item exposure.
- [x] 5.2 State plainly that this is a coverage **gain from zero**, not a
      relocation: `model/file_load.rs` stays put (task 2), so unlike slot 1 there
      is no parity claim to make for a moved module. Report the newly generated
      mutant count and confirm every one is killed. Slot 2a and 2b both had to make
      this distinction; mixing a gain with a parity claim makes both unreadable.
- [x] 5.3 Decide `services/editor_io.rs`'s buried load-side pure policy explicitly,
      following slot 2b's `services/search_backup.rs` precedent: encoding
      detection and fallback selection, the decode-failure classification, and the
      transient-weight admission arithmetic are pure once given their inputs and
      cannot move to `ui/` because a service must not depend on the adapter.
      Decide whether they become a `services/editor_io/policy.rs` or stay as
      private pure functions with direct unit tests, and **record the reason**.
      They are already inside the mutation scope through `services/**`, so the win
      is testability without a tempdir, not coverage.
      **This file is shared with the save workflow and 3a already decided it
      (3a task 6.5), so do not re-litigate.** If 3a created
      `services/editor_io/policy.rs`, **extend it** with the load-side rules; if
      3a declined and kept private pure functions with direct unit tests, state
      explicitly whether the load side changes that answer and why. Two
      contradicting decisions about one file across consecutive slots is exactly
      the forked convention the programme exists to prevent.
- [x] 5.4 Confirm `crates/lushtext-core/tests/properties/file_load.rs` still
      exercises the same pure logic after the extraction, and extend it to cover
      any newly extracted pure rule that fits the property target's charter
      (deterministic, bounded, GTK-free). Do **not** move GTK-dependent install
      behavior into the property target; the boundary in `.agents/rules/build.md`
      is explicit. The target is guarded by
      `required-features = ["property-tests"]` and is **excluded from default
      nextest and default mutation runs**, so `make test` will not exercise this
      work at all — task 9.12 runs the lane explicitly, and this task is not
      verified until it does.
- [x] 5.5 **Decide `OpenPopoverRowLayoutSnapshot`'s ownership** and record the
      reason. The matrix lists it in this row's `Evidence surface` cell beside
      `FileLoadAdmissionSnapshot`, but it lives in `ui/open_popover/mod.rs` and
      describes recent-document row layout, not load state. **There are three
      possible outcomes, not two**: it folds into this workflow's evidence
      surface; it belongs to the recent-Open popover surface and therefore to slot
      7's sweep; **or the census has a gap.** Check that third one first —
      `ui/open_popover/` and `ui/window/recent_open.rs` appear in **no matrix
      row's file set at all**, so "it belongs to `WFR-SHELL-LAYOUT`" may be an
      assumption rather than a recorded fact. If they are genuinely uncensused,
      the honest output is to **assign them to a row and record the census gap**
      in the matrix, the way slot 2a recorded that
      `ui/window/focus_indexing.rs` had been attributed to the palette row while
      remaining window code. Decide, record, and update the row's cell so a third
      slot does not trip over the ambiguity. Do the same for
      `ui/window/recent_open.rs`'s `recent_documents.loading` state if task 0.7
      found tests reaching into it.
- [x] 5.6 Write the mutation evidence to
      `openspec/changes/migrate-document-load-workflow-readability/evidence/mutation-gain-load-policy.md`,
      following the structure of slot 1's
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`
      and slot 2b's
      `openspec/changes/archive/2026-08-25-complete-search-replace-workflow-readability/evidence/mutation-parity-replace-policy.md`:
      scope re-verification with
      **the exact commands run**, the before/after table, per-survivor
      disposition, and the merge-base diff workaround if `make mutants-diff`
      cannot see working-tree edits. **Keep the mutant anchors coarse —
      file-level generated/killed counts rather than per-line identifiers** —
      because a later simplification pass will refuse edits that invalidate
      recorded anchors, and a line-precise anchor freezes the file. The file is
      named for a gain rather than parity because task 5.2 says that is what it
      is; name it honestly.

## 6. Evidence surface

- [x] 6.1 Create `ui/editor_page/load/evidence.rs` with one accessor that is the
      single source of the workflow's observable state, at the narrowest visibility
      its readers require (`ui/automation.rs` in-crate and the external widget
      harness — the same reader pair every migrated workflow has). Fold the
      existing typed observation in rather than leaving a second path:
      `FileLoadAdmissionSnapshot` is already typed, and
      `load_runtime::snapshot_for_test` plus the load hooks in `load_save.rs` are
      the scattered getters the convention retires.
- [x] 6.2 Expose at minimum: the load generation; the request ticket's identity and
      cancel-token state; the admission snapshot and whether a disposal wakeup is
      armed; whether an install is active, its slice count, and its retained
      weight; whether the install is chunked or direct; the projection-suspension
      state; the load state the tab reports; and the terminal outcome including a
      **publish-refused-as-stale** verdict distinct from a completed load and from
      a user cancellation. Expect slot 2b's lesson to recur: the workflow's
      observable outcome is often already computed somewhere that throws it away —
      where the window or the service computes a load outcome the editor never
      hears about, the honest fix is a named workflow operation that records it,
      not a test getter reaching into the window.
- [x] 6.3 Honor the constraint task 1 promotes to convention: no field may be read
      from inside a mutable borrow of the state the accessor reads. Prove it with
      the required test, in the shape task 1.2 fixes — drive the workflow through
      each operation that takes such a mutable borrow, read the evidence surface
      **after** each one, and assert that repeated reads of unchanged state are
      identical. Do not write a test that reads the surface while a borrow is
      held; that is the panic the constraint exists to prevent, not the proof of
      it.
- [x] 6.4 Confirm reading the surface mutates nothing — no timer, queue,
      generation counter, coordinator, admission reservation, or disposal
      reservation — and does not require the workflow to be in a particular stage.
- [x] 6.5 Retire the load-side inspection seams into the surface and confirm the
      retired functions have no remaining callers. The candidate list from
      authoring, to be re-derived in task 0.3: `load_runtime::snapshot_for_test`,
      `load_runtime::disposal_wakeup_armed_for_test`, and in `load_save.rs`
      `load_projection_suspended_for_test`,
      `transient_load_admission_snapshot_for_test`,
      `transient_load_disposal_wakeup_armed_for_test`,
      `load_installation_slice_count_for_test`,
      `load_installation_active_for_test`, `load_installation_weight_for_test`,
      `load_generation_for_test`, and `load_cancel_token_for_test`. **Do not add a
      per-field `*_for_test` accessor for anything**: a test needing a fact the
      surface lacks extends the surface.
- [x] 6.6 Collapse the load workflow's configuration seams into **one**
      test-policy value in `test_policy.rs`, entirely behind
      `#[cfg(feature = "test-utils")]` so a production build compiles no override
      storage. Candidates include `load_runtime.rs`'s two probe statics
      (`NEXT_LOAD_BODY_DISPOSAL_PROBE`,
      `NEXT_LOAD_DISPOSAL_RESERVATION_WEIGHT`) and their setters. Where an
      override must stay in the service because the service owns the behavior —
      `services/editor_io.rs`'s load-delay and processing-chunk statics — say so
      and keep it there; do not create a second policy value in `ui/` that shadows
      it.
- [x] 6.7 Classify and **preserve** the actuation seams: `apply_load_result_for_test`,
      `apply_reload_error_for_test`, `apply_loaded_content_for_test`,
      `reset_transient_load_admission_for_test`, `load_runtime::reset_for_test`,
      and the chooser-bound seams in `ui/window/dialogs.rs`
      (`select_open_file_for_test`, `select_open_file_uri_for_test`,
      `cancel_open_file_for_test`). These drive steps reachable only through a
      `GtkFileChooser` or a worker completion; `cancel_open_file_for_test` is the
      programme record's own named example of the deferred category. Record the
      count before and after and confirm it did not grow.
- [x] 6.8 Migrate the widget tests catalogued in task 0.7 to read evidence where
      the question is "did the workflow record it", keeping a direct read only
      where the question is genuinely "what is on disk" or where the test is
      *arranging* state rather than asserting it. The ungated
      `page.imp().load_state.set(...)` writes are the priority: they set production
      state from the test side, which is an actuation reach-through masquerading as
      setup — decide per site whether it becomes a named test seam that already
      exists, an evidence assertion, or a real load through the workflow. State
      per site which category it is and how many moved, in
      `openspec/changes/migrate-document-load-workflow-readability/evidence/widget-test-load-site-migration.md`,
      following slot 2b's `widget-test-search-backup-site-migration.md`.
- [x] 6.9 Confirm the project test count has not decreased, recorded before and
      after with the counting method stated, in
      `openspec/changes/migrate-document-load-workflow-readability/evidence/test-counts.md`.

## 7. Automation: project from evidence without widening

- [x] 7.1 Identify this workflow's exported surface exactly and record it from
      `docs/automation-reference.md` rather than from this file: the snapshot field
      `tabs[].load_state` (`untitled`, `loading`, `loaded`, `failed`, `unknown`)
      and the readiness blocker `file-load`. Note that `file-load` gates **six**
      documented predicates — `app-startup`, `file-open-complete`,
      `session-restore-complete`, `recovery-restore-complete`,
      `visual-geometry-settled`, and `accessibility-settled` — so this change's
      no-widening proof must cover readiness, not just the snapshot object.
- [x] 7.2 Make those fields project from the load evidence surface instead of
      re-deriving the same state from widgets, with the exported field names,
      types, and semantics **unchanged** — including that `file-open-complete`
      reports `workflow-failure` rather than readiness for a failed load, which is
      documented behavior and a likely regression point when the terminal-outcome
      field from task 6.2 becomes the source.
- [x] 7.3 Add the corresponding rows to the `Evidence Projection Map` in
      `docs/automation-reference.md`. The drift gate reads that table and fails
      when a projected evidence field is added, removed, or renamed without it; the
      table's own rule is that the authority for "is this field projected" is the
      Rust snapshot function, so a field the projection does not read is internal
      and must not appear.
- [x] 7.4 Confirm every other new evidence field from task 6 — generations, ticket
      identity, slice counts, retained weight, projection suspension, admission
      state — is **not** serialized into any snapshot, and that existing redaction
      and omission behavior for private state (paths, buffer text) is preserved.
- [x] 7.5 Prove no widening rather than asserting it: capture an Automation1
      snapshot for the same app state before and after, and diff the `tabs[]`
      objects **and all six affected readiness predicates** to zero differences
      apart from the documented projection being sourced differently. Run
      `make check-automation-docs` and `make automation-client-self-test`.
- [x] 7.6 Decide `WFR-AUTOMATION-SPINE`'s status. Slot 3 is the last slot the
      ledger currently carries it on before slot 4; it stays `pending` in the
      matrix because it continues per migrated workflow. Write it
      `WFR-AUTOMATION-SPINE (partial)` on slot 3b's `complete` line and keep it on
      slot 4's `outstanding` line, adding it to slot 4's row in the
      **remaining-scope table** too so the prose and the machine-readable list
      agree. Omitting it from every outstanding line fails the gate; marking the
      row `migrated` would be a false claim.

## 8. Facade, matrix, and record completion

- [x] 8.1 Write `ui/editor_page/load/mod.rs`'s module-doc narration from the code
      recorded in task 0.4, not from the census trace: the ordered stages with
      their intent named, each delegating to a named role, and **every** inversion
      with the point where control resumes — the admission drain, the worker
      read/decode completion, the per-slice install resumption, the abort and
      cancel resumptions, and finalization. Include the role table and the
      shared-field owners from task 3.5 so a reader can tell where decoded bytes
      enter the buffer and where a stale load is refused.
- [x] 8.2 **Measure the facade and hold it to 370 physical lines.** The response
      order is fixed: delegate stage bodies into the coordination modules' own
      module docs — **and note that the check counts raw physical lines, comments
      and blanks included, so moving thin facade *code* into coordination is
      equally permitted, not just doc lines**; keep each stage to intent plus
      delegate plus resumption point; fold module-ownership detail into the role
      table and compress inversion bullets, the sequence that brought slot 2b back
      from 379 to 369. **Do not edit the budget line.** Raising it is a convention
      amendment requiring every migrated row re-checked in the same change — four
      rows now — and this change already carries one amendment.
      **If an honest narration genuinely cannot fit, park the change in this exact
      state and escalate to the maintainer.** The parked state is operable rather
      than a half-finished tree: leave the facade at its honest measured length;
      leave the row's `Status` **un-flipped** and its ledger line `outstanding`,
      so `make check-workflow-boundaries` reports a pending migration rather than
      a budget violation and stays green on its own terms; record the measured
      count and the irreducible remainder — which stages, which inversions, how
      many lines each — in Appendix A.11; do not mark the surrounding tasks
      complete; and do not archive. The record says the case for correcting the
      number is cheaper to make now than at slot 6 and that the window is closing,
      so make it explicitly from that parked state rather than mangling the
      narration.
- [x] 8.3 **Protect the other facades' headroom.** The search facade sits at 369 of
      370, so any incidental edit to `ui/search_panel/mod.rs` can break the gate;
      3a's save facade has whatever margin it measured. Confirm this change touches
      neither, or re-measure and record.
- [x] 8.4 Add the `### WFR-DOCUMENT-LOAD` subsection under
      `Migrated Workflow Roles` in the documented format — `facade`,
      `coordination`, `policy`, `evidence`, `mutation parity` — naming real paths,
      with `mutation parity` pointing at task 5.6's evidence file and stating that
      it records a gain rather than a relocation parity. **Record that pointer in
      the live `openspec/changes/migrate-document-load-workflow-readability/...`
      form, and do not pre-emptively archive-prefix it.** This was settled
      empirically against `scripts/check-workflow-boundaries.py`'s
      `claim_exists()`: before archival the live form is the **only** accepted
      form, and an archive-prefixed pointer on an unarchived change resolves to
      `False` and hard-fails the gate. The checker's `archive/*-<name>/` fallback
      also keeps resolving the live form **after** archival, so a live-form
      pointer never rots. Rewriting an existing pointer to the archive-prefixed
      form is an optional post-archival human-readability edit, never a
      correctness requirement. Note the per-workflow subdirectory as this row's
      role home and that `load_save.rs` is gone.
- [x] 8.5 Update the row's cells: `Current size` and `Seams (i/c/a/p)` from task
      0.3, `Owned pure policy` for the newly extracted policy **and** the recorded
      decision that `model/file_load.rs` stays, `Seam value object` from
      `required:` to the reified `LoadRequestTicket`, `Evidence surface` for the
      new surface with the `OpenPopoverRowLayoutSnapshot` decision from task 5.5,
      `Risk` recording that the tier-3 install path is now covered, `Slot` `3b`,
      and `Status` `migrated`.
- [x] 8.6 Update the `Seam Value Objects` section: move the
      `required: LoadRequestTicket` entry to `done:` and state what it removed (the
      exploded parameters at both dispatch sites and the clause-by-clause
      comparison at the completion). With `QueuedSaveTicket` done in 3a, note how
      many of the four originally-unreified seams remain (`WorkspaceWatchTicket`
      for slot 5 and `NotesBrowserTicket` for slot 5).
- [x] 8.7 Update the row's `Workflow Stage Traces` entry so the trace names the
      current operations and modules, with the corrected inversion set from task
      8.1 and the shared-field owners from task 3.5. Also update or retire the
      programme's symptom-3 wording where it says `load_save.rs` "was 1,795 lines
      holding two workflows" — the measurement stays as history, but the record
      must say the file is gone and which changes dissolved it.
- [x] 8.8 Advance `docs/next/workflow-readability.md`: flip slot 3b's ledger line
      to `complete` with `WFR-AUTOMATION-SPINE (partial)`, keep
      `WFR-AUTOMATION-SPINE` on slot 4's outstanding line and in slot 4's
      remaining-scope row, update the status line to record slot 3 complete and
      slot 4 next, and add a "Baseline after slot 3b" table reporting workflows
      migrated, share of censused `ui/` + `model/` lines, policy modules relocated
      (unchanged by this change, and the candidate denominator drops again because
      `file_load.rs` is resolved as domain), test seams addressed, seams reified,
      automation projections, and the facade budget position with the measured load
      facade line count.
- [x] 8.9 Add a "Convention friction slot 3b hit" section for slots 4 through 7:
      whether the promoted reentrancy constraint was already satisfied by the
      migrated rows or needed tests written, whether the per-workflow role home
      read well on a second adopter, whether the 370-line budget held on a facade
      narrating one stage order with many entry points, whether the bounded role
      set covered load's cancellation and abort paths, and whether the
      `data-safety` pass produced findings. Four workflows are migrated after this
      change, so the retroactive-amendment rule is more expensive again — say so
      with the row count, and note that slot 4's four rows will make it more
      expensive still.
- [x] 8.10 Update `AGENTS.md`, `README.md`, and any `.agents/rules/*.md` or
      `.agents/skills/**` reference naming a path this change moved or removed
      (`load_save.rs`, `load_runtime.rs`), plus `.cargo/mutants.toml` if a legacy
      hand-listed UI entry or `exclude_re` entry retires with this workflow — the
      config comment says the remaining UI entries "retire as their workflows
      migrate", so check whether any names a load path and retire it rather than
      widening anything.

## 9. Verification

- [x] 9.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries` (policy purity, mutation reach, role
      completeness, facade budget, ledger-versus-matrix agreement),
      `check-filesystem-boundary`, `check-automation-docs`,
      `check-accessibility-policy`, and `check-visual-proof-policy`.
- [x] 9.2 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines**. A recovered flake is a blocker per
      `.agents/rules/preexisting-blockers.md`: read the real failure, classify the
      wait, fix the cause, rerun in isolation. Attach task 6.9's counts.
- [x] 9.3 `make mutants-diff` clean, with task 5.6's evidence attached and every
      survivor closed by an added test rather than a scope change. The extraction
      is unscoped-to-scoped, so its new mutants must be fully killed, not excluded.
- [x] 9.4 The mandatory proof lanes for `ui/` and widget-test changes, each from a
      clean artifact root: `make visual-geometry-smoke`, `make accessibility-smoke`,
      `make visual-smoke`. Order these **after all source edits**, including
      documentation and rules edits: the accessibility policy gate fingerprints the
      contents of accessibility-relevant files, so an edit after the lane runs
      voids the proof and the lane must be rerun.
- [x] 9.5 **Load behavior equivalence**, each case with a test asserting the
      user-visible outcome and the resulting buffer content: a small file taking
      the direct install; a file large enough to require chunked installation; a
      file whose largest paragraph exceeds the slice budget, which must install in
      one turn per the paragraph-boundary contract; an empty file; a file whose
      bytes are undecodable in the requested encoding; a missing file and a
      permission-denied file; a reopen with a different encoding replacing loaded
      content; a load cancelled by the user mid-install, whose partial content must
      be cleared and whose payload must be retired; a load superseded by a newer
      load of a **different** path, whose stale completion must publish nothing;
      and a load whose editor is closed before the worker returns. The last three
      are what `LoadRequestTicket` protects.
- [x] 9.6 **Install-slicing contract equivalence.** Confirm install and clear
      slices still end on paragraph boundaries and that installation stays linear
      rather than quadratic in the number of slices for a single-paragraph
      document, **measured rather than asserted** — a large single-line fixture
      with slice-count and elapsed-time evidence, since this is the regression
      whose failure mode was minutes of frozen crash recovery. Record the numbers
      in the evidence directory.
- [x] 9.7 `make crash-recovery-smoke` clean. Bounded install is the path crash
      recovery uses to restore a large draft, so a regression here shows up as a
      recovery hang rather than a test failure.
- [x] 9.8 `make performance-smoke` clean, and a Criterion baseline comparison
      (`make bench-baseline` before, `make bench-compare` after, or the recorded
      equivalent) for the load and install benchmarks. This is the one workflow in
      slot 3 whose contract is a performance contract; a green functional suite is
      not sufficient proof.
- [x] 9.9 Re-run the `data-safety` skill in explicit mode over the actual diff and
      resolve every confirmed finding. **A tier-3 change does not close with an
      open data-safety finding**, and per
      `.agents/rules/preexisting-blockers.md` a pre-existing one found here is in
      scope rather than deferrable.
- [x] 9.10 **Live run.** Open real files through the chooser, the recent-documents
      popover, and sidebar activation; reopen a document with a different encoding;
      and cancel a load of a large file, watching stderr for new `Gtk-WARNING`,
      `Gtk-CRITICAL`, `GLib-GObject-WARNING`, pixman `*** BUG ***`, or
      `Trying to measure` output. Load reads rather than writes, so the risk is
      lower than 3a's — but still use throwaway fixture files inside an isolated
      `LUSHTEXT_DATA_DIR` and isolated XDG directories (the crash-recovery smoke
      lane's isolation pattern) rather than the maintainer's real documents, and
      **check for a running `dev.cominotti.lushtext` owner before launching** and
      stop rather than racing it. Record exactly what was run, the fixture paths,
      and **what remains uncovered**, in
      `openspec/changes/migrate-document-load-workflow-readability/evidence/live-run.md`
      with the captured stderr beside it. Do not silently downgrade to a headless
      run and call the item done.
- [x] 9.11 Cold-read check: with this change's conversation set aside, read only
      the facade and confirm the whole open-to-loaded story and every inversion are
      followable without opening the coordination or policy modules, and that a
      reader can tell where decoded bytes enter the buffer, where a stale load is
      refused, and where a cancelled load's content is cleared. If not, the split
      in task 3 is wrong and must be revisited before archiving.
- [x] 9.12 **`make test-prop` clean.** Task 5.4 touches
      `crates/lushtext-core/tests/properties/file_load.rs`, which the default
      nextest and mutation lanes never run because it is gated behind
      `required-features = ["property-tests"]` — a green `make test` proves
      nothing about it. If the property extension is nontrivial, also run
      `make test-prop-deep PROPTEST_DEEP_CASES=1024` once and record the result;
      do not raise the default 64-case pull-request count to investigate a broad
      invariant. Record any new regression seed committed to
      `crates/lushtext-core/proptest-regressions/properties.txt`.
- [x] 9.13 `openspec validate migrate-document-load-workflow-readability --strict`
      clean.

## 10. Handoff

- [x] 10.1 Confirm the programme record and the matrix agree: `WFR-DOCUMENT-SAVE`
      and `WFR-DOCUMENT-LOAD` are both migrated, slot 3 is complete in both
      halves, slot 4 is the next outstanding slot with `WFR-AUTOMATION-SPINE`
      carried onto its line, and `load_save.rs` no longer exists.
- [x] 10.2 Hand slot 4 the facts it needs. Slot 4's four rows
      (`WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`,
      `WFR-BUFFER-REPLACEMENT`) all touch what slot 3 just migrated, so record:
      which load and save operations they call and by what name; the shared-field
      ownership decisions from task 3.5 and 3a's task 3.2 that slot 4 inherits;
      the seam and reach-through counts that turned out to be shared with
      `WFR-DRAFT-RECOVERY`; the per-workflow role home precedent and the exact
      paths, since `ui/window/` hosts twelve workflows and will hit the same
      collision; and that `journal` is the role name to check first for drafts,
      session, and local history, because all three keep durable records a later
      stage restores from — the record predicts it and slot 2b's definition
      already includes the admission gate protecting the record.

---

## Appendix B — handoff

### B.1 Programme and matrix agreement (task 10.1)

Confirmed mechanically, `make check-workflow-boundaries` passing:

- `WFR-DOCUMENT-SAVE` and `WFR-DOCUMENT-LOAD` are both `migrated`, each with a
  complete `Migrated Workflow Roles` subsection naming real paths.
- **Slot 3 is complete in both halves.** The ledger reads
  `slot 3a (complete): WFR-DOCUMENT-SAVE, WFR-AUTOMATION-SPINE (partial)` and
  `slot 3b (complete): WFR-DOCUMENT-LOAD, WFR-AUTOMATION-SPINE (partial)`.
- Slot 4 is the next outstanding slot, and `WFR-AUTOMATION-SPINE` is carried onto
  its outstanding line **and** into its remaining-scope table row, so the prose
  and the machine-readable list agree. `WFR-AUTOMATION-SPINE` stays `pending` in
  the matrix rather than `migrated`, because it continues per migrated workflow.
- **`ui/editor_page/load_save.rs` no longer exists**, and neither does
  `ui/editor_page/load_runtime.rs`. The programme's third measured symptom is now
  history; the record says so and names the two changes that dissolved it.

### B.2 To slot 4 (`WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-BUFFER-REPLACEMENT`) — task 10.2

**Load and save operations slot 4's rows call, by name.** All four rows touch
what slot 3 migrated, so these are the names to call rather than state to reach
into:

| Operation | Owner | Notes |
| --- | --- | --- |
| `load_file_async(path)` | load facade | the plain entry; used by reload, retry, and discard |
| `load_file_async_with_encoding(path, reopen_as)` | load facade | reopen-with-encoding |
| `load_file_async_with_planning_terminal(path, on_terminal)` | load facade | **session restore's entry.** The terminal is now released on *every* path — see the fixed strand below |
| `set_file_path_for_pending_load(path)` | `ui/editor_page/document_identity.rs` | provisional identity before content exists; **shared identity, not load state** — load calls it, as save calls `set_file_path_with_canonical` |
| `cancel_load()` / `dispose_load_resources()` | load facade | the two retirement entries |
| `connect_file_loaded(f)` | load facade | fan-out, survives reloads |
| `connect_load_completed_once(f)` / `connect_load_failed_once(f)` | load facade | **new named operations** replacing the window's direct `.imp().load.*_callback` writes |
| `load_evidence()` | load evidence | read this instead of reaching into `imp().load*` |
| `set_file_path` / `set_file_path_with_canonical` / `size_check` | `ui/editor_page/document_identity.rs` | shared identity; neither document workflow owns it |
| `set_restore_position` / `cursor_position` / `visible_top_line` / `apply_restore_position` | `ui/editor_page/restore_position.rs` | **cross-cutting, five owning workflows.** Slot 4's session-restore row is one of them; call it, do not absorb it |
| `save_file_async` / `save_file_async_to_path` / `save_file_async_for_close` / `is_saving` / `save_evidence` | save facade (3a) | unchanged by this change |

**Shared-field ownership slot 4 inherits** — full table in A.10. The three that
matter most: `cancel_load` and all `imp().load*` state belong to load; `size_check`
and the document-identity group belong to *neither* document workflow; and the
restore-position group is cross-cutting and must stay shared.

**A data-safety contract slot 4 now depends on.** Every load terminal either
carries a parked request's background planning owner into the restart or releases
it — no path drops it. The pre-change code dropped it in `finish_load_finalization`
on both branches, which would stall the session-restore sequencer, because
`SessionRestorePolicy::release_permit` counts exactly those releases to decide
when to open the next document. **Slot 4 owns that sequencer and should not
re-introduce a drop.**

**Two pre-existing candidates routed to slot 4**, both recorded in A.9 with the
missing evidence named:

1. **`installation_incomplete` is invisible to the draft-autosave lane.** Every
   guard in `ui/window/drafts.rs` tests only `is_modified()` / `draft_dirty()` /
   `is_evicted()`. Whether a cancelled load's emptied buffer can overwrite a
   draft that held unsaved edits needs the `draft_dirty` transition trace, which
   is slot 4's file. Identical shape pre-change.
2. **The planning completion's dead-editor early return** leaves a stored
   terminal unfired, relying on GTK dispose to reach `dispose_load_resources`. No
   path was found where dispose is skipped, but it was not proved. Worst case is
   a stalled sequencer, never lost content.

**Seam and reach-through counts that turned out to be shared with
`WFR-DRAFT-RECOVERY`.** The row's pre-migration `Seams` cell pooled the 6
load-side `test-utils` overrides in `services/editor_io.rs` with save's and with
drafts'. Those 6 stay in the service. Slot 4 should expect to re-derive
row-scoped counts rather than inherit the cell — this is the third consecutive
slot for which the cell was wrong.

**The per-workflow role home precedent, and the exact paths.**
`crates/lushtext-core/src/ui/editor_page/load/` with `mod.rs` (facade),
`admission.rs` + `execution.rs` + `retirement.rs` (coordination), `policy.rs`,
`evidence.rs`, `test_policy.rs`. `ui/window/` hosts **twelve** workflows, so slot
4 will hit the same collision on its first `policy.rs`; the nested
`ui/**/policy.rs` glob is verified reachable by both cargo-mutants and
`check-workflow-boundaries` at two adopters now, so slot 4 only needs to
re-verify after its own move, not re-derive the shape.

**`journal` is the role name to check first**, and this time it should fit.
Drafts, session, and local history all keep durable records a later stage
restores from, which is the test slot 3a wrote and slot 3b re-applied: *does a
later stage of this workflow restore from it*, not *does it touch the disk*. Load
failed that test outright; drafts should pass it. Slot 2b's definition already
includes the admission gate protecting the record.

**Run the rustdoc gate before shipping a facade.** It is not in `make check`,
`make pre-commit`, or `make check-policy`, and a narrative facade in a `pub`
module naturally wants to intra-doc-link its own private coordination modules —
which is a `rustdoc::private_intra_doc_links` error. Slot 3a shipped that failure
to CI. The command is recorded in `.agents/rules/build.md`; the fix is always to
drop the link and keep the name in backticks, never to widen visibility.

**Cost warning.** Four rows are migrated, so a convention amendment now owes four
per-row re-checks. Slot 4 migrates four more rows, taking it to eight. Slot 3b's
amendment cost one test to write; the same amendment after slot 4 would have cost
up to five.

## Appendix A — orientation record

Filled in during implementation. Each subsection is required by the task that
names it; leaving one empty means that task is not done.

### A.1 Gate evidence (task 0.1)
Verified mechanically on a clean tree, not read from the proposal.

- `openspec/changes/archive/2026-08-26-migrate-document-save-workflow-readability/`
  exists with `proposal.md`, `tasks.md`, its four evidence files, and its
  `specs/gtk-adapter-module-boundaries/spec.md` delta. The delta is merged: the
  per-workflow-subdirectory permission is present in
  `openspec/specs/gtk-adapter-module-boundaries/spec.md` (the "Role files inside
  a per-workflow subdirectory stay unqualified" scenario and its surrounding
  requirement text).
- `docs/workflow-readability-matrix.md` marked `WFR-DOCUMENT-SAVE` `migrated`
  with a complete `Migrated Workflow Roles` subsection naming
  `ui/editor_page/save/` as its per-workflow role home.
- `docs/next/workflow-readability.md`'s ledger marked slots 1, 2a, 2b, and 3a
  `complete` and slot 3b `outstanding`.
- `make check-workflow-boundaries` passed on the clean tree before any edit:
  "3 workflow policy module(s) are pure and mutation-scoped".
- Slot-2a deliverables confirmed present: the machine-readable
  `- normative facade line budget: 370` line, the stage-order qualification
  rule in `openspec/specs/gtk-adapter-module-boundaries/spec.md`, and the
  working evidence-to-snapshot drift check in
  `scripts/check-automation-docs.py` (`EVIDENCE_PROJECTIONS` plus
  `evidence_projection_findings`, self-tested).
- 3a's handoff (its task 12.2, Appendix B.1) was read in full; its five handed
  decisions are consumed in A.10 rather than re-derived.

### A.2 Premise re-verification (task 0.3)

Every figure re-derived against the post-3a tree. "Unchanged" is recorded
explicitly.

| Figure | Authoring claim | Measured | Verdict |
| --- | --- | --- | --- |
| `load_save.rs` residual after 3a | two estimates, ≈1,046 and ≈1,088 | **1,212** | both authoring estimates were **low**. Taken from 3a's handoff and confirmed by `wc -l`, not from either estimate. 3a left **no** save-side residue: the file held the load half plus the two cross-cutting groups |
| `ViewInteractivityState` in the residual | expected | present, and 3a's `save/execution.rs` defines its own `SaveViewInteractivity` twin | confirmed; collected by this change, and the twin deliberately not merged backward |
| `set_file_path_for_pending_load` in the residual | expected | present | confirmed. 3a assigned it to load; review re-decided it as **shared identity** and it now lives in `document_identity.rs` — see A.10 |
| `load_runtime.rs` | 423 | 423 | unchanged |
| `model/file_load.rs` | 462 | 462 (279 production + 183 co-located tests) | unchanged |
| `model/file_load.rs` reference set | census said 4; proposal said 6 production files | **6 production files**, but **not the six the proposal listed**. `model/save_admission.rs` is gone — slot 3a relocated it — and its place is taken by `ui/editor_page/save/policy.rs`. Full table in A.3 | census undercounted; the proposal's *count* was right and its *list* was stale. The `services/editor_io.rs` reference is genuine (`transient_load_weight`, `FileLoadFacts`, `FileLoadPlan`), which is what decides task 2 |
| owning workflows among its `ui/` consumers | proposal said 2 | **3** (`WFR-PLAIN-DISPOSAL`, `WFR-DOCUMENT-LOAD`, `WFR-DOCUMENT-SAVE`) | the relocation in 3a *added* a `ui/` consumer. The cross-cutting case is stronger than the proposal's, not weaker |
| Six grep false-positive families | listed | all six re-confirmed as name collisions, not references | unchanged; recorded in the matrix so a fourth slot does not repeat the overcount |
| Row `Current size` (`10 files, 5,301 lines (ui 3,265 / model 661 / services 1,375)`) | — | wrong in both directions. The `ui` subtotal pooled window files this row only *calls*; the `services` subtotal counted the whole of `editor_io.rs` (3,035), shared with save and every other read path. Row-scoped pre-migration `ui` code was 1,212 + 423 = **1,635** | corrected in the matrix |
| Row `Seams (i/c/a/p)` (`23/7/3/1 = 34 fns, 55 sites, 3 override statics`) | — | **not row-scoped.** Row-scoped: 10 inspection surfaces (8 on the editor page, 2 crate-internal in `load_runtime.rs`), 2 configuration statics in `load_runtime.rs`, 8 actuation surfaces (5 editor-page including the redundant `load_runtime::reset_for_test`, 3 chooser-bound in `ui/window/dialogs.rs`), plus 6 load-side `test-utils` overrides in `services/editor_io.rs` **shared with `WFR-DOCUMENT-SAVE` (3a) and `WFR-DRAFT-RECOVERY` (slot 4)** | corrected in the matrix with the shared population named |
| Inversions in the stage trace | 4 | **7** | fourth confirmation that census inversion counts are floors. Full list in A.4 |
| `ui/search_panel/mod.rs` | 369 of 370 | 369 | unchanged, and untouched by this change |
| `ui/editor_page/save/mod.rs` | 223 of 370 | 223 | unchanged, and untouched by this change |

### A.3 `model/file_load.rs` reference set and decision (tasks 2.1, 2.2)

Grouped by layer, matching the form slot 2b used for `model/workspace_search.rs`.
The full table, the decision, the dependency-direction reason, and the recorded
false-positive families are in the matrix under
[Modules confirmed as domain and staying in `model/`](../../../docs/workflow-readability-matrix.md#modules-confirmed-as-domain-and-staying-in-model);
the census cell now points there and states the decision is closed.

**Decision: it stays in `model/`.** Two independent reasons, either sufficient:

1. **Dependency direction.** `services/editor_io.rs` depends on it. Relocating it
   under `ui/` would make a service depend on the adapter, which the convention
   forbids outright.
2. **Cross-cutting eligibility.** Its `ui/` consumers span **three** owning
   workflows, and eligibility counts owning workflows rather than consuming
   files. Three clears the bar without reason 1.

It is already pure, already mutation-scoped through `model/**`, and already
carries co-located unit tests, so the move would trade a dependency-direction
violation for nothing.

### A.4 Current ordered stages before the change (task 0.4)

Read from the code, not from the census trace. Line numbers are pre-change.
Inversions are marked ⇢ with their resumption point named.

1. **Entry** — `load_save.rs:417` `load_file_async`, `:422`
   `load_file_async_with_encoding`, `:427`
   `load_file_async_with_planning_terminal`, all funnelling into `:438`
   `load_file_async_with_encoding_and_planning_terminal`. The window first calls
   `:1125` `set_file_path_for_pending_load`.
2. **Park or rotate** — `:444` if `finalizing`, park in `pending_load` and
   `cancel_noninstall_load_resources`; `:455` if an installation is live, park,
   set the cancel token, `load_runtime::cancel_for_editor`, and
   `abort_chunked_install(Cancel)`. Otherwise reset identity/metadata state and
   rotate `{generation, cancel_token}` at `:494-500`.
3. **Plan** — `:504` `spawn_blocking_then(editor_io::plan_text_file)`.
   **⇢ inversion 1: planning worker, resuming in the completion closure at
   `:507`,** which checks `load_request_is_current`, then either
   `load_runtime::submit` or `apply_load_result_if_current(Err)`, and always
   `finish_load_planning`.
4. **Queue** — `load_runtime.rs:129` `submit` → `FileLoadAdmissionPolicy::queue`
   → `schedule_drain`. **⇢ inversion 2: `glib::idle_add_local_once`, resuming in
   `load_runtime.rs:199` `drain`.**
5. **Drain** — retires stale requests, refreshes priorities, computes external
   save pressure and protected residency, then admits. When
   `try_reserve_file_load_for_gtk` refuses, the grant is returned to the queue
   and a capacity wakeup is armed at `:294-302`. **⇢ inversion 3:
   `DisposalCapacityWakeup`, resuming in `schedule_drain`.**
6. **Read and decode** — `load_runtime.rs:306` `dispatch` →
   `spawn_blocking_then(editor_io::load_planned_text_file)`. **⇢ inversion 4:
   read/decode worker, resuming in the completion closure at `:331`,** which
   calls `accept_admitted_load_outcome`.
7. **Accept** — `load_save.rs:581` `accept_admitted_load_outcome` re-checks
   generation and token, then `:605` `install_guarded_load` →
   `:692` `requires_chunked_install`.
8. **Install, direct** — `:640` `install_loaded_direct`: `begin_load_installation`,
   `begin_irreversible_action`, `set_text`, then `complete_loaded_installation`
   and `finish_load_finalization`.
9. **Install, chunked** — `:661` `start_chunked_install` builds
   `ChunkedLoadInstall` and calls `:117` `schedule_install_slice`.
   **⇢ inversion 5: 1 ms timeout per slice, resuming in `:125`
   `run_install_slice`,** which re-checks freshness and dispatches
   `run_existing_clear_slice` (`:177`), `run_text_insert_slice` (`:196`), or
   `run_cancelled_clear_slice` (`:355`). Insert boundaries come from
   `next_install_boundary`; clear boundaries from `:159` `delete_buffer_slice`.
10. **Publish** — `:730` `complete_loaded_installation` adopts size, size class,
    canonical path, encoding, BOM, health findings, and mtime; calls
    `apply_restore_position`; seeds local history; restores the suspended
    projections; sets `Loaded`; starts the monitor; fires the one-shot completed
    callback and then the fan-out `file_loaded_callbacks`.
11. **Finalize** — `:839` `finish_load_finalization` clears `finalizing`, takes
    the parked request, drops the permit. **⇢ inversion 6:
    `TransientLoadPermit::drop` posts `glib::idle_add_once`, resuming in
    `load_runtime.rs:345` `release_on_main`,** which re-arms both lanes' drains.
12. **Retire** — `:884` `cancel_load` and `:931` `dispose_load_resources` →
    `:962` `cancel_current_load_resources` → `:297` `abort_chunked_install`.
    A cancel moves the session to `ClearingCancelled`. **⇢ inversion 7:
    cancelled-clear slices, resuming per slice in `:355`
    `run_cancelled_clear_slice`,** which clears the partial buffer, publishes the
    user-cancelled terminal when `user_cancel_pending`, and restarts a parked
    request.
13. **Window side** — `documents.rs:197` `set_file_path_for_pending_load`, `:212`
    and `:247` the two one-shot callbacks written **inline through `.imp()`**,
    `:306`/`:308` the load call; `encoding.rs:391`/`:401` reopen-with-encoding;
    `search.rs:885` reload-to-match; `focus_indexing.rs:388` indexing reload;
    `tabs.rs:84` `cancel_load` on close; `session_persistence.rs:566`
    `set_restore_position`. `dialogs.rs`, `recent_open.rs`, and
    `session_restore.rs` all reach the load through `window.open_document`.

**Seven inversions, not four.** The census recorded the read worker, the drain,
the install slices, and finalization; it missed the planning worker, the
disposal-capacity wakeup, and the cancelled-clear resumption.

### A.5 Paragraph-boundary contract, as implemented today (task 0.5)

`.agents/rules/rust.md` states it normatively:

> Bounded buffer installation and clearing slices must end on paragraph
> boundaries (`next_replacement_boundary` cuts just after a newline; iter-based
> clears extend to the next line start). GTK text layout validates whole
> paragraphs, so a slice that stops mid-paragraph re-lays-out everything already
> installed in that paragraph on every later slice — quadratic work that froze
> crash recovery of a 33 MB single-line draft for minutes. A paragraph larger
> than the slice byte budget installs or clears in one turn.

The implementation, quoted as found:

- **Install side**, `model/buffer_replacement.rs:72` `next_replacement_boundary`,
  re-exported as `model/file_load.rs:276` `next_install_boundary`. Returns
  `text.len()` when the remainder fits the 256 KiB budget; otherwise cuts just
  after the **last** newline inside the budget; otherwise scans **past** the
  budget to the next newline, so an oversized paragraph is taken whole.
- **Clear side**, the pre-change `load_save.rs:159` `delete_buffer_slice`:
  `remaining.min(CLEAR_SLICE_CHARS)` with `CLEAR_SLICE_CHARS = 64 * 1024`, then
  `if !end.is_end() && !end.starts_line() { end.forward_line() }`.

**Ownership after the split.** The install side **stays in `model/`**: it is
shared with `WFR-BUFFER-REPLACEMENT` and duplicating it would fork a shared
limit. The clear side moved into `ui/editor_page/load/policy.rs` as the pure pair
`clear_slice_char_count` + `clear_slice_extends_to_paragraph_end`, with the GTK
iterator work staying in `execution::delete_buffer_slice`. The budget constant is
pinned against `model/buffer_replacement::REPLACEMENT_CLEAR_SLICE_CHARS` by a
unit test, which is also what killed this module's one mutation survivor.

Measured, not asserted:
`openspec/changes/migrate-document-load-workflow-readability/evidence/install-slicing-linearity.md`.

### A.6 Reentrancy constraint basis and per-row re-check (tasks 1.1, 1.3)

**Basis, from the code before amending anything:**

- One accessor reads the whole surface. `search_panel/evidence.rs`,
  `command_palette/evidence.rs`, and `save/evidence.rs` each expose exactly one
  `*_evidence()` / `evidence()` method returning the whole value.
- The exemplar's `evidence.rs` module doc already recorded the shared-borrow
  constraint as a per-workflow note, and `save/evidence.rs` restated it — the
  duplication is itself the evidence that it belongs in the convention.
- Slot 2b obeyed it while adding ten fields and recorded explicitly that it
  "should become a stated convention, not a per-workflow module note", because
  "it follows from 'one accessor reads the whole surface' plus `RefCell`, and
  every later slot will re-derive it".

**Per-row re-check.** Recorded in the matrix's
[Slot 3b amendment re-check](../../../docs/workflow-readability-matrix.md#slot-3b-amendment-re-check),
following slot 2b's format. Summary: `WFR-SEARCH-REPLACE` **existed** (slot 2b's
reference implementation, correct shape); `WFR-DOCUMENT-SAVE` **existed** (slot
3a, verified to read *after* each mutating operation rather than during one);
`WFR-COMMAND-PALETTE` was **missing and was written here** —
`command_palette::test_evidence_reads_stay_side_effect_free_across_palette_mutation`.
The palette had only a teardown-observation test, which proves a different
property.

The other settled conventions were re-confirmed unchanged (task 1.4): facade
budget 370 with all four migrated facades measured against it, bounded
coordination role set, seam value-object shape, evidence-surface visibility rule,
and 3a's per-workflow role home.

Standing guidance updated (task 1.5): the evidence-surface section of
`.agents/rules/widget-wiring.md`, the `Evidence` role bullet in
`.agents/rules/rust.md`, and the matrix's new
`Settled Conventions > Evidence-surface reentrancy` subsection.
`make check-agent-docs` and `make check-agent-skills` pass.

### A.7 Widget-test reach-through sites and categorization (tasks 0.7, 6.8)

Full site list, per-site categorization, and outcomes are in
`openspec/changes/migrate-document-load-workflow-readability/evidence/widget-test-load-site-migration.md`.
Summary:

- **17 ungated `.imp().` sites catalogued, 7 migrated.** All 7 were *writes* to
  load state (5 `load_state.set`, 2 `file_path.replace`) across three tests, and
  all 7 became **real drives of the workflow**. No new seam was created.
- **10 ungated sites recorded and left**: `file_size.set` × 8 and
  `size_check.set` × 1 are shared editor-page document metadata, not load state;
  1 is the recent-documents popover's own loading flag. Both groups are assigned
  in the matrix rather than absorbed.
- **10 inspection surfaces retired, 51 call sites migrated** to `LoadEvidence`.
- **One retirement required extending the surface** rather than mapping:
  `load_cancel_token_for_test` returned a live `Arc`, so the surface gained
  `previous_request_cancelled`.
- **The proposal's site claim was low.** It named "several"
  `page.imp().load_state.set(...)` writes and one `recent_documents.loading`
  read; the full grep found 17 load-relevant sites across three files.

### A.8 Coordination role mapping, and why `journal` does not fit (task 3.1)

Three cohesive coordination jobs, all taking unqualified bounded names because
the workflow owns exactly **one** stage order.

| Job | Role | Module | Contents |
| --- | --- | --- | --- |
| Everything before decoded bytes exist | `admission` | `load/admission.rs` | the entry stage, identity rotation into one ticket, the parked-request slot, the compact planning probe, the process-wide `thread_local` coordinator, the queue, the idle drain, the disposal-capacity wakeup, the admitted read dispatch, and exactly-once charge release |
| Accepting and installing them | `execution` | `load/execution.rs` | the freshness gate at the completion, direct install, the four-phase bounded install state machine and its scheduler, the final projection, and finalization |
| Giving them back | `retirement` | `load/retirement.rs` | user cancellation, widget disposal, the abort transitions, the cancelled-clear phase, and the user-cancelled terminal |

**Authoring's expectation was confirmed, with one refinement.** Authoring
expected the coordinator/drain/wakeup as `admission` and the dispatch,
completion, and install slicing as `execution`, and asked whether cancellation
was cohesive enough for its own module. It is: ~190 lines answering one question
— "the workflow is being stopped; give back the payload, the charge, the partial
buffer, and the identity" — and it is the data-safety-critical half, so a
separate module makes the tier-3 risk legible instead of burying it in a
600-line `execution`.

**The one judgement worth recording is what was *not* split.** The install state
machine is one object with four phases. `execution` keeps the session type, the
scheduler, and the dispatcher; `retirement` owns the abort transitions and the
`ClearingCancelled` phase body, which the dispatcher calls across the boundary.
Splitting the *object* would have hurt readability; splitting the *question* did
not.

**`journal` was checked first and rejected**, on slot 3a's reusable test: "does a
later stage of *this* workflow restore from it", not "does it touch the disk".
Load keeps no durable record at all — it reads one, installs it, and forgets it.
The nearest durable record on this path is the draft, owned by
`WFR-DRAFT-RECOVERY` (slot 4). No novel job appeared, so no role name was added
and the bounded set is unchanged; this change already carries one amendment.

### A.9 Data-safety passes (tasks 0.6, 9.9)

**Pass 1, before writing code**, over the intended diff surface. Findings, both
carried into the implementation rather than deferred:

1. **The planning-owner strand (fixed).** The pre-change
   `finish_load_finalization` took the parked `PendingFileLoad` and, on the
   `disposed` branch, dropped it — and on the non-disposed branch restarted the
   load with `load_file_async_with_encoding`, which **drops the parked request's
   `planning_terminal` callback** rather than carrying it. That callback is a
   background planning *owner*: `ui/window/session_restore.rs` counts exactly
   these releases (`release_permit`) to decide when to open the next document, so
   a dropped owner stalls the restore sequencer. Unreachable in practice today
   (it needs a second planning-terminal load request on the *same* editor while
   that editor is finalizing, and session restore gives each document a fresh
   page), which is why it had never been seen. **Fixed in
   `execution::finish_load_finalization`: every terminal now either carries the
   owner into the restart or releases it, and no path drops it.** Recorded in the
   matrix's roles entry and in the friction section as a fact slot 4 inherits.
2. **The `installation_incomplete` invariant (preserved, and now legible).** The
   cancelled path empties the buffer *on purpose*, so a save allowed to run
   against it would write a truncated file over the user's document. The flag is
   set in `retirement::abort_installation` and cleared only by
   `execution::complete_loaded_installation`; `save/admission.rs` still refuses
   with `IncompleteLoadInstallation`. The invariant is now stated in
   `retirement.rs`'s module doc, which is where a reader looking at the clearing
   code will be.

**Pass 2, over the actual diff.** Explicit mode, all five domains, run as leaf
reviewers against the recorded scope (`git diff --name-only`, 22 files). Per-domain
verdicts:

| Domain | Verdict |
| --- | --- |
| `draft-integrity` | **CLEAN.** DI-1/2/3/4/4b have no surface in the scoped files (confirmed by repo-wide grep: `set_draft_dirty`, `autosave_inflight`, `save_manifest`, `find_by_id`, `original_path` live only in `ui/window/drafts.rs`, `services/draft_service.rs`, `model/draft.rs`). DI-5 double guard intact and byte-identical to pre-change |
| `close-flow` | **CLEAN.** CF-1/2/4/5/6 all guarded at their current owners; `tabs.rs`'s `cancel_load()` on detach runs after consent and cannot resurrect a half-cleared buffer |
| `atomic-write` | **CLEAN.** AW-1/4/5/6 not applicable (this workflow performs no writes); AW-2 and AW-3 verified sound |
| `restore-lifecycle` | **CLEAN.** RL-1/2/3/4 all negative; stored restore positions survive a failed or cancelled load because only the publish stage takes them |
| `replace-safety` | **CLEAN**, and zero scope overlap — no Replace All file appears in the diff, verified rather than assumed. Swept anyway so the record is complete: RS-1/1b/1c/2/3/4 all negative, and the cross-check confirms Replace All's write path reads **disk** bytes under a `TargetWriteGuard`, never the GTK buffer, so an `installation_incomplete` editor is not an exposure for it. It independently observed the new save-side guard described below |

**Verified invariants worth recording, because they are the ones a reader of this
diff will worry about:**

- **`TransientLoadPermit` releases exactly once on all eleven terminals**
  (worker success direct and chunked, worker error, `Cancelled`, stale/refused
  completion, dead-page completion, disposal, user cancellation, cancelled-clear
  completion, finalization, and each of the four `finish_chunked_install` early
  returns). Two invariants make it hold and both are preserved: `terminal = true`
  is only ever set in the same `borrow_mut` block that takes the permit, and the
  `Cancel`-with-dead-editor path recurses into `Dispose`, which
  `policy::abort_action` handles rather than ignoring. `Drop` posting
  `glib::idle_add_once` instead of releasing inline is also load-bearing, not
  stylistic: `drain` constructs permits *inside* the coordinator's
  `borrow_mut`, so a synchronous release would re-enter that borrow and abort
  during unwinding.
- **`installation_incomplete` is set on every cancelled-install path** and cleared
  only by a successful publish, so the deliberately-emptied buffer cannot be
  saved over the user's file.
- **`complete_loaded_installation` is unchanged**, including
  `set_modified(false)` before metadata adoption and `load_state.set(Loaded)`
  last, after `apply_restore_position` and history seeding.
- **Freshness got stricter, not looser.** `accept_admitted_load_outcome` now
  validates `Arc::ptr_eq` on the cancellation token through
  `LoadRequestTicket::is_current`; the pre-change code compared only generation
  and flag.

**Confirmed finding, fixed in this change: the save-admission race
(`save/execution.rs`).** `begin_admitted_save` called `editor.cancel_load()` and
then captured buffer text with **no re-check** of `installation_incomplete`.
Reachability was resolved rather than left unresolved:

- The queue stage's gates cannot cover it. A load may start *between* queueing
  and admission: `load::admission::drain` blocks only on *close* save work, so an
  ordinary in-flight save merely adds byte pressure, and no load entry point
  gates on `is_saving()`.
- The window that makes it reachable is a **saturated save byte budget** — a
  queued save waits while another save writes, and a user-initiated load
  (info-bar Retry or Discard, reopen-with-encoding, sidebar activation, recent
  open) has time for both worker round-trips and can be mid-install when the
  queued save is finally admitted.
- The consequence is a lost file, not a lost draft: `cancel_load` on a live
  installation only *schedules* the clearing slices, so the immediately following
  capture reads a **partially installed decode** and writes it over the user's
  document.

Severity **HIGH** (specific but realistic conditions). Fixed by re-checking
`installation_incomplete` immediately after the `cancel_load()` in
`begin_admitted_save` and refusing with `IncompleteLoadInstallation`, exactly as
the queue stage does. **Pre-existing** — `save/execution.rs`'s logic here is
unchanged by this migration — and fixed in this work stream per
`.agents/rules/preexisting-blockers.md` rather than recorded as debt. Flagged
prominently for the review passes because it touches a file slot 3a migrated.

Regression coverage:
`editor_page::test_cancelling_a_live_installation_blocks_saving_the_emptied_buffer`
pins the guard's precondition and the safety property it protects — that
cancelling a live installation sets `installation_incomplete` **synchronously**
(so the flag is already true at the instant a queued save could be admitted),
that a save queued at that moment is refused, and that the file on disk is
untouched. **Deferred, with the reason stated:** the full admission-time
interleaving is not automated. Reproducing it needs the save byte budget
saturated *and* a load install still running at the moment the queued save is
admitted, which the harness can only arrange by timing coincidence — and a
timing-dependent test against a hard zero-`FLAKY` gate would trade a real bug for
an unreliable signal. Making it deterministic needs a new actuation seam to pause
save admission, which is a counted exception this change should not spend.

**Two pre-existing candidates left UNRESOLVED, both outside this row and neither
introduced here:**

1. **`installation_incomplete` is invisible to the draft-autosave lane.** Every
   guard in `ui/window/drafts.rs` tests only `is_modified()` / `draft_dirty()` /
   `is_evicted()`. After a cancelled load empties a buffer, the cancelled-clear
   terminal sets `set_modified(false)`, so autosave skips — but one subsequent
   keystroke makes it modified again and the next batch would write the
   near-empty buffer over the draft. Whether that is data loss depends on whether
   the draft could have held unsaved edits the file does not, which needs the
   `draft_dirty` transition trace in `drafts.rs`. **Routed to slot 4**
   (`WFR-DRAFT-RECOVERY`), which owns that file; the shape is identical
   pre-change.
2. **Session-restore terminal on a dead editor.** If the planning completion's
   `editor_weak.upgrade()` fails, it returns before `finish_load_planning`,
   leaving the stored terminal unfired; recovery relies on GTK dispose reaching
   `dispose_load_resources`. No path was found where dispose is skipped, but it
   was not proved from the scoped files. Worst case is a stalled sequencer, never
   over-admission or lost content. Unchanged by this refactor.

### A.9b Automation no-widening proof (task 7.5)

Proved by capture and diff, not asserted. `make automation-smoke` was run on the
pre-change tree (a detached `git worktree` at `origin/main`) and on the changed
tree, both under isolated headless Mutter and a private D-Bus session with the
same fixture document, and the two artifact sets were compared.

**`tabs[]` — zero differences.**

```
=== tabs[] keys ===
before: ['active', 'document_kind', 'draft_present', 'evicted', 'file_size',
         'index', 'load_state', 'modified', 'path', 'pinned', 'saving', 'title']
after:  ['active', 'document_kind', 'draft_present', 'evicted', 'file_size',
         'index', 'load_state', 'modified', 'path', 'pinned', 'saving', 'title']
KEYS IDENTICAL: True

=== tabs[] value diff ===
ZERO DIFFERENCES
```

Fixture paths were normalized to `<root>/…` because the two trees have different
roots; that is the only normalization applied, and it is about the fixture rather
than the contract. `load_state` reads `"loaded"` on both sides — the same value,
now sourced from `LoadEvidence` instead of re-derived from the widget.

**Readiness — zero differences across all 11 predicates**, which covers the six
that `file-load` gates:

```
predicate universe identical: True
count: 11 -> 11
total differing predicates: 0

app-startup                same=True
file-open-complete         same=True
session-restore-complete   same=True
recovery-restore-complete  same=True
visual-geometry-settled    same=True
accessibility-settled      same=True
```

Each predicate's full record was compared, not just its name: anchor, blocker
list, description, and stability. The `file-load` blocker still appears in
exactly the same six blocker lists, in the same positions.

**Two limitations recorded rather than glossed:**

1. The `file-open-complete` **failure** semantics (a failed load reporting
   `workflow-failure` rather than readiness) is not exercised by this lane's
   fixture, which loads successfully. It is covered instead by
   `current_readiness_failure`'s unchanged code path and by the widget tests that
   assert a failed load's `load_state`/`outcome`. The projection was the risk
   here, and the projection is what the diff covers.
2. The **before** run's warning scan failed on host noise, not app behavior:
   a fresh worktree's first launch raced `org.a11y.atspi.Registry` activation in
   the private session (`Could not activate remote peer ... unit failed`, plus
   three `xdg-desktop-portal-gtk` AT-SPI warnings). The snapshot and predicate
   artifacts it needed to produce were produced, which is what the diff consumes.
   **The after run passed the warning scan outright** — `PASS: no unexpected
   GTK/GDK/Libadwaita/GIO/D-Bus/portal/AT-SPI/filesystem warnings` — and that is
   the side the acceptance criterion applies to.

`make check-automation-docs` and `make automation-client-self-test` both pass.

**The drift gate needed extending, and that is part of this proof.** Two workflow
surfaces now project into one snapshot object (`tabs` carries both `SaveEvidence`
and `LoadEvidence`), which the checker could not express: it matched
`\bevidence\.(field)` with one fixed binding name and keyed the documented map by
snapshot object alone, so each surface would have appeared to project the other's
fields and a genuine rename would have passed. `EvidenceProjection` now carries a
per-projection `binding` (the load projection reads through `load`, the save one
through `evidence`), and the documented map is keyed by evidence type as well.
A new self-test case, `misattributed evidence binding`, points the load surface at
the save binding and requires the check to fail naming `LoadEvidence.inflight`,
so the attribution cannot silently regress.

### A.9c Lane consequences of the module rename, and one artifact trap (task 9.4)

Two lane failures during final verification were **not** product defects, and
both are recorded so a later slot recognises them in one read rather than
debugging them again.

1. **The accessibility smoke's warning allowlist keyed on the old module path.**
   That lane deliberately loads an unreadable fixture to exercise the
   permission-denied path, and
   `scripts/accessibility_warning_allowlist.py` classified the expected
   `tracing::error!` by its module: `...editor_page::load_save`. After the
   rename the line reads `...editor_page::load::execution`, so the lane failed
   with one "unexpected warning" that was in fact the expected one. The
   allowlist was updated to the new module path, and the classifier was
   re-verified to still **reject** both an unrelated path and the stale module
   name, so it did not become a blanket match:

   ```
   expected line allowlisted : True
   unrelated path allowlisted: False
   stale module allowlisted  : False
   ```

   This is the kind of coupling a module rename creates that no compiler
   catches. A later slot renaming a module that logs should grep the smoke
   allowlists for its old path.

2. **A stale cross-worktree artifact made `make visual-geometry-smoke` fail 80
   of 80 cases.** Every case reported
   `internal session failed: gsettings set show-minimap failed` with
   `No such schema "dev.cominotti.lushtext"`. The cause was not this change:
   `cargo-gtk-proof` resolves its schema directory with
   `env!("CARGO_MANIFEST_DIR")/../../data`, **baked at compile time**, and the
   binary in the shared `target/` had last been compiled from a temporary
   worktree (used to ship an unrelated doc fix) whose `data/` directory no
   longer existed. Rebuilding the tool from the main tree fixed all 80 cases.

   The durable lesson: **sharing one `CARGO_TARGET_DIR` across git worktrees can
   bake a removed path into any binary that uses `env!("CARGO_MANIFEST_DIR")`,
   and the resulting failure names a schema, not a path.** If a proof or smoke
   lane fails wholesale right after work in a second worktree, suspect the
   artifact before the change.

3. **One load-amplified flake, root-caused and fixed rather than retried away.**
   The final full-suite rerun reported
   `editor_page::test_minimap_native_viewport_effect_reprojects_after_mid_file_scroll`
   failing with `condition was not met within 3s`, then passing on the
   whole-suite retry — `make test` exited 0. Under
   `.agents/rules/preexisting-blockers.md` that recovered flake is a blocker, not
   noise, so it was investigated rather than accepted:

   - **Not caused by this change, and not a real break.** The test is absent from
     this change's diff, performs no load at all (it sets buffer text directly),
     and passed **3/3 in isolation** through the same headless harness.
   - **But this change is what made it manifest**, which is the part that matters.
     The migration added several multi-megabyte load tests to the same module, so
     the parallel harness now runs heavier while this test's window is open.
   - **The cause is a mis-budgeted wait, not the machine.** The test waits on
     `scroll_to_mark`, which settles through the GTK frame clock, and
     `.agents/rules/build.md` classifies that as the generous-budget class
     (≥5–10s) precisely because "a larger ceiling costs nothing on the fast path
     and only matters when a loaded machine delays the work". It had 3s, and the
     shared `wait_for_minimap_ready` helper it calls first had 2s.
   - **Fix:** both raised to 10s, with the budget class named at each site. The
     shared helper was fixed rather than the one call site, so every minimap test
     that waits on realization gets the same correction. No predicate, no
     mechanism, and no production code changed — `wait_until` already drains
     ready main-loop sources each poll, so this was not the aliased-polling
     class.
   - **Proof:** the `editor_page::` module passes cleanly after the fix, and the
     **full widget lane was rerun with no suite retry at all** so a recurrence
     would surface as a failure rather than be masked.

   Deliberately not done: the file holds many other 2s waits. They have no
   failure evidence, several are synchronous UI-state flips where a long ceiling
   would hide a real bug, and rewriting them blind would be scope creep. Only the
   two waits on the failing path were changed.

### A.10 Shared-field ownership inherited from 3a (task 3.5)

| Field / group | Owner after 3b | Crossing operation |
| --- | --- | --- |
| the load cancellation the save path triggers | **load** owns `cancel_load` and all `imp().load*` state; **save** owns the *decision* to call it | save reaches it through `policy::save_may_preempt_pending_load` plus the load facade's public `cancel_load()`. No load state moved into `save/`, and no save state moved into `load/` |
| `size_check` ("size classification from the last file load", read by the save path) | **neither** — shared editor-page document metadata, now in `ui/editor_page/document_identity.rs` | the public `size_check()` getter moved there with the other identity accessors. Load's publish stage and save's accept terminal both write `imp().size_check` directly, exactly as before |
| `ViewInteractivityState` | **load**, collected here as 3a directed | stays a private field of `LoadInstallationState` in `load/execution.rs`. Save's `SaveViewInteractivity` twin is **deliberately not merged backward**: importing load's type would recreate the `save → load` dependency 3a avoided |
| `set_file_path_for_pending_load` | **neither** — shared document identity, corrected during review | 3a handed it to load, and it first landed on the load facade. Review found that wrong twice over: it is a *stage body* on a facade (a generation advance, six imp writes, three GTK refreshes), and a near-duplicate of `set_file_path_with_canonical`. It now lives in `ui/editor_page/document_identity.rs` beside its twin, both sharing one private `republish_document_identity` tail, and the facade only narrates that load **calls** it. That removed the convention exception and the duplication together, and shrank the facade from 273 to 253 |
| `set_file_path` / `set_file_path_with_canonical` / `reapply_language` | **neither** — shared document identity, also used by rename, minimap, encoding, accessibility, and local history | moved to `ui/editor_page/document_identity.rs`. Save reaches identity adoption through its own `adopt_saved_destination`; load calls `reapply_language` from its publish stage |
| **the restore-position group** (`set_restore_position`, `cursor_position`, `visible_top_line`, `apply_restore_position`) | **cross-cutting — five owning workflows**, and it **MUST NOT** live in `load/` | moved to `ui/editor_page/restore_position.rs`, a shared `ui/editor_page/` location, with the five owners named in its module doc. Load calls `apply_restore_position` once, from its publish stage, and owns none of it. It moved only because `load_save.rs` no longer exists |

**The double `cancel_load()` in `save/admission.rs` — decided, per 3a's
handover.** `queue_save_request` calls it at `:101` inside the
load-in-progress branch and again at `:120-122` before ownership is published, so
a Save As during an in-flight load advances the load generation by **two**.

**Decision: keep both calls. They are not the same call twice — they run on
opposite sides of the refusal gates and observe different state.** 3a flagged the
generation double-bump, which is real; reading the ordering makes the stronger
point:

- The **first** call is at `:101`, *before* the `installation_incomplete` gate at
  `:107`. Cancelling a live chunked installation is exactly what **sets**
  `installation_incomplete` (`retirement::abort_installation`'s
  `BeginCancelledClear` branch), because the cancelled path deliberately empties
  the buffer. So the first call's real job is to make the gate at `:107` fire:
  the save is refused with `IncompleteLoadInstallation` rather than proceeding.
- The **second** call is at `:120-122`, *after* all four gates. It covers the
  path where `load_state` is not `Loading` — most importantly during
  finalization, where `cancel_load` withdraws a reload a file-loaded callback
  queued reentrantly and returns without bumping the generation.

**Collapsing them would be a data-loss-shaped change, not a cleanup.** Removing
the first call lets a Save As with an in-flight chunked install pass the
`installation_incomplete` gate and *then* cancel the install — queueing a write
of a buffer that is about to be deliberately emptied. Removing the second leaves
a reentrantly-queued reload running behind an admitted save. And merging them at
either position changes the observable load generation, which
`SaveCompletionTicket` captures.

A comment at the site now states both the ordering dependency and the generation
semantics, so a future reader does not delete it as duplication. That is the
decision 3a asked this change to make, made rather than deferred.

### A.11 Facade measurement (task 8.2)

**The budget held. No escalation, no parked state, and the budget line was not
edited.**

| Facade | Measured | Budget | Margin |
| --- | --- | --- | --- |
| `ui/editor_page/load/mod.rs` (this change) | **253** | 370 | 117 under |
| `ui/editor_page/save/mod.rs` (slot 3a) | 223 | 370 | 147 — **untouched by this change**, re-measured to confirm |
| `ui/search_panel/mod.rs` (exemplar) | 369 | 370 | 1 — **untouched by this change**, re-measured to confirm |
| `ui/command_palette/mod.rs` | 335 | 370 | 35 |

Raw physical lines, comments and blanks included, which is what
`make check-workflow-boundaries` counts.

**The response order in task 8.2 was not needed, and the reason sharpens slot
3a's data point.** This facade narrates **one** stage order with **seven**
inversions and **seven** distinct entry points — two more inversions and four
more entry points than the save facade — and still fits with 117 lines spare. Two
independent data points now agree: **what stresses the budget is the number of
stage orders**, not the inversion count, the entry-point count, the workflow's
size, or its risk tier. The 314 lines of install slicing are stage *body* and
went to `execution`, exactly as slot 2a's finding predicted. Slot 6 (minimap)
remains the slot most likely to prove the number wrong.

### A.12 Benchmark comparison (task 9.8)

Criterion baseline saved on a detached worktree at `origin/main`
(`--save-baseline slot3b-before`) and compared from the changed tree
(`--baseline slot3b-before`), sharing one `target/criterion` store so both sides
come from one toolchain.

`transient_file_load` — this workflow's own benchmarks:

| Benchmark | Change | Verdict |
| --- | --- | --- |
| `admission/many_small_512` | +1.30% | noise |
| `admission/concurrent_large_8` | −0.30% | noise |
| `admission/exclusive_near_supported_limit` | −0.38% | noise |
| `admission/stale_queued_1024` | +0.86% | noise |
| `install_boundaries/unicode_50_mib` | **−0.73%** | slightly faster |

`editor_file_io` — the read/decode/write path: every result within ±4%. Criterion
flagged `analyze_windows1252_lossless/10MB` as "regressed" (+3.6%) and
`analyze_shift_jis_lossy/10MB` as "improved" (−1.8%), both in **encoding analysis
code this change does not touch**, and the 1 MB variant of the same
`analyze_windows1252_lossless` benchmark moved the *other* way (−2.8%). That
signature — same function, opposite directions at different sizes — is
shared-runner noise, not a regression.

This is the expected result: the change is confined to `ui/`, and these
benchmarks exercise `model/` and `services/`. The performance claim that actually
matters for this workflow is the install-slicing linearity measurement, which is
recorded separately in
`openspec/changes/migrate-document-load-workflow-readability/evidence/install-slicing-linearity.md`,
and `make performance-smoke` passed including its two load-specific proofs
("headless transient file-load responsiveness proof" and "bounded editor-load
slice cancellation proof").

**Task 8.3 — the other facades' headroom is intact.** `git diff --stat` lists
neither `ui/search_panel/mod.rs` nor `ui/editor_page/save/mod.rs` as changed by
the load migration; both were re-measured above. (`save/mod.rs` and
`save/policy.rs` do carry a separate, doc-comment-only rustdoc-link fix that
shipped as its own commit before this change; it changed no code and moved
`save/mod.rs` by zero net lines.)
