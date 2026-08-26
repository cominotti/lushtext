## 0. Gates, orientation, and premise re-verification

- [x] 0.1 **Tier-3 proof gate — blocking.** Confirm mechanically, not by reading
      the proposal: slots 1, 2a, and 2b are archived under
      `openspec/changes/archive/2026-08-25-*` with their deltas merged into
      `openspec/specs/`; `docs/workflow-readability-matrix.md` marks
      `WFR-SEARCH-REPLACE` and `WFR-COMMAND-PALETTE` `migrated` with complete
      `Migrated Workflow Roles` subsections; `docs/next/workflow-readability.md`'s
      ledger marks slots 1, 2a, and 2b `complete`; and
      `make check-workflow-boundaries` passes on a clean tree. Also confirm the
      three slot-2a deliverables this change depends on are present: the declared
      facade budget line in the matrix, the stage-order qualification rule in
      `openspec/specs/gtk-adapter-module-boundaries/spec.md`, and the working
      evidence-to-snapshot drift check in `scripts/check-automation-docs.py`. The
      convention requires two completed lower-risk proofs before a tier-3
      workflow; three exist. Do not proceed if any of the above is a claim rather
      than a fact.
- [x] 0.2 Read `docs/next/workflow-readability.md` end to end — the status line,
      section 2's baseline tables (including "Baseline after slot 2b"), section 3's
      remaining-scope table and slot ledger, both "Convention friction" sections,
      section 7's programme-level deferrals, and section 8. Read
      `docs/workflow-readability-matrix.md` rows `WFR-DOCUMENT-SAVE`,
      `WFR-DOCUMENT-LOAD`, `WFR-AUTOMATION-SPINE`, `WFR-BUFFER-SNAPSHOT`,
      `WFR-PLAIN-DISPOSAL`, `WFR-EDITOR-MEMORY`, and `WFR-DRAFT-RECOVERY`, plus
      the `Measurement Definitions`, `Policy Module Census`, `Test Seam Census`,
      `Seam Value Objects`, `Settled Conventions`, `Facade size budget`,
      `Migrated Workflow Roles`, and `Completion Rule` sections. Read all five
      capability specs. Read slot 2b's handoff notes (its task 11.2 output, now in
      the record's "Convention friction slot 2b hit" section): they record the
      friction this change inherits, including that `journal` will keep looking
      applicable and that a readability slot over a durable path is where
      pre-existing data-loss defects surface.
- [x] 0.3 **Premise re-verification — do this before writing any code.** Every
      slot so far found stale matrix facts. Re-derive each figure below against
      the tree and record the result in Appendix A.2, including "unchanged" where
      it is unchanged. Where a figure is wrong, correct the matrix in task 10 and
      work from the corrected number, not from this file.
      - `ui/editor_page/load_save.rs` line count and the save/load line split.
        Authoring measured **1,795 lines with zero `#[cfg(test)]` lines** — all
        coverage is external, in `crates/lushtext/tests/widget/**` and
        `crates/lushtext-core/tests/properties/file_load.rs` — so a split cannot
        "move tests" and the whole file is production surface.
      - `model/save_admission.rs` size and reference set. The census cell records
        **405 lines, 2 consumer files** (`save_runtime.rs`, `load_save.rs`).
        Authoring found **five referencing files**: those two plus
        `model/mod.rs`, `crates/lushtext-core/benches/benchmarks.rs` (which
        addresses `SaveAdmissionSnapshot` directly at `benchmarks.rs:3752`), and
        the widget tests. Slot 2b's `model/workspace_search.rs` finding was the
        same shape, so re-derive rather than assume.
      - The row's `Current size` cell (`7 files, 6,672 lines (ui 2,132 / model
        991 / services 3,549)`). Authoring's production save surface is smaller
        than the `ui` subtotal implies. Re-derive the file set and correct the
        cell.
      - The row's `Seams (i/c/a/p)` cell (`10/11/9/4 = 34 fns, 44 sites, 5
        override statics`). Authoring counted, in the save-relevant files:
        `load_save.rs` 18 `_for_test` functions of which roughly 7 are save-side,
        `save_runtime.rs` 2, `services/editor_io.rs` 14 (save and load mixed),
        `services/durable_write.rs` 3, `services/filesystem/write.rs` 1,
        `ui/window/dialogs.rs` 7, `ui/window/documents.rs` 3; and 19 / 3 / 22
        `#[cfg(feature = "test-utils")]` sites in `load_save.rs` /
        `save_runtime.rs` / `editor_io.rs`. Some of that population is shared with
        `WFR-DOCUMENT-LOAD` (3b) and `WFR-DRAFT-RECOVERY` (slot 4). Produce a
        **row-scoped** count with the shared population named, the way slot 2a had
        to for the palette.
      - The four inversions the matrix's `WFR-DOCUMENT-SAVE` stage trace records.
        Slot 2a's finding stands: **census inversion counts are floors, not
        totals.** Write the facade narration from the code and correct the trace.
- [x] 0.4 **Confirm the archetype defect is still live and record it verbatim.**
      Authoring found the boolean stored as `cancel_pending_load` in
      `save_runtime.rs`'s `QueuedSave` and `SaveSubmission`, forwarded positionally
      into `queued_save_is_current`'s `explicit_destination` parameter from
      `load_save.rs` (inside `begin_admitted_save`), from `save_runtime.rs`'s
      stale-drain check, and into `begin_admitted_save` from `save_runtime.rs`'s
      admission handoff. Quote the definition and all forwarding call sites with
      current line numbers in Appendix A.3. If the code has moved since authoring,
      record where it is now; do not work from these line numbers.
- [x] 0.5 **Read the code before changing it** and write the current ordered
      stages into Appendix A.4: `load_save.rs`'s save half, `save_runtime.rs`,
      `model/save_admission.rs`, the save and durability paths in
      `services/editor_io.rs` and `services/durable_write.rs`, and the window-side
      invocations in `ui/window/dialogs.rs` (Save As chooser),
      `ui/window/documents.rs`, and `ui/window/imp.rs` (close-with-changes and
      autosave-on-close). Name **every** inversion and its resumption point,
      including the ones the census trace does not list.
- [x] 0.6 Invoke the `data-safety` skill in explicit mode over the intended diff
      surface before writing code, and again in task 11 over the actual diff.
      Record both in Appendix A.9. Slot 2b's readability pass over a durable path
      produced two confirmed pre-existing data-loss findings and
      `.agents/rules/preexisting-blockers.md` made fixing them non-negotiable
      despite the change's own "no behavior change" non-goal. **Budget for the
      same here and do not treat the non-goal as permission to defer one.**
- [x] 0.7 Grep this workflow's tests for `\.imp()\.` reach-through, not only for
      `_for_test`. Slot 2a's lesson: an ungated reach-through appears in no seam
      census yet shapes production field layout. Authoring found save-related
      sites in `crates/lushtext/tests/widget/window.rs` (direct writes to
      `editor.imp().save.inflight`, reads of `window.imp().session.save_failed`,
      a write to `session.active_close_save_identity`) and in
      `crates/lushtext/tests/widget/editor_page.rs`. Produce the full site list
      with a per-site categorization (inspection → migrates to evidence;
      actuation → classified and preserved) in Appendix A.7.
      **Scope it to this row.** The strict save/load reach-through count is **13
      sites: 9 writes, 3 reads, and 1 widget actuation.** A further 11 sites
      touching `drafts.*` / `session.*` belong to `WFR-DRAFT-RECOVERY` (slot 4):
      catalogue them in the appendix and **hand them to slot 4 in task 12.2
      rather than migrating them here**, the same way slot 2a catalogued the
      palette seams that belonged to slot 5.

## 1. Register the slot split

- [x] 1.1 Split slot 3 in `docs/next/workflow-readability.md`. In the
      remaining-scope table replace the single slot 3 row with `3a` (save,
      `WFR-DOCUMENT-SAVE` plus continuing `WFR-AUTOMATION-SPINE` projections) and
      `3b` (load, `WFR-DOCUMENT-LOAD` plus continuing projections). In the
      machine-readable ledger replace the slot 3 line with two labelled lines,
      per the grammar the record states for letter suffixes:
      `- slot 3a (outstanding): WFR-DOCUMENT-SAVE, WFR-AUTOMATION-SPINE` and
      `- slot 3b (outstanding): WFR-DOCUMENT-LOAD, WFR-AUTOMATION-SPINE`.
      **Slots 4 through 7 are not renumbered** — their numbers are cited from the
      matrix and from per-row `Slot` cells.
      Also fix the record's stale lead sentence in section 3, which still reads
      "Six changes remain (2b and 3 through 7)" now that 2b is complete: it
      becomes "(3a, 3b, and 4 through 7)". While there, give the two-proof
      sentence immediately after it a pass — it says every `tier-3` slot follows
      at least two completed lower-risk migrations "which slot 2a is the second
      of", which was written before 2b landed and reads oddly now that three
      migrations precede this one.
- [x] 1.2 Register both change names in the record's naming table, so the next
      cold session can find them without searching:
      `3a` → `migrate-document-save-workflow-readability`,
      `3b` → `migrate-document-load-workflow-readability`. State that 3a lands
      before 3b and why (shared `load_save.rs`; 3a's delta establishes the
      per-workflow role home 3b reuses).
- [x] 1.3 Record the split rationale in the record beside slot 2's, briefly: two
      independently tier-3 workflows rather than one workflow with a tier-3 half,
      scale relative to slot 2b, a shared 1,795-line file that splits cleanly and
      sequentially, and the archetype defect being save-side.
- [x] 1.4 Update the matrix's `Slot` cells: `WFR-DOCUMENT-SAVE` → `3a`,
      `WFR-DOCUMENT-LOAD` → `3b`, and the "Migration Order And Risk Tiers" table's
      slot 3 row split the same way. Run `make check-workflow-boundaries` after
      this task alone: the gate compares the ledger against the matrix, so a
      half-applied split fails loudly here rather than at the end.

## 2. Close the role-home adjacency

- [x] 2.1 Confirm the collision is real and mechanical before amending anything.
      Record in Appendix A.5: how many workflows `ui/editor_page/` hosts (the
      matrix says eight), that the convention fixes the role file names
      `policy.rs` and `evidence.rs` at one each per workflow, that
      `.cargo/mutants.toml`'s `examine_globs` reaches pure policy through the
      literal `crates/lushtext-core/src/ui/**/policy.rs` so a prefixed
      `save_policy.rs` would leave the default mutation scope, and that
      `openspec/specs/mutation-testing/spec.md` treats a policy module outside
      scope reach as a blocking failure rather than accepted debt. Also record
      that the census already assumed the answer by writing
      `ui/editor_page/minimap/policy.rs` as the minimap's relocation target while
      writing `ui/editor_page/policy.rs` for save's — one file for eight
      workflows, which cannot be right.
- [x] 2.2 Apply this change's `gtk-adapter-module-boundaries` delta: the
      per-workflow subdirectory as a **permitted** role home, role files inside it
      staying unqualified, and flat workflow-scoped names still permitted where
      they do not collide. That is the delta's whole content. Do not add a role
      name, do not touch the stage-order qualification rule (slot 2a owns it), do
      not touch the `journal` definition (slot 2b owns it), and do not touch the
      facade budget.
- [x] 2.3 Verify the nested path is actually reachable by the tooling rather than
      assuming the glob matches: confirm `make check-workflow-boundaries` accepts
      `ui/editor_page/save/policy.rs` as both a purity-checked and a
      mutation-reachable policy module, and confirm `cargo mutants --list` (or
      `make mutants-list`) generates mutants for it after the move. A silent
      no-match here would look like a clean run while deleting coverage.
- [x] 2.4 **Retroactive-amendment obligation.** Re-check every row the matrix
      marks `migrated` against the amended requirement and record the verdict
      **per row** in the matrix's amendment section, following the format slot 2b
      used for its own re-check. At this point that is `WFR-SEARCH-REPLACE` and
      `WFR-COMMAND-PALETTE`; both own dedicated directories with `mod.rs` facades,
      so the expected outcome is two confirmations and zero renames. Adding a
      permitted location cannot invalidate a correct existing location — but if a
      row's declared home is genuinely wrong under the amended text, fix it here,
      because two generations of the convention must not coexist in the tree.
      Also re-confirm in the same section that the other settled conventions are
      unchanged by this amendment: the facade budget stays 370 with both migrated
      facades measured against it, the bounded role set is unchanged, the seam
      value-object shape is unchanged, and the evidence-surface visibility rule is
      unchanged.
- [x] 2.5 Update standing guidance so none of it still implies flat role names are
      the only option: `.agents/rules/rust.md`'s "Workflow Vocabulary And
      Boundaries" section, the matrix's "Role file names" table, and any
      `.agents/skills/**` reference that enumerates role placement. Run
      `make check-agent-docs` and `make check-agent-skills`.

## 3. Design the save/load extraction seam

- [x] 3.1 From the code, record the save workflow's cohesive coordination jobs and
      map each to a bounded role name. Authoring's expectation, to be confirmed or
      corrected rather than assumed: `save_runtime.rs`'s queue, drain, and stale
      rejection is an **admission** job; the capture-dispatch-write-accept
      sequence in `load_save.rs`'s save half is an **execution** job. Check
      `journal` before proposing anything new — the record predicts it will look
      applicable in slot 3 — and record why it does or does not fit. A save that
      replaces file bytes is not a durable record a later stage reads back; the
      draft that protects it is `WFR-DRAFT-RECOVERY`'s (slot 4), so do not pull
      draft persistence into this change to justify a role name. If a genuinely
      novel job appears, **stop and escalate**: adding a role name is a spec
      amendment that now costs a re-check of three migrated rows, and this change
      already carries one amendment.
- [x] 3.2 **Name the shared state across the save/load cut and give each field one
      owner.** This is slot 2b's hardest lesson: the two halves of a split are
      rarely state-disjoint, and the split must be designed around the handoff
      rather than assume it away. **The file's regions interleave**, so this list
      is the extraction unit, not a line range. Authoring found at least these
      entanglements in `load_save.rs`, each needing a recorded owner and, where it
      crosses, a named crossing operation:
      - the save path's cancellation of an in-flight load (the
        `cancel_pending_load` semantic) — a save-side action reaching load-side
        state;
      - `size_check`, documented as "size classification from the last file
        load" but read by the save path;
      - **the restore-position group** (`set_restore_position`,
        `cursor_position`, `visible_top_line`, `apply_restore_position` at
        1469-1530, called from line 814 inside load's completion). **This one is
        neither save nor load state and MUST NOT move into
        `ui/editor_page/save/`.** It has three further owners outside both
        workflows: `ui/window/session_persistence.rs` (`WFR-SESSION-RESTORE`,
        slot 4), `ui/window/search.rs` (`WFR-EDITOR-FIND`, slot 7), and
        `ui/window/notes/bookmarks.rs` plus `ui/editor_page/bookmarks.rs`
        (`WFR-NOTES-BOOKMARKS`, slot 5). Cross-cutting eligibility counts
        **owning workflows**, and this has five, so it stays in a shared
        `ui/editor_page/` location with its ownership recorded and the save
        workflow reaches it through a named operation. Add it to task 3.4's
        boundary list too;
      - `ViewInteractivityState` (43-47), which sits inside the save cluster while
        being a field of the load-side `LoadInstallationState` — load state living
        in save's neighbourhood, so it moves with 3b, not here;
      - `set_file_path_for_pending_load` (1215-1228), load-only but sitting inside
        the save tail;
      - save-side seams at 1058-1143 sitting inside the load region, and four
        save-side `_for_test` hooks interleaved into the load-side hook run.
      For each: state which workflow owns it after 3a, and if 3a cannot own it
      because it is load state or cross-cutting state, state the named operation
      the save side calls and leave the field where 3b will find it. **Do not**
      move a load-owned or cross-cutting field into `ui/editor_page/save/` for
      tidiness; that would hand 3b a worse seam than it has today.
- [x] 3.3 Record the residual-file decision: `load_save.rs` keeps its name after
      3a, holding the load half only, with a module-doc line stating that the save
      half migrated to `ui/editor_page/save/` and the load half is slot 3b's. It
      is not renamed, because 3b dissolves it and renaming a file two consecutive
      changes touch is churn on a tier-3 path — the same reasoning slot 2b used to
      leave `execution.rs` unrenamed. Record it so a reader between 3a and 3b is
      not misled by the stale name.
- [x] 3.4 Confirm the boundaries this change must not cross, and record them:
      `ui/buffer_snapshot` (`WFR-BUFFER-SNAPSHOT`, cross-cutting, slot 7),
      `ui/plain_disposal` and `model/plain_disposal.rs` (`WFR-PLAIN-DISPOSAL`,
      cross-cutting, slot 7), `model/editor_memory.rs` (`WFR-EDITOR-MEMORY`,
      exempt, no slot), `services/draft_service.rs` and `ui/window/drafts.rs`
      (`WFR-DRAFT-RECOVERY`, slot 4), `model/file_load.rs` plus `load_runtime.rs`
      (3b), and **the restore-position group from task 3.2**, whose five owning
      workflows make it cross-cutting editor-page state rather than save state.
      The save workflow **calls** these; it does not own or restructure them.

## 4. Extract the save workflow into role-named modules

- [x] 4.1 Create `ui/editor_page/save/` with the roles decided in task 3.1: a
      facade `mod.rs`, the coordination modules, `policy.rs`, and `evidence.rs`.
      Declare the module in `ui/editor_page/mod.rs`. Retire `save_runtime.rs`:
      `runtime` is the name the convention rejects, and the census found it naming
      three different jobs across four files.
- [x] 4.2 Move the save half of `load_save.rs` and the whole of `save_runtime.rs`
      into the new modules, assigning each function exactly one role. The facade
      must own no timer, admission bookkeeping, generation counter, or GTK widget
      mutation; the coordination modules own the `thread_local` coordinator, the
      queue and drain, the buffer-capture handoff, the worker dispatch, and the
      completion acceptance. `write_snapshot_async` is the largest function in the
      file at roughly 172 lines — it is a stage body, so it belongs in
      coordination, and the facade narrates it in one stage with its resumption
      point named.
- [x] 4.3 Make the window side delegate, following slot 2b's window-side fix. The
      save, Save As, close-with-changes, and autosave-on-close invocations in
      `ui/window/dialogs.rs`, `ui/window/documents.rs`, and `ui/window/imp.rs`
      must call one named save operation per step instead of re-reading and
      re-mutating editor save state inline. Keep the generation and freshness
      guards on the editor side where they already live. Record every window-side
      site that changes, and whether the guard moved (it should not).
- [x] 4.4 Confirm the extraction is behavior-neutral at the API level: no changed
      public or `pub(crate)` signature semantics beyond the ticket introduction in
      task 5, no changed call order, no new or removed `spawn_blocking_then`
      boundary, no changed timer interval, and no changed notification. Where a
      rename lands on a cross-module operation, name it for the workflow intent
      per the intent-first naming rule, and record the old → new mapping so
      reviewers can diff behavior rather than names.

## 5. Reify the admission seam and kill the renamed value

- [x] 5.1 Introduce `QueuedSaveTicket` + `QueuedSaveFacts` with one
      `queued_save_is_current(&ticket, &facts)` predicate, following the
      Ticket + Facts + predicate shape the matrix's Seam Value Objects section
      records and the exemplar's `ReplacePreviewTicket` + `ReplacePreviewFacts`
      demonstrates. Carry `{save_generation, path, explicit_destination,
      required_modified, close_session_identity}`. Construct the ticket **once**,
      at the workflow entry point, and validate it as a unit — not clause by
      clause at each call site. `SaveCompletionTicket` already exists for the
      completion seam and keeps its `is_current(&editor)` shape; the two are
      distinct seams and both stay.
- [x] 5.2 **The field is named `explicit_destination`.** The matrix states this
      normatively: it names the user's intent, where `cancel_pending_load` names
      only a consequence. Do not carry both names for one value.
- [x] 5.3 **Decide, from the code, whether one value can honestly carry both
      meanings**, and record the answer in Appendix A.3 with the reasoning. The
      predicate uses it to decide whether to compare the queued path against
      `file_path()`; the queue stage uses it to decide whether to cancel an
      in-flight load. If every caller that wants one wants the other, keep one
      field named `explicit_destination` and give the cancellation site a named
      derivation so the inference is visible in the code rather than implied by a
      shared bool. If they can diverge for any caller — plain save, close-save,
      autosave-on-close, Save As, or a session-restore-driven save — they are two
      fields, and the change must say which callers set which. **State the failure
      mode either way**: a save wrongly claiming an explicit destination skips the
      stale-target path comparison, and a Save As that stops cancelling the
      pending load races a load into a just-saved buffer. Both are data-safety
      outcomes, so this decision belongs in the `data-safety` pass, not only in
      review.
- [x] 5.4 Remove `begin_admitted_save`'s
      `#[expect(clippy::too_many_arguments)]`. The matrix's "Argument-count
      suppressions" section names it as the one suppression in workflow code and
      names this ticket as the mechanism. Confirm the workspace count drops from 2
      to 1 and that the remaining one is the domain catalog constructor the rule
      exempts. Treat a surviving suppression on a cross-module workflow boundary
      as an unreified seam, not an accepted exception.
- [x] 5.5 Add a test that the mismatch can no longer be reintroduced silently:
      construct a ticket whose `explicit_destination` and current path disagree
      and assert the predicate's verdict, so a future positional edit is a type
      error or a failing assertion rather than an invisible rename.
- [x] 5.6 Report **seams reified** as this change's primary unit, per the record's
      instruction, and report long signatures only as a secondary figure stating
      which definition it uses (receiver-counted 88 or strict 43). Unlike the
      three previous slots, this change is expected to shorten at least one
      genuinely long signature, so the secondary figure is informative here for
      the first time — say so.

## 6. Pure policy: relocate, extract, and prove parity

- [x] 6.1 Relocate `model/save_admission.rs` (405 lines at authoring) to
      `ui/editor_page/save/policy.rs`. Confirm it contains no `gtk4`, `glib`,
      `gio`, `libadwaita`, or `sourceview5` import after the move, and that its
      co-located unit tests move with it.
- [x] 6.2 Handle the benchmark consumer explicitly.
      `crates/lushtext-core/benches/benchmarks.rs` addresses
      `SaveAdmissionSnapshot` directly, so the relocated module needs the same
      `pub` treatment `ui/search_panel/policy.rs` already carries for its
      GTK-free policy benchmarks. Record that this is existing precedent rather
      than a new pattern, and that a bench consumer is **not** a
      dependency-direction problem the way `services/` was for
      `model/workspace_search.rs`. **Scope the exposure precisely: `pub` only the
      items `crates/lushtext-core/benches/benchmarks.rs` actually imports, and
      `pub(crate)` for everything else.** The palette's policy module over-exposed
      22 items on the strength of one bench consumer; do not copy that. Confirm
      `make bench` still compiles (the
      `Bench Compile` CI job covers this) and that no `services/` or `model/`
      file depends on the relocated module afterwards — if one does, the move is
      forbidden and the finding must be recorded like slot 2b's.
- [x] 6.3 Extract the save half's remaining pure decisions from the GTK adapter
      into the same `policy.rs`: the save-formatting acceptance rule, the buffer
      mirror-back decision, the chunked-versus-direct capture threshold
      (`live_buffer_requires_chunked_snapshot` and its callers), and the
      queued-save staleness predicate. None of that logic is under mutation today,
      so this is a coverage **gain from zero**, not a relocation. Report it
      separately from task 6.1's parity numbers; slot 2a and 2b both had to make
      that distinction and mixing them makes both claims unreadable.
- [x] 6.4 Keep the boundary honest about what is not pure: anything touching the
      GTK buffer, the disposal lane, `TargetWriteGuard`, or filesystem metadata
      stays in coordination. `policy.rs` purity is what keeps it in the mutation
      scope, and `make check-workflow-boundaries` fails on a single GTK-family
      import.
- [x] 6.5 Decide `services/editor_io.rs`'s and `services/durable_write.rs`'s
      buried pure policy explicitly, following slot 2b's
      `services/search_backup.rs` precedent. Rules that are pure once given their
      inputs — save-encoding selection, the write-outcome classification into
      `BeforeRename` / `AfterRename` / accepted, and the identity-metadata
      preservation decisions — cannot move to `ui/`, because a service must not
      depend on the adapter. Decide whether they become a
      `services/<module>/policy.rs` or stay as private pure functions with direct
      unit tests, and **record the reason**. They are already inside the mutation
      scope through `services/**`, so the win is testability without a tempdir,
      not coverage. **`services/editor_io.rs` is shared with the load workflow, so
      record the decision and the resulting shape in the handoff (task 12.2): 3b
      makes the same choice for this file's load side and must not re-litigate
      it.** If this change creates `services/editor_io/policy.rs`, say so
      explicitly so 3b extends it; if it declines, say why, so 3b can state
      whether load changes the answer.
- [x] 6.6 Classify the write-side test seams in `services/editor_io.rs`,
      `services/durable_write.rs`, and `services/filesystem/write.rs` by kind
      (inspection / configuration / actuation / probe), the way slot 2b classified
      `services/content_search/replace.rs`. The fault-injection seams
      (`fail_next_save_for_path_for_test`, `observe_temp_after_content_for_test`,
      `fail_next_parent_sync_for_test`, the after-metadata hook) **stay**: they are
      the mechanism this change's failure-path verification depends on. Record the
      classification in Appendix A.8 so a later slot does not read them as
      unfinished work. Note that `services/durable_write.rs`'s hook registry
      returns cleanup ownership from registration precisely so parallel tests
      cannot consume each other's injected failure — preserve that property.
- [x] 6.7 Write mutation parity evidence to
      `openspec/changes/migrate-document-save-workflow-readability/evidence/mutation-parity-save-policy.md`,
      following the structure of
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`
      and slot 2b's `mutation-parity-replace-policy.md`: scope re-verification
      with **the exact commands run**, the before/after table, per-survivor
      disposition, the relocation's parity numbers and task 6.3's gain-from-zero
      numbers reported separately, and the merge-base diff workaround if
      `make mutants-diff` cannot see working-tree edits. **Keep the mutant anchors
      coarse — file-level generated/killed counts rather than per-line
      identifiers** — because a later simplification pass will refuse edits that
      invalidate recorded anchors, and a line-precise anchor freezes the file.

## 7. Evidence surface

- [x] 7.1 Create `ui/editor_page/save/evidence.rs` with one accessor that is the
      single source of the workflow's observable state, at the narrowest
      visibility its readers require (the in-crate `ui/automation.rs` and the
      external widget harness, which is the same reader pair both migrated
      workflows have). Fold the existing typed observation in rather than leaving a
      second path: `SaveAdmissionSnapshot` is already typed, and
      `save_runtime::snapshot_for_test` plus the save-side hooks in `load_save.rs`
      are the scattered getters the convention retires.
- [x] 7.2 Expose at minimum: whether a save is in flight; the save generation; the
      admitted ticket's identity fields including `explicit_destination`; the
      queued-save depth and whether a drain is pending; chunked-versus-direct
      capture state and whether a capture is in flight; the last write's outcome
      classification (accepted, `BeforeRename`, `AfterRename`/durability
      unconfirmed); whether save formatting rewrote the buffer and whether the
      mirror-back completed; and the close-session identity when a
      close-with-changes save owns it. Expect slot 2b's lesson to recur: **the
      workflow's observable outcome is often already computed somewhere that
      throws it away.** Where the window computes a save outcome the editor never
      hears about, the honest fix is a named workflow operation that records it,
      not a test getter reaching into the window.
- [x] 7.3 Respect the reentrancy constraint the exemplar's surface documents: the
      accessor takes shared `RefCell` borrows, so no field may be read from inside
      a `borrow_mut()`. Prove it with a test that **drives the workflow through
      each operation taking a mutable borrow of the state the accessor reads,
      reads the evidence surface after each such operation, and asserts that
      repeated reads of unchanged state are identical.** Do not write a test that
      reads the surface *while* a borrow is held — that is the panic the
      constraint exists to prevent, not the proof of it. Slot 2b's test
      (`crates/lushtext/tests/widget/search_panel.rs:3437-3457`) is the reference
      implementation; read it before writing this one. (Slot 3b promotes this
      constraint into `workflow-evidence-surfaces` as a stated convention with
      exactly this proof shape; until then it is a per-workflow module note, and
      this change must still honor it — and 3b's task 1.3 will verify that the
      test written here has the right shape.)
- [x] 7.4 Confirm reading the surface mutates nothing — no timer, queue,
      generation counter, coordinator, admission reservation, or disposal
      reservation — and does not require the workflow to be in a particular stage.
- [x] 7.5 Retire the save-side inspection seams into the surface and confirm the
      retired functions have no remaining callers:
      `save_runtime::snapshot_for_test`, and in `load_save.rs`
      `transient_save_admission_snapshot_for_test`,
      `save_uses_chunked_snapshot_for_test`, and
      `save_snapshot_inflight_for_test` (re-derive the exact list in task 0.3).
      **Do not add a per-field `*_for_test` accessor for anything**: a test needing
      a fact the surface lacks extends the surface. That rule is the one the
      evidence surface exists to enforce.
- [x] 7.6 Collapse the save workflow's configuration seams into **one**
      test-policy value in a `test_policy.rs` that is entirely behind
      `#[cfg(feature = "test-utils")]`, so a production build compiles no override
      storage. Save-side candidates include the write-delay and
      save-failure-injection overrides currently held as module-level statics in
      `services/editor_io.rs`. Where an override must stay in the service because
      the service owns the behavior, say so and keep it there — do not create a
      second policy value in `ui/` that shadows it.
- [x] 7.7 Classify and **preserve** the actuation seams:
      `reset_transient_save_admission_for_test`,
      `pause_next_save_snapshot_for_test`, `resume_save_snapshot_for_test`,
      `save_runtime::reset_for_test`, and the chooser-bound seams in
      `ui/window/dialogs.rs` (`select_save_as_destination_for_test`,
      `select_save_as_uri_for_test`, `cancel_save_as_destination_for_test`). These
      drive steps reachable only through a `GtkFileChooser`, an
      `AdwAlertDialog`, or a worker completion, and they stay per the
      programme-level deferral in the record's section 7. Record the count before
      and after. The count should not grow — but if task 7.8 concludes that an
      ungated write site genuinely needs a **new** named actuation seam, that is a
      **counted, recorded exception stated here with its justification**, never a
      silent increment. An unexplained increase is a failed task; a justified and
      recorded one is a decision.
- [x] 7.8 Migrate the widget tests catalogued in task 0.7. For each **read** site,
      read evidence where the question is "did the workflow record it", and keep a
      direct read only where the question is genuinely "what bytes are on disk".
      For each ungated **write** site — the verified ones are
      `crates/lushtext/tests/widget/window.rs:6085`, `:6096`, `:13830`, `:13986`,
      and `:14005`, writing `save.inflight` and `active_close_save_identity`
      through plain `pub` fields — "keep it as arranging state" is **not** an
      available answer, because a write through a `pub` field is an actuation
      reach-through masquerading as setup and it shapes production field layout.
      Decide per site among three outcomes: it becomes a call to a **named
      actuation seam that already exists** (preferred); it becomes an **evidence
      assertion** because the test was really asserting, not arranging; or it
      becomes a **real drive of the workflow**. If none of the three fits and a
      genuinely new seam is required, that is task 7.7's counted, recorded
      exception. State per site which outcome it took and how many moved, in
      `openspec/changes/migrate-document-save-workflow-readability/evidence/widget-test-save-site-migration.md`,
      following slot 2b's `widget-test-search-backup-site-migration.md`. The
      ungated `editor.imp().save.inflight` writes and
      `window.imp().session.save_failed` reads are the priority: they shape
      production field layout from the test side.
- [x] 7.9 Confirm the project test count has not decreased, recorded before and
      after with the counting method stated, in
      `openspec/changes/migrate-document-save-workflow-readability/evidence/test-counts.md`.

## 8. Automation: project from evidence without widening

- [x] 8.1 Identify this workflow's exported surface exactly and record it: the
      snapshot field `tabs[].saving`, the readiness blocker `save`, and the
      `save-complete` predicate that consumes it. `tabs[].modified` is buffer
      state the row shares rather than owns; `tabs[].load_state` and the
      `file-load` blocker belong to 3b. Check `docs/automation-reference.md`
      rather than trusting this list.
- [x] 8.2 Make those fields project from the save evidence surface instead of
      re-deriving the same state from widgets, with the exported field names,
      types, and semantics **unchanged**.
- [x] 8.3 Add the corresponding rows to the `Evidence Projection Map` in
      `docs/automation-reference.md`. The drift gate slot 2a implemented reads
      that table and fails when a projected evidence field is added, removed, or
      renamed without it — and the table's own rule is that the authority for "is
      this field projected" is the Rust snapshot function. A field the projection
      does not read is internal and must not appear.
- [x] 8.4 Confirm every other new evidence field from task 7 — generations, ticket
      identity, capture state, write classification, queue depth — is **not**
      serialized into any snapshot, and that existing redaction and omission
      behavior for private state (paths, buffer text) is preserved.
- [x] 8.5 Prove no widening rather than asserting it: capture an Automation1
      snapshot for the same app state before and after, and diff the `tabs[]`
      objects and the readiness fields to zero differences apart from the
      documented projection being sourced differently. Run
      `make check-automation-docs` and `make automation-client-self-test`.
- [x] 8.6 Keep `WFR-AUTOMATION-SPINE` `pending` in the matrix and write it
      `WFR-AUTOMATION-SPINE (partial)` on slot 3a's `complete` ledger line while
      keeping it on 3b's `outstanding` line. Omitting it from every outstanding
      line fails the gate; marking the row `migrated` would be a false claim.

## 9. Facade

- [x] 9.1 Write `ui/editor_page/save/mod.rs`'s module-doc narration from the code
      recorded in task 0.5, not from the census trace: the ordered stages with
      their intent named, each delegating to a named role, and **every** inversion
      with the point where control resumes. The matrix records four inversions
      (idle drain, chunked capture, worker write, permit release); slot 2a's
      finding is that census counts are floors, so narrate what the code does.
- [x] 9.2 Include the role table naming which module owns what, and the
      shared-field owners from task 3.2 — including any field the save workflow
      reads but 3b will own — so a reader can tell where the durable write lives
      and where the save/load boundary is.
- [x] 9.3 **Measure the facade and hold it to 370 physical lines.** The response
      order when it does not fit is fixed and is not negotiable at implementation
      time: (a) delegate more work into the coordination modules' own module docs
      — **and note that the check counts raw physical lines, comments and blanks
      included, so moving thin facade *code* into coordination is equally
      permitted, not just doc lines**; (b) keep each stage to intent plus delegate
      plus resumption point; (c) fold module-ownership detail into the role table
      and compress inversion bullets, as slot 2b did to come back from 379 to 369;
      (d) only then treat non-fitting as evidence the number is wrong. **Do not
      edit the budget line.** Raising it is a convention amendment requiring every
      migrated row re-checked in the same change — three rows now — and this change
      already carries one amendment.
      **If (a) through (c) genuinely cannot fit an honest narration, park the
      change in this exact state and escalate to the maintainer.** The parked state
      is operable rather than a half-finished tree: leave the facade at its honest
      measured length; leave the row's `Status` **un-flipped** and its ledger line
      `outstanding`, so `make check-workflow-boundaries` reports a pending
      migration rather than a budget violation and stays green on its own terms;
      record the measured count and the irreducible remainder — which stages,
      which inversions, how many lines each — in Appendix A.11; do not mark the
      surrounding tasks complete; and do not archive. The record says the case for
      correcting the number is cheaper to make now than at slot 6, so make it
      explicitly from that parked state rather than mangling the narration or
      quietly changing the number.
- [x] 9.4 **Protect the exemplar's 1 line of headroom.** The search facade sits at
      369 of 370, so any incidental edit to `ui/search_panel/mod.rs` — a rename,
      an import, a doc touch-up — can break the gate. Confirm this change touches
      it not at all, or re-measure it and record the number if it must.
- [x] 9.5 Cold-read check: with this change's conversation set aside, read only the
      facade and confirm the whole Ctrl+S story and every inversion are followable
      without opening the coordination or policy modules, and that a reader can
      tell where the bytes are written and where a failed write is classified. If
      not, the split in task 4 is wrong and must be revisited before archiving.

## 10. Matrix and record completion

- [x] 10.1 Add the `### WFR-DOCUMENT-SAVE` subsection under
      `Migrated Workflow Roles`, in the documented format: `facade`,
      `coordination`, `policy`, `evidence`, and `mutation parity` naming real
      paths. Note the per-workflow subdirectory as this row's role home and why,
      so the next `ui/editor_page/` workflow copies the boundary rather than
      re-deriving it. Record **this** change's own `mutation parity` pointer in
      the **live** `openspec/changes/<name>/` form.
      **Corrected during implementation: this task originally said the opposite,
      and the opposite does not work.** The boundary checker's tolerance runs one
      way only. Its `claim_exists` probe resolves a live-form pointer against
      `openspec/changes/archive/*-<name>/` as a fallback, so the live form passes
      both before and after archival; it has no reverse fallback, so an
      archive-prefixed pointer on an unarchived change probes a path that does
      not exist yet and fails the gate immediately (verified: the archive-form
      pointer written first made `make check-workflow-boundaries` fail with
      "claims ... but ... does not exist"). Rewriting a pointer to the
      archive-prefixed form is therefore an **optional post-archival** edit for
      human readability — a person following a live-form path after archival
      finds nothing, even though the gate still resolves it — and never a
      pre-archival one. The rule is recorded in the matrix's
      "Evidence pointer form" section so the next slot does not re-derive it.
- [x] 10.2 Fix the two archive-rot pointers already in the matrix while you are
      in this section. `docs/workflow-readability-matrix.md:1125` points at
      `openspec/changes/migrate-command-palette-workflow-readability/evidence/...`
      for the palette's parity evidence, and `:454` points at a live-form slot-1
      path; both changes are archived, so both should take the archive-prefixed
      form the `WFR-SEARCH-REPLACE` row uses at `:1039`. This is a two-line fix
      that prevents a cold session following a dead path, and it is cheapest to do
      from inside this section.
- [x] 10.3 Update the row's cells: `Current size` and `Seams (i/c/a/p)` from task
      0.3's re-derivation, `Owned pure policy` for the relocated and newly
      extracted policy, `Seam value object` from `required:` to the reified
      `QueuedSaveTicket` + `QueuedSaveFacts` with the renamed-field resolution
      stated, `Evidence surface` for the new surface, `Risk` recording that the
      tier-3 write path is now covered, `Slot` `3a`, and `Status` `migrated`.
- [x] 10.4 Update the `Seam Value Objects` section: move the
      `required: QueuedSaveTicket + QueuedSaveFacts` entry to `done:`, state what
      it removed (the positional forwarding, the duplicated clause-by-clause
      freshness comparison, and the argument-count suppression), and record the
      `explicit_destination` versus `cancel_pending_load` resolution from task
      5.3. Leave `LoadRequestTicket` as `required:` for 3b.
- [x] 10.5 Correct the `Policy Module Census`: `save_admission.rs`'s consumer list
      and its relocation target, which the census records as
      `ui/editor_page/policy.rs` and which is now
      `ui/editor_page/save/policy.rs`. Keep a short pointer at the old location so
      a reader following the census snapshot does not think the target is still
      `ui/editor_page/policy.rs`. Leave `model/file_load.rs`'s row for 3b —
      the census flags it as needing an explicit slot-3 decision, and that
      decision is 3b's.
- [x] 10.6 Update the row's `Workflow Stage Traces` entry so the trace names the
      current operations and modules, with the corrected inversion set from task
      9.1 and the shared-field owners from task 3.2.
- [x] 10.7 Update the `Argument-count suppressions` section: the count is now 1,
      and the workflow-code suppression this change removed is named as removed
      rather than pending the residual sweep.
- [x] 10.8 Advance `docs/next/workflow-readability.md`: flip slot 3a's ledger line
      to `complete` with `WFR-AUTOMATION-SPINE (partial)`, keep slot 3b
      outstanding, update the status line, add a "Baseline after slot 3a" table
      reporting workflows migrated, share of censused `ui/` + `model/` lines,
      policy modules relocated (this is the **third** relocation and the first
      since slot 1 — the denominator is 6 candidates), test seams addressed, seams
      reified, automation projections, and the facade budget position with the
      measured save facade line count.
- [x] 10.9 Add a "Convention friction slot 3a hit" section to the record for slots
      3b through 7: whether the per-workflow-subdirectory role home read well,
      whether `journal` was checked and rejected for save and why, whether the
      370-line budget held on a one-stage-order facade and at what measurement,
      whether the shared-field ownership across the save/load cut left 3b a
      clean seam, and whether the `data-safety` pass produced findings the way it
      did in 2b. Four workflow halves are migrated after this change, so the
      retroactive-amendment rule is more expensive again — say so with the row
      count.
- [x] 10.10 Update `AGENTS.md`, `README.md`, and any `.agents/rules/*.md` or
      `.agents/skills/**` reference naming a path this change moved
      (`load_save.rs`, `save_runtime.rs`, `model/save_admission.rs`), plus
      `.cargo/mutants.toml` if a legacy hand-listed UI entry or `exclude_re` entry
      retires with this workflow. The mutants config comment says the remaining UI
      entries "retire as their workflows migrate" — check whether any names a save
      path, and retire it rather than widening anything.

## 11. Verification

- [x] 11.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries` (policy purity, mutation reach, role
      completeness, facade budget, ledger-versus-matrix agreement),
      `check-filesystem-boundary`, `check-automation-docs`,
      `check-accessibility-policy`, and `check-visual-proof-policy`.
- [x] 11.2 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines**. A recovered flake is a blocker per
      `.agents/rules/preexisting-blockers.md`, not accepted noise: read the real
      failure, fix the cause, and rerun in isolation. Attach task 7.9's counts.
- [x] 11.3 `make mutants-diff` clean, with task 6.7's evidence attached and every
      survivor closed by an added test rather than a scope change. An
      unscoped-to-scoped move is a **gain**, and its new mutants must be fully
      killed, not excluded.
- [x] 11.4 The mandatory proof lanes for `ui/` and widget-test changes, each from a
      clean artifact root: `make visual-geometry-smoke`, `make accessibility-smoke`,
      `make visual-smoke`. Order these **after all source edits**, including
      documentation and rules edits: the accessibility policy gate fingerprints
      the contents of accessibility-relevant files, so an edit after the lane runs
      voids the proof and the lane must be rerun.
- [x] 11.5 **Save behavior equivalence**, each case with a test asserting both the
      user-visible outcome and the on-disk bytes: save of an untitled buffer
      through Save As; plain overwrite of an existing file; save of a clean
      unmodified buffer; save with EditorConfig `trim_trailing_whitespace` and
      `insert_final_newline` rewriting the text, where the saved bytes and the
      live buffer must agree before the tab is marked clean (per
      `.agents/rules/rust.md`); a superseded save whose stale completion must
      publish nothing; close-with-changes and autosave-on-close; a save whose
      editor is closed before the worker returns; and a save whose editor changes
      path before the worker returns. The last two are what
      `SaveCompletionTicket::is_current` protects.
- [x] 11.6 **Durability failure-path equivalence** through the existing
      fault-injection seams in `services/durable_write.rs`,
      `services/filesystem/write.rs`, and `services/editor_io.rs`:
      `DurableWriteError::BeforeRename` must still report as an unwritten save with
      the document left modified and the previous bytes intact;
      `DurableWriteError::AfterRename` must still surface as a distinct
      "durability unconfirmed" warning and never as a generic lost save; and the
      after-metadata hook must fire at the same point as before. Also confirm
      identity-metadata preservation across atomic replace (permission bits
      exactly; ownership, ACLs, xattrs best-effort) is unchanged, and that
      `TargetWriteGuard` is still acquired on the resolved target before any read
      or write of the destination bytes.
- [x] 11.7 `make crash-recovery-smoke` clean. Save interacts with draft and
      session recovery state even though those workflows are slot 4's, so a
      regression here would surface as lost user content.
- [x] 11.8 Re-run the `data-safety` skill in explicit mode over the actual diff
      and resolve every confirmed finding. **A tier-3 change does not close with an
      open data-safety finding**, and per
      `.agents/rules/preexisting-blockers.md` a pre-existing one found here is in
      scope rather than deferrable.
- [x] 11.9 **Live run.** Exercise real saves, a real Save As, and a real
      close-with-changes save through the running app, watching stderr for new
      `Gtk-WARNING`, `Gtk-CRITICAL`, `GLib-GObject-WARNING`, pixman `*** BUG ***`,
      or `Trying to measure` output. **Save rewrites files, so this must never be
      pointed at the maintainer's real documents.** Pre-authorized substitution,
      following slot 2b's precedent: run against throwaway fixture files inside an
      isolated `LUSHTEXT_DATA_DIR` and isolated XDG directories (the
      crash-recovery smoke lane's isolation pattern), rather than a plain
      `make run` against the maintainer's live session. Before launching, check for
      a running `dev.cominotti.lushtext` owner and stop rather than racing it —
      `make run` asks an existing owner to quit, and the substituted invocation
      must do the equivalent. Record exactly what was run, the fixture paths, and
      **what remains uncovered** by the substitution, in
      `openspec/changes/migrate-document-save-workflow-readability/evidence/live-run.md`
      with the captured stderr beside it. Do not silently downgrade to a headless
      run and call the item done.
- [x] 11.10 `openspec validate migrate-document-save-workflow-readability
      --strict` clean.

## 12. Handoff

- [x] 12.1 Confirm the programme record and the matrix agree: `WFR-DOCUMENT-SAVE`
      is migrated, slots 1, 2a, 2b, and 3a are complete, slot 3b is the next
      outstanding slot, and `WFR-AUTOMATION-SPINE` is carried onto 3b's line.
- [x] 12.2 Hand 3b the facts it needs so it does not re-derive them: the residual
      shape of `load_save.rs` after the save half left — **including its measured
      line count, since authoring's two estimates of the load half disagree
      (roughly 1,088 counted from 3a's side against roughly 1,046 from 3b's), and
      the post-3a measurement is what settles it**; the shared-field ownership
      decisions from task 3.2 and which of them 3b now owns, including that the
      restore-position group is cross-cutting and that `ViewInteractivityState` is
      load state 3b collects; the `services/editor_io.rs` policy decision and
      shape from task 6.5, which 3b must follow rather than re-litigate; the
      per-workflow-subdirectory precedent and the exact paths; the measured save
      facade line count as a data point for the budget; the reentrancy proof test
      this change wrote and where it lives, since 3b's task 1.3 re-checks its
      shape; the re-derived `model/file_load.rs` reference set if this change
      happened to measure it; and the seam and reach-through counts that turned
      out to be shared between the two rows.
- [x] 12.3 Hand slot 4 the 11 `drafts.*` / `session.*` reach-through sites task
      0.7 catalogued but did not migrate, so `WFR-DRAFT-RECOVERY` inherits a list
      rather than a rediscovery.

---

## Appendix B — handoff

### B.1 To slot 3b (`migrate-document-load-workflow-readability`) — task 12.2

- **Residual `load_save.rs` measures 1,212 lines**, down from 1,795. That
  settles the disagreement between the two authoring estimates (roughly 1,088
  from 3a's side against roughly 1,046 from 3b's): **both were low**, because
  both treated the cross-cutting groups as if they would leave with the save
  half. They did not, and must not.
- **Shared-field ownership** is recorded per field in A.10. The two 3b must
  *not* absorb: the **restore-position group** is cross-cutting with five owning
  workflows and stays in a shared `ui/editor_page/` location; **document
  identity and metadata** (`set_file_path`, `set_file_path_with_canonical`,
  `size_check`) are shared with the rename, minimap, encoding, accessibility,
  and local-history paths. `ViewInteractivityState` **is** load state and 3b
  collects it — the save workflow now has its own `SaveViewInteractivity`, so
  nothing depends on load's copy.
- **The `services/editor_io.rs` policy decision: no `services/*/policy.rs` was
  created, and 3b must follow rather than re-litigate it.** The pure rules there
  stay as private functions with direct unit tests, because `services/**` is
  already inside the mutation scope so a policy module buys no coverage, and
  moving them under `ui/` would invert dependency direction. If 3b finds the
  load side genuinely changes the answer, that is a new decision to state
  explicitly, not a correction of this one. Same for
  `services/durable_write.rs`.
- **The per-workflow subdirectory precedent and its exact paths.** Role home:
  `crates/lushtext-core/src/ui/editor_page/save/` with `mod.rs` (facade),
  `admission.rs` + `execution.rs` (coordination), `policy.rs`, `evidence.rs`.
  3b should use `ui/editor_page/load/` on the same shape. **The nested
  `ui/**/policy.rs` glob is verified reachable** by both cargo-mutants and
  `check-workflow-boundaries` — 3b does not need to re-derive that, only to
  re-verify it after its own move.
- **Facade budget data point: the save facade measures 223 of 370** narrating
  one stage order with five inversions. The budget is not close for a
  one-stage-order facade.
- **The reentrancy proof test** is
  `test_save_evidence_reads_stay_side_effect_free_across_save_mutation` in
  `crates/lushtext/tests/widget/editor_page.rs`. It drives the workflow through
  each operation taking a mutable borrow of the state the accessor reads, reads
  after each, and asserts repeated reads of unchanged state are identical. It
  does **not** read the surface while a borrow is held — that is the panic the
  constraint prevents, not a proof of it. 3b's task 1.3 re-checks this shape.
- **`model/file_load.rs` was not re-measured** by this change beyond confirming
  `load_save.rs` still imports `SYNCHRONOUS_INSTALL_THRESHOLD_BYTES` and
  `next_install_boundary` from it. 3b must derive its reference set itself.
- **Shared seam populations.** `services/editor_io.rs` holds 10 unique
  `*_for_test` names of which **6 are load-side** (`set_load_delay_for_test`,
  `set_payload_load_delay_for_test`, `delay_load_for_test`,
  `delay_payload_load_for_test`, `take_load_processing_chunks_for_test`,
  `cancel_load_after_processing_chunks_for_test`) and 1 is shared
  (`set_transient_weight_override_for_test`). `load_save.rs` holds **12
  load-side** `*_for_test` functions after this change and no save-side ones.
  The row's `Seams` cell pools service seams the same way save's did — 3b should
  expect to re-derive row-scoped counts.
- **Flagged, not fixed: `queue_save_request` calls `cancel_load()` twice**
  (`save/admission.rs:100-101` inside the load-in-progress branch, and again at
  `:120-122` before ownership is published). It is real redundancy and it is
  **not collapsible without changing observable behavior**, which is why slot 3a
  preserved it exactly as the pre-migration code had it: `cancel_load` bumps
  `load_tracking.generation` unconditionally on every call
  (`load_save.rs:904-907`), so two calls advance the load generation by two, and
  any load-freshness assertion or in-flight load callback keyed to that value
  would observe a different number if the calls were merged. **3b owns
  `cancel_load` next and should decide**: either collapse the calls and re-derive
  the load-generation expectations, or keep both and document why the double bump
  is intended. Do not treat it as dead code to delete without that decision.
- **The load lane's cross-lane calls now point at the new module**:
  `load_runtime.rs` calls `save::admission::active_pressure`,
  `close_work_pending_or_active`, and `schedule_drain_for_external_change`.

### B.2 To slot 4 (`WFR-DRAFT-RECOVERY` and family) — task 12.3

40 ungated `.imp().` reach-through sites in
`crates/lushtext/tests/widget/window.rs` were catalogued but **not** migrated,
because they belong to the draft, session, and session-restore rows rather than
to document save. Slot 4 inherits a list rather than a rediscovery.

**`drafts.*` — 21 sites (10 reads, 11 writes):**
writes at `:927`, `:5780`, `:5781`, `:7697`, `:7701`, `:7702`, `:7982`, `:8030`,
`:8094`, `:18610`, `:18611`; reads at `:7700`, `:7983`, `:8031`, `:8095`,
`:8154`, `:8171`, `:8400`, `:8417`, `:10526`, `:13430`. The writes touch
`drafts.manifest`, `drafts.preloaded`, `drafts.autosave_inflight`, and
`drafts.autosave_pending`; the reads touch those plus `manifest_authority` and
`close_discard_ids`.

**`session.*` — 19 sites (18 reads, 1 write).** The single write
(`active_close_save_identity`) was migrated by this change, because it is the
close-save session identity the save ticket consumes; it is now
`window.expire_close_save_session_for_test()`. The 18 reads remain and touch
`session.save_failed` (`:6672`, `:13212`, `:13227`), `close_safety_inflight`
(`:6672`, `:10266`, `:13140`, `:13150`), `close_safety_bypass` (`:6677`),
`restore_cancel` (`:6702`, `:6791`, `:6842`, `:6891`, `:6935`, `:6991`), and
`failure_detail` (`:13228`).

**One correction slot 4 should carry forward:** `window.imp().session.save_failed`
is *session-file* save failure, written and cleared only by
`ui/window/session_persistence.rs`. It was named in slot 3a's planning as a
document-save priority site and is not one. A field whose name contains "save" is
not thereby save-workflow state.

## Appendix A — orientation record

Filled in during implementation. Each subsection is required by the task that
names it; leaving one empty means that task is not done.

### A.1 Gate evidence (task 0.1)
Verified mechanically on a clean tree, not read from the proposal.

- `openspec/changes/archive/` contains `2026-08-25-normalize-workflow-readability-boundaries`
  (slot 1), `2026-08-25-migrate-command-palette-workflow-readability` (2a), and
  `2026-08-25-complete-search-replace-workflow-readability` (2b). All three
  deltas are merged: `openspec/specs/` holds
  `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
  `gtk-adapter-module-boundaries`, `mutation-testing`, and
  `dbus-automation-spine`.
- `docs/workflow-readability-matrix.md` marks `WFR-SEARCH-REPLACE` (line 86) and
  `WFR-COMMAND-PALETTE` (line 87) `migrated`, each with a complete
  `Migrated Workflow Roles` subsection naming real facade, coordination, policy,
  evidence, and mutation-parity paths.
- The programme record's ledger marked slots 1, 2a, and 2b `complete`.
- `make check-workflow-boundaries` passed on the clean tree before any edit.
- The three slot-2a deliverables are present: the machine-readable
  `- normative facade line budget: 370` line in the matrix's
  "Facade size budget" section; the stage-order qualification rule in
  `openspec/specs/gtk-adapter-module-boundaries/spec.md`; and the working
  evidence-to-snapshot drift check in `scripts/check-automation-docs.py`
  (`EVIDENCE_PROJECTIONS` plus `evidence_projection_findings`, self-tested).

The convention requires two completed lower-risk proofs before a tier-3
workflow. Three exist.


### A.2 Premise re-verification (task 0.3)
Every figure re-derived against the tree. "Unchanged" is recorded explicitly.

| Figure | Authoring claim | Measured | Verdict |
| --- | --- | --- | --- |
| `load_save.rs` size | 1,795 lines | 1,795 | unchanged |
| `load_save.rs` in-file tests | zero `#[cfg(test)]` | zero | unchanged — all coverage is external, so the split cannot "move tests" and the whole file is production surface |
| `save_runtime.rs` size | 337 lines | 337 | unchanged |
| `model/save_admission.rs` size | 405 lines | 405 (261 production + 144 test) | unchanged |
| `save_admission.rs` reference set | census said 2 consumer files; authoring found 5 | **5 confirmed**: `save_runtime.rs`, `load_save.rs`, `model/mod.rs`, `benches/benchmarks.rs`, and `tests/widget/window.rs` | census cell was short; corrected in the matrix |
| Row `Current size` | `7 files, 6,672 lines (ui 2,132 / model 991 / services 3,549)` | wrong. The `services` subtotal counts the whole of `editor_io.rs` (3,035) and `durable_write.rs` (1,228), both shared with load and every other write path; the `ui` subtotal counts `load_save.rs` whole although the save half is roughly a third of it | corrected in the matrix |
| Row `Seams (i/c/a/p)` | `10/11/9/4 = 34 fns, 44 sites, 5 override statics` | **not row-scoped.** Row-scoped: `load_save.rs` holds 18 `*_for_test` functions of which **6** are save-side and 12 load-side; `save_runtime.rs` holds 2; `services/editor_io.rs` holds 10 unique names (6 load, 3 save, 1 shared); `durable_write.rs` 2; `filesystem/write.rs` 1; `ui/window/dialogs.rs` 7 (3 save-chooser-bound); `ui/window/documents.rs` 3. Gate-attribute sites: `load_save.rs` 19, `save_runtime.rs` 3, `editor_io.rs` 22. The 5 override statics are all in `services/editor_io.rs` and are shared with load | corrected in the matrix, with the shared population named |
| Inversions in the stage trace | 4 | **5.** The census missed the mirror-back inversion through the bounded buffer-replacement workflow, which the old trace folded into a prose phrase | corrected in the matrix; third confirmation that census inversion counts are floors |
| `ui/search_panel/mod.rs` | 369 of 370, 1 line of headroom | 369 | unchanged, and untouched by this change |


### A.3 The renamed-value seam: sites, decision, and failure modes (tasks 0.4, 5.3)
**Still live at implementation time.** Quoted with the line numbers as found.

Definition sites, both storing the value under the consequence's name:

- `ui/editor_page/save_runtime.rs:39` — `cancel_pending_load: bool,` in `struct QueuedSave`
- `ui/editor_page/save_runtime.rs:77` — `pub cancel_pending_load: bool,` in `struct SaveSubmission`

Three forwarding hops, **two of which cross the rename**:

1. `ui/editor_page/load_save.rs:1285` — `queue_save_request(.., cancel_pending_load: bool, ..)` receives it from three callers (`:1247` plain save with `false`, `:1260` Save As with `true`, `:1277` close save with `false`) and stores it into `SaveSubmission` at `:1331`. *Same name; no crossing.*
2. `ui/editor_page/save_runtime.rs:183` — inside the stale-request drain,
   `request.cancel_pending_load` is passed **positionally** as the third argument
   of `editor.queued_save_is_current(generation, &path, …)`, whose third
   parameter is declared `explicit_destination: bool` at
   `ui/editor_page/load_save.rs:1344`. **Crossing.**
3. `ui/editor_page/save_runtime.rs:255` — the admission handoff passes
   `request.cancel_pending_load` into `begin_admitted_save`'s
   `cancel_pending_load` parameter (`load_save.rs:1380`), which at `:1390` then
   passes it **positionally** into the same `explicit_destination` parameter.
   **Crossing.**

Inside the predicate (`load_save.rs:1351`) the value decides whether the queued
path is compared against `file_path()` at all.

`begin_admitted_save` carries the programme's only non-catalog
`#[expect(clippy::too_many_arguments)]` at `load_save.rs:1372-1375`.

**Decision (task 5.3): one field, named `explicit_destination`, with the
cancellation consequence derived through a named predicate.**

Read from the code, `queue_save_request` has exactly three callers and the value
is `true` for exactly one of them:

| Caller | Value | Wants the path comparison skipped? | Wants an in-flight load pre-empted? |
| --- | --- | --- | --- |
| `save_file_async` (plain Ctrl+S) | `false` | no — the target *is* the tracked path | no — must refuse instead |
| `save_file_async_to_path` (Save As) | `true` | yes — deliberately writes elsewhere | yes — the destination does not depend on the load |
| `save_file_async_for_close` (close-with-changes) | `false` | no | no |

Every caller that wants one wants the other, and the reason is a genuine
implication rather than a coincidence: a save with an explicit destination does
not depend on the in-flight load's result, so it may cancel that load and
proceed; a save without one targets the very path the load is establishing, so it
must refuse rather than write bytes derived from a half-installed buffer. So one
field is honest — but the implication is now **visible**, as
`policy::save_may_preempt_pending_load(explicit_destination)`, rather than
implied by a shared bool.

**Failure modes, stated for both directions** (and routed through the
`data-safety` pass in A.9, not left to review):

- A save that wrongly claims an explicit destination **skips the stale-target
  path comparison**. An editor re-pathed between queue and admission would write
  its bytes to the path it no longer tracks — a silent overwrite of the wrong
  file.
- A Save As that stops pre-empting the pending load **races a load into a
  just-saved buffer**. The load's completion would replace the user's freshly
  saved content with the older on-disk text, and the tab would read clean.

Both are pinned by
`policy::tests::explicit_destination_and_pending_load_cancellation_stay_distinct`,
and every operator in `queued_save_is_current` is individually mutation-killed.


### A.4 Current ordered stages before the change (task 0.5)
Read from the code, not from the census trace. Inversions are marked ⇢ with
their resumption point named. Line numbers are pre-change.

1. **Entry** — `load_save.rs:1242` `save_file_async` (plain), `:1255`
   `save_file_async_to_path` (Save As), `:1264` `save_file_async_for_close`.
   Each resolves a destination and calls `queue_save_request`.
2. **Queue** — `:1282` `queue_save_request`. Refusal gates in order: load in
   progress (cancels it when `cancel_pending_load`, else `LoadInProgress`);
   `installation_incomplete` → `IncompleteLoadInstallation`; buffer replacement
   in progress → `LoadInProgress`; `save.inflight` → `SaveInProgress`. Then the
   save generation advances, `inflight` is set, memory policy is notified, lossy
   consent is taken, and `save_runtime::submit` is called.
3. **Submit** — `save_runtime.rs:84`. Weight is computed from the O(1) live
   buffer estimate; the compact request is queued in `SaveAdmissionPolicy`;
   `schedule_drain()` and the load lane's drain are scheduled.
   **⇢ inversion 1: `glib::idle_add_local_once`, resuming in
   `save_runtime.rs:169` `drain`.**
4. **Drain** — retires requests whose freshness no longer holds (calling
   `queued_save_is_current` per request), refreshes weights and destination
   identities for survivors, computes external pressure from the load lane and
   from protected residency, then admits as many as fit and calls
   `begin_admitted_save` per grant.
5. **Admit** — `load_save.rs:1376` `begin_admitted_save`. Revalidates freshness,
   cancels any load, captures a `SaveCompletionTicket`, suspends view
   editability and cursor, and chooses a capture mode.
6. **Capture** — under the chunked threshold, `snapshot_buffer_text_direct`
   inline. Over it, `snapshot_buffer_text_async`.
   **⇢ inversion 2: chunked capture yields between slices, resuming in the
   snapshot callback at `:1417`,** which either continues to step 7 or ends the
   save without a write via `finish_save_snapshot_without_write` (`:1454`).
7. **Write** — `:1531` `write_snapshot_async`. Prepares local history, captures
   encoding state and formatting overrides, then `spawn_blocking_then`.
   **⇢ inversion 3: worker thread, resuming in the completion closure at
   `:1613`.** On the worker: apply save formatting; write through
   `editor_io::write_document_to_path` (which reaches
   `filesystem::write::atomic_replace`); resolve the canonical path; capture a
   local-history snapshot; decide the captured text's disposition.
8. **Accept** — the completion closure checks `SaveCompletionTicket::is_current`
   first; a stale result publishes nothing and restores the view. When
   formatting did **not** rewrite the text, `finish_accepted_save` (`:1723`)
   runs directly. When it did,
   **⇢ inversion 4: the bounded buffer-replacement workflow installs the
   formatted text, resuming in its terminal callback at `:1647`,** which is the
   only place the tab is marked clean on that path.
9. **Settle** — `finish_accepted_save` adopts size, size class, load state,
   encoding state, BOM, canonical path, file-health findings, mtime; refreshes
   marks, minimap, and accessibility; completes local history; then drops the
   permit. **⇢ inversion 5: `SavePayloadPermit::drop` posts
   `glib::idle_add_once`, resuming in `save_runtime.rs:270` `release_on_main`,**
   which releases the charge and re-arms both drains. Finally the caller's
   callback runs.
10. **Window side** — `ui/window/documents.rs:394` `save_current` (draft
    cleanup, open-path reconciliation, status message, announcement);
    `ui/window/dialogs.rs:88` `handle_save_as_selection` → `:157`
    `complete_save_as` (open-path rewrite, identity adoption, EditorConfig
    re-resolution, notes reset, draft deletion, tab title); `:596` the
    close-save pipeline, which gates on `close_save_session_is_current`.

**Five inversions, not the four the census recorded.** The missing one is
number 4.


### A.5 Role-home collision evidence (task 2.1)
The collision is mechanical, not stylistic.

- `ui/editor_page/` hosts **eight** workflows (matrix, "Role file names":
  "`ui/editor_page/` and `ui/window/` host 8 and 12 workflows respectively").
- The convention fixes the role file names: "The pure policy role is named
  `policy.rs` and the evidence role is named `evidence.rs`, one of each per
  workflow." Two workflows in one directory therefore cannot both use them.
- `.cargo/mutants.toml`'s `examine_globs` reaches pure policy through the
  literal `crates/lushtext-core/src/ui/**/policy.rs`. Verified contents:
  `model/**/*.rs`, `services/**/*.rs`, `ui/**/policy.rs`,
  `ui/markdown_preview/inline_footnotes.rs`, `ui/editor_page/minimap.rs`. A
  workflow-prefixed `save_policy.rs` matches none of them.
- `openspec/specs/mutation-testing/spec.md` treats a policy module outside scope
  reach as a **blocking** coverage regression, not accepted debt.
- The census had already assumed the answer without saying it: its relocation
  target for the minimap's policy is `ui/editor_page/minimap/policy.rs`, a
  per-workflow subdirectory, while its target for save's is written
  `ui/editor_page/policy.rs` — one file for eight workflows, which cannot be
  right. This change corrects that census cell.

**Verified after the move (task 2.3), not assumed:**
`./scripts/run-mutants.sh list | grep -c 'ui/editor_page/save/policy.rs'` → **58**,
and `make check-workflow-boundaries` reports **3** pure, mutation-scoped policy
modules. The nested path is reachable by both tools.


### A.6 Coordination role mapping, and why `journal` does not fit (task 3.1)
Two cohesive coordination jobs, both taking unqualified bounded names because the
workflow owns exactly **one** stage order — so the stage-order qualification rule
does not apply.

| Job | Role | Module | Contents |
| --- | --- | --- | --- |
| Everything before document text is copied | `admission` | `save/admission.rs` | the process-wide `thread_local` coordinator, the queue stage that publishes save ownership, the idle drain that retires stale tickets and admits fresh ones, exactly-once charge release, and the pressure/close-work queries other lanes read |
| Everything after | `execution` | `save/execution.rs` | view suspension, chunked or direct buffer capture, the worker write, completion acceptance, the formatting mirror-back, and the terminals |

Authoring's expectation was confirmed, with one refinement: the queue stage
itself (`queue_save_request`) moved into `admission` rather than staying in the
facade, because it owns bookkeeping — generation advance, ownership publication,
lossy-consent consumption — which the facade role forbids.

**`journal` was checked first, as the programme record instructed, and
rejected.** The record predicted it would look applicable in slot 3, and at first
glance it does: a save writes durably and irreversibly, and `retirement` would be
its opposite. But `journal` names a durable, generation-guarded record that **a
later stage of the same workflow reads back**, with startup recovery and stale-
record cleanup. A save replaces the user's file bytes; no later stage of the save
workflow restores from them. The record that protects an unsaved buffer *is* a
journal in that sense — but it is the draft, owned by `WFR-DRAFT-RECOVERY`
(slot 4), and pulling draft persistence into this change to justify the name
would be exactly the overload the bounded set exists to prevent.

The reusable test: **"does a later stage of *this* workflow restore from it",
not "does it touch the disk durably".** Slot 4 is where `journal` genuinely fits.

No novel job appeared, so no role name was added and the bounded set is
unchanged. This change carries one amendment already (the role home); a second
would have cost a re-check of three migrated rows.


### A.7 Widget-test reach-through sites and categorization (tasks 0.7, 7.8)
Full site list, per-site categorization, and outcomes are recorded in
`evidence/widget-test-save-site-migration.md`. Summary:

- **Save-owned ungated writes: 5** (`window.rs:6085`, `:6096`, `:13830`,
  `:13986`, `:14005`). All five verified present and all five are writes. Four
  became real drives of the workflow using an existing configuration seam; one
  became a new named actuation seam, counted and justified.
- **Save-owned ungated reads: 0.**
- **Retired inspection seams: 4 call surfaces over 3 mechanisms**, 20 call sites
  migrated to `save_evidence()`.
- **Scope correction:** the planning figure of "13 sites: 9 writes, 3 reads, 1
  widget actuation" was not row-scoped, and `window.imp().session.save_failed` —
  named as a save priority — is *session-file* save failure owned by
  `WFR-SESSION-RESTORE` (slot 4), not document save.
- **Handed to slot 4: 40 sites** touching `drafts.*` (21 sites: 10 reads, 11
  writes) and `session.*` (19 sites: 18 reads, 1 write, the last of which this
  change migrated because it is the close-save session identity the save ticket
  consumes). See task 12.3.


### A.8 Write-side seam classification (task 6.6)
Classified by kind, so a later slot does not read them as unfinished work. **All
of these stay.**

`services/editor_io.rs`:

| Seam | Kind | Disposition |
| --- | --- | --- |
| `set_save_write_delay_for_test` | configuration | stays in the service — the service owns the delayed behavior. This change now *uses* it to drive real saves in tests that previously forged `save.inflight` |
| `fail_next_save_for_path_for_test` | configuration (fault injection) | stays — the mechanism this change's `BeforeRename`/`AfterRename` failure-path verification depends on |
| `clear_save_failure_for_test` | configuration | stays, paired with the above |
| `set_transient_weight_override_for_test` | configuration | stays; shared with the load lane |
| `delay_save_write_for_test` | probe | stays; the internal `cfg(not(...))` no-op pair keeps production builds free of the branch |
| `set_load_delay_for_test`, `set_payload_load_delay_for_test`, `delay_load_for_test`, `delay_payload_load_for_test`, `take_load_processing_chunks_for_test`, `cancel_load_after_processing_chunks_for_test` | configuration / inspection | **load-side**; belong to slot 3b |

`services/durable_write.rs`:

| Seam | Kind | Disposition |
| --- | --- | --- |
| `observe_temp_after_content_for_test` (the after-metadata hook) | actuation (fault/observation injection at an exact ordering point) | stays. **Its registration returns cleanup ownership specifically so parallel tests cannot consume each other's injected failure — that property is preserved and must not be refactored into a process-global slot** |
| `fail_next_parent_sync_for_test` | configuration (fault injection) | stays — this is what produces a genuine `AfterRename` / durability-unconfirmed outcome |

`services/filesystem/write.rs`:

| Seam | Kind | Disposition |
| --- | --- | --- |
| `fail_next_parent_sync_for_test` | configuration (fault injection) | stays, `pub(crate)`; the boundary-level counterpart of the above |

**No `test_policy.rs` was created for this workflow.** Task 7.6 permits an
override to stay in the service when the service owns the behavior, and all five
`test-utils` override statics are in `services/editor_io.rs` and are shared with
the load lane. A second policy value in `ui/` would shadow them and would have to
be kept in sync across two workflows.


### A.9b Automation no-widening proof (task 8.5)

Proved by capture and diff, not asserted.

`make automation-smoke` was run on the pre-change tree (via `git stash -u`) and
on the changed tree, both under isolated headless Mutter and a private D-Bus
session with the same fixture document, and the two
`build/smoke/automation/assertions/snapshot-initial.json` captures plus the two
`readiness-predicates.json` captures were diffed:

- `window.tabs` objects: **identical**, including `saving: false` and every
  neighbouring field.
- Whole-snapshot key set: **zero keys added, zero removed**.
- Whole-snapshot value diff: **zero differing keys at any depth**.
- Readiness predicates and blockers: **identical**.

Re-proved after review finding F6 moved the two boolean-only readiness sites back
to the facade's cheap `is_saving()` accessor: same four checks, same zero-diff
result. Only the documented `tabs[].saving` projection reads the evidence
surface; the readiness aggregate and the `save` blocker read the same
`save.inflight` cell through `is_saving()` rather than building a whole
`SaveEvidence` per editor per poll, which is identical by construction.

That is stronger than the field-level check the task asked for, and it is
consistent with the structural argument: `save_evidence().inflight` reads exactly
the cell `is_saving()` read, so the projected value is byte-identical by
construction. The exported `AutomationTabSnapshot` struct, the
`READINESS_BLOCKER_SAVE` constant, and the `save-complete` predicate are
untouched — `git diff` on `ui/automation.rs` shows three source substitutions and
one comment, with no type, field, or constant edit.

`make check-automation-docs` and `make automation-client-self-test` both pass,
and the drift gate now covers the new projection through a
`window.tabs` / `SaveEvidence` / `inflight` / `tabs.saving` row in the Evidence
Projection Map plus a third entry in the script's `EVIDENCE_PROJECTIONS`.

### A.9 Data-safety passes (tasks 0.6, 11.8)
**Pass 1 — before writing code, over the intended diff surface.**

Reviewed against the durable-write contract in `.agents/rules/rust.md` and the
`data-safety` domains (draft persistence, save/close flow gaps, replace-all
backup safety, session restore, async concurrency):

- *Ordering contract.* `TargetWriteGuard` acquisition, the probe → temp-create →
  write → flush → metadata → `sync_all` → `rename` → parent `fsync` sequence, and
  the `BeforeRename`/`AfterRename` split all live in
  `services/filesystem/write.rs` and `services/durable_write.rs`, which this
  change does not touch. The extraction moves the *caller*, not the boundary.
- *Buffer-versus-disk agreement.* Identified as the highest-risk invariant to
  preserve: when save formatting rewrites the text, the tab must not be marked
  clean until the formatted text is installed back into the buffer. Pre-change
  this is enforced by the buffer-replacement terminal callback being the only
  path to `finish_accepted_save` in the rewrite case. **Flagged as an invariant
  the extraction must not reorder.**
- *Freshness.* Two independent guards (`queued_save_is_current`,
  `SaveCompletionTicket::is_current`) protect different windows. **Flagged: they
  must stay distinct**; collapsing them would let a stale worker result mutate a
  re-pathed editor.
- *The renamed value.* Both failure modes recorded in A.3 are data-safety
  outcomes, so the decision was taken here rather than in review.
- *Permit release.* Exactly-once release across cancellation, worker failure,
  stale completion, and success. **Flagged: every terminal must still converge on
  `SavePayloadPermit::drop`.**

**Pass 2 — after the diff.**

- *Ordering contract:* unchanged. No line in `services/` was modified;
  `git diff --stat` shows zero changes under `crates/lushtext-core/src/services/`
  except the new `expire_close_save_session_for_test` in `ui/window/dialogs.rs`,
  which touches no I/O.
- *Buffer-versus-disk agreement:* preserved. `finish_accepted_save` is still
  reachable in the rewrite case only from the buffer-replacement terminal
  callback, and the disposition that decides the rewrite is now a pure,
  mutation-killed function (`classify_saved_text`) rather than an inline boolean
  pair. The evidence surface additionally makes the invariant *observable*
  (`formatting_rewrote_buffer`, `mirror_back_completed`), which it was not before.
- *Freshness:* both guards intact and now documented as distinct in the facade.
  One disclosed behavioral detail: `QueuedSaveFacts` is captured as a unit, so a
  ticket naming a close session resolves the window lookup even on paths where an
  earlier clause already failed. It is a pure scalar read on a path about to
  cancel the request anyway; no ordering or durability consequence.
- *The renamed value:* resolved as recorded in A.3, with both failure modes
  pinned by a pure test and every predicate operator mutation-killed.
- *Permit release:* unchanged. `SavePayloadPermit::drop` still posts
  `idle_add_once`, and all four terminals still reach it.
- *Test-side forgery removed:* four widget tests previously asserted close-flow
  and eviction behavior against a **forged** `save.inflight` flag that no
  workflow had produced. They now drive real saves. This is a small but real
  data-safety improvement: the guards are now tested against states the workflow
  can actually reach.

**Confirmed findings: none.** Unlike slot 2b, which found two pre-existing
data-loss defects, this pass produced no confirmed finding. That is reported as a
data point rather than a reassurance — this change deliberately moved the save
path without re-sequencing it, and the invariants it preserves were already
correct.


### A.10 Shared-field ownership across the save/load cut (task 3.2)
The file's regions interleave, so the extraction was **item-level**, not a line
range. Each entanglement with its post-3a owner:

| Entanglement | Owner after 3a | How the cut was made |
| --- | --- | --- |
| The pending-load cancellation (`cancel_pending_load` semantic) — a save-side action reaching load-side state | **load** owns `cancel_load` and all `imp().load*` state; **save** owns the *decision* to call it | Save reaches it through one named derivation, `policy::save_may_preempt_pending_load`, and one call to the load workflow's existing public `cancel_load()`. No load state moved into `save/` |
| `size_check` — documented as "size classification from the last file load" but written by the save path too | **neither**: editor-page document metadata, living on `imp()`, written by both load and save and read by minimap, encoding, accessibility, and local history | The public `size_check()` getter stays in `load_save.rs` with the other identity/metadata accessors. Save writes `imp().size_check` directly in `finish_accepted_save`, exactly as before |
| **The restore-position group** (`set_restore_position`, `cursor_position`, `visible_top_line`, `apply_restore_position`) | **cross-cutting** — five owning workflows: session restore (slot 4), editor find (slot 7), notes/bookmarks (slot 5), load (3b), and the window's tab handling | **Did not move, and must not.** Cross-cutting eligibility counts owning workflows. It stays in the shared `ui/editor_page/` location with its ownership recorded in the residual file's module doc. **The save workflow does not call it at all** — verified: `apply_restore_position` has exactly one caller, inside load's completion |
| `ViewInteractivityState` (was `load_save.rs:43-47`) — sat inside the save cluster while being a field of the load-side `LoadInstallationState` | **load** (3b) | Left exactly where it was. Save defines its own two-field `SaveViewInteractivity` in `save/execution.rs`. The duplication is deliberate: importing load's type would have created a `save → load_save.rs` dependency that 3b would then have to untangle, which is the "worse seam" the task warned against. Each workflow now owns the flags it suspended |
| `set_file_path_for_pending_load` (was `:1215-1228`) — load-only but sitting inside the save tail | **load** (3b) | Left in place; it was never save's |
| `set_file_path` / `set_file_path_with_canonical` | **neither**: shared document identity, also used by the rename flow in `ui/window/documents.rs` | Left in `load_save.rs`. Save reaches identity adoption through one named facade operation, `adopt_saved_destination`, which the window calls after a successful Save As |
| Save-side seams at `:1058-1143` sitting inside the load region, interleaved with load-side hooks | **save** | Moved item by item: the two admission seams to `save/admission.rs` and `save/evidence.rs`, the two snapshot seams to `save/execution.rs`. The 12 load-side seams stayed |
| `SaveCallback` type alias, `SaveCompletionTicket`, `AdmittedSaveContext`, `SaveWriteOutcome` | **save** | Moved to `save/mod.rs` and `save/execution.rs` |

**Residual `load_save.rs`: 1,212 lines** (from 1,795), holding the load half plus
the two cross-cutting groups above, with a module doc stating the name is stale
and pointing at both `super::save` and slot 3b.


### A.11 Facade measurement, and the parked-state record if the budget cannot fit (task 9.3)
**The budget held comfortably. No escalation, no parked state, and the budget
line was not edited.**

| Facade | Measured | Budget | Margin |
| --- | --- | --- | --- |
| `ui/editor_page/save/mod.rs` (this change) | **223** physical lines | 370 | 147 under |
| `ui/search_panel/mod.rs` (exemplar) | **369** | 370 | 1 under — **untouched by this change**, re-measured to confirm |
| `ui/command_palette/mod.rs` | **335** | 370 | 35 under |

Physical lines, comments and blanks included, which is what
`make check-workflow-boundaries` counts. The gate passes.

The response order in task 9.3 was not needed, but the reason it was not needed
is worth recording: **this facade narrates one stage order.** It has five
inversions — more than the palette's first stage order — and still fits with room
to spare. Slot 2b's qualification therefore gains a second data point: what
stresses the budget is the *number of stage orders*, not the risk tier, the
inversion count, or the size of the workflow. Slot 6 (minimap) remains the slot
most likely to prove the number wrong, and slot 3a supplies no evidence either
way.

**Task 9.4 — the exemplar's 1 line of headroom is intact.**
`ui/search_panel/mod.rs` measures 369 and appears in no diff hunk of this change;
`git diff --stat` does not list it.

