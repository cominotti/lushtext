## 0. Gates, orientation, and premise re-verification

- [x] 0.1 **Slot-3 gate — blocking.** This change may not begin until slot 3 is
      complete in **both** halves. Verify mechanically on a clean tree rather than
      reading it from the proposal: `openspec/changes/archive/` contains the slot
      1, 2a, 2b, 3a, and 3b changes; `openspec/specs/` holds
      `workflow-readability-boundaries`, `workflow-evidence-surfaces`,
      `gtk-adapter-module-boundaries`, `mutation-testing`, and
      `dbus-automation-spine`; `docs/workflow-readability-matrix.md` marks
      `WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`, and
      `WFR-DOCUMENT-LOAD` `migrated` with complete `Migrated Workflow Roles`
      subsections naming paths that exist; the slot ledger in
      `docs/next/workflow-readability.md` marks slots 1, 2a, 2b, 3a, and 3b
      complete and slot 4 outstanding; and `make check-workflow-boundaries`
      passes. Three of this slot's four rows are `tier-3`, so the two-proof rule
      applies to each. Record in A.1.
- [x] 0.2 Read `docs/next/workflow-readability.md` end to end, including all four
      "Convention friction slot N hit" sections, and
      `docs/workflow-readability-matrix.md`'s Settled Conventions, Facade size
      budget, Evidence-surface reentrancy, Cross-cutting eligibility, Evidence
      pointer form, and Completion Rule sections. Then read slot 3a's and 3b's
      archived `tasks.md` Appendix B handoffs — this change is their named
      recipient twice over. Note the four capability specs this change consumes
      and the two it amends.
- [x] 0.3 **Premise re-verification — before any code, and once per row.** Four
      consecutive slots found their measured cells wrong in both directions, and
      the amendment in task 1 makes re-derivation a stated obligation rather than
      a habit. For **each** of `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`,
      `WFR-LOCAL-HISTORY`, and `WFR-BUFFER-REPLACEMENT`, produce a row-scoped
      figure and name what the census cell had pooled:

      - **Size**: production lines only, excluding `#[cfg(test)]` modules,
        counting only files the workflow owns. Do not count shared services
        (`services/draft_service.rs`, `session_service.rs`,
        `local_history_service.rs`, `recovery_metadata.rs`), cross-cutting modules
        (`model/buffer_replacement.rs`, `ui/plain_disposal.rs`,
        `ui/buffer_snapshot.rs`, `ui/editor_page/restore_position.rs`), or
        neighbour files the workflow only calls. Authoring measured
        `ui/window/drafts.rs` 2,460 total, `session_persistence.rs` 1,110,
        `session_restore.rs` 417, `startup_data.rs` 435, `local_history.rs` 1,633,
        `ui/editor_page/local_history.rs` 953, and `buffer_replacement.rs` 1,029
        **including** co-located tests; treat those as upper bounds to be
        corrected, not as the answer.
      - **Seams**, per kind (inspection / configuration / actuation / probe-reset)
        with the `#[cfg(feature = "test-utils")]` site count. Authoring counted 28
        `*_for_test` functions across 55 gate sites in `drafts.rs`, 4 across 5 in
        `session_persistence.rs`, 4 across 12 in `ui/window/local_history.rs`, 13
        across 17 in `ui/editor_page/local_history.rs`, 8 across 26 in
        `buffer_replacement.rs`, and 0 in `session_restore.rs`,
        `startup_data.rs`, and `draft_ordering.rs`. Classify each function by
        kind; the census tuples predate that classification.
      - **Pure policy consumer counts** for `model/draft.rs`,
        `model/session.rs`, `model/local_history.rs`, and
        `model/buffer_replacement.rs`, counted as **owning workflows** rather than
        referencing files, and with substring false positives named. 3b lost time
        to six `file_load` substring hits; expect the same shape from `draft`,
        `session`, and `local_history`, which appear in callback names, field
        names, and test function names throughout `ui/`.
      - **The shared population each corrected cell had pooled**, named with the
        rows that share it — the amendment requires this so slot 5 or 7 does not
        re-derive it from the other side. 3b already named one: the six load-side
        `test-utils` overrides in `services/editor_io.rs` are shared with
        `WFR-DOCUMENT-SAVE` and `WFR-DRAFT-RECOVERY` and **stay in the service**.

      Write all four to `openspec/changes/migrate-user-content-restore-workflow-readability/evidence/census-reverification.md`
      and summarise in A.2. Correct the matrix cells in task 9.
- [x] 0.4 **Read the code before changing it.** For each of the four workflows,
      write the current ordered stages and **every** control-flow inversion from
      the code, not from the matrix trace. Census inversion counts are **floors**:
      four consecutive slots found more inversions in the code than the trace
      recorded, roughly double in 3b's case. The matrix records 7 worker handoffs
      for drafts, 1 for session restore, 6 for local history, and 1 for buffer
      replacement; budget for more. Record in A.4, and correct the
      `Workflow Stage Traces` entries in task 9.
- [x] 0.5 **Record the durability contracts before touching anything near them.**
      These are behavior this change must preserve exactly, so they are written
      down first and diffed against afterwards:

      - **Orphan-body cleanup identity**, per `.agents/rules/rust.md`: inspection
        records the candidate inode; execution reloads the latest trusted
        manifest, acquires the same stable `TargetWriteGuard` used by atomic
        replacement, **then rechecks inode before deleting**. Manifest
        serialization alone is insufficient because an autosave may finish
        replacing the body before it acquires the manifest lock. Record the
        current call order verbatim.
      - **Paragraph-boundary bounded installation**: install and clear slices end
        on paragraph boundaries, and a paragraph larger than the slice budget
        installs in one turn. A slice stopping mid-paragraph makes every later
        slice re-lay-out that paragraph — the quadratic behavior that once froze
        crash recovery of a 33 MB single-line draft for minutes. This slot owns
        the module that implements it (`model/buffer_replacement.rs`'s
        `next_replacement_boundary`) as a *caller*, and it must not be
        re-derived, duplicated, or "simplified".
      - **Durable draft, session, and sidecar write ordering** as
        `services/filesystem::write` provides it, including the
        `BeforeRename`/`AfterRename` classification honesty.

      Write to `evidence/durability-contracts.md` with a before/after section, and
      summarise in A.5.
- [x] 0.6 Invoke the `data-safety` skill in explicit mode over the intended diff
      **before** implementing. Three of four rows are tier-3 and this is the first
      time this family is read end to end. Slot 2b found two confirmed pre-existing
      defects and 3b found one; **budget for findings**, and treat
      `.agents/rules/preexisting-blockers.md` as binding — a confirmed finding is
      fixed in this work stream, not recorded as debt, even though the change's
      non-goals say "no behavior change". Record in A.9.
- [x] 0.7 Grep this family's tests for `\.imp()\.` reach-through, not only for
      `_for_test`. Ungated reach-through appears in no seam census yet shapes
      production field layout. Slot 3a catalogued **40 sites in
      `crates/lushtext/tests/widget/window.rs`** and handed them here with line
      numbers: `drafts.*` writes at `:927`, `:5780`, `:5781`, `:7697`, `:7701`,
      `:7702`, `:7982`, `:8030`, `:8094`, `:18610`, `:18611` and reads at `:7700`,
      `:7983`, `:8031`, `:8095`, `:8154`, `:8171`, `:8400`, `:8417`, `:10526`,
      `:13430`; `session.*` reads at `:6672`, `:6677`, `:6702`, `:6791`, `:6842`,
      `:6891`, `:6935`, `:6991`, `:10266`, `:13140`, `:13150`, `:13212`, `:13227`,
      `:13228`. Line numbers drift — re-derive by field name, not by line.

      **Correct the total rather than inheriting it.** 3a reported 40; the current
      tree holds **35 sites — 21 `.imp().drafts.` and 14 `.imp().session.`** — and
      the session half is 15 field occurrences across 14 lines because one line
      reads two fields. 3a's own enumerated list cannot reach 40 either, so this is
      a correction, not drift. Task 6.12's delta baseline uses 35.

      Also sweep for reach-through 3a did not catalogue: `local_history` and
      `buffer_replacement` in `editor_page.rs` (authoring found **zero** — confirm
      and record the zero rather than assuming a population), and
      `ui/window/dialogs.rs`-driven state in any widget module, since that file
      owns the close-save session over `imp().session`. Categorize every site as
      *evidence read*, *real drive through an existing seam*, or *needs a counted
      actuation seam*, and record in A.7.
- [x] 0.8 **Open the two candidates 3b routed here as investigations, with their
      missing evidence named.** Neither is assumed to be a defect and neither is
      assumed benign; each ends in a recorded verdict with the evidence that
      produced it. See task 7.

## 1. Apply the two convention amendments and pay the retroactive re-check

- [x] 1.1 Confirm each amendment's basis from the code and the matrix before
      amending anything. For (a): confirm `model/buffer_replacement.rs` is
      recorded cross-cutting, confirm `scripts/check-workflow-boundaries.py`
      already accepts `policy: none` as an optional role value, and confirm no
      existing spec scenario states that such a row is complete — the permission
      currently reads as gate tolerance. For (b): confirm from the four migrated
      rows' history that each corrected its measured cells, and quote the three
      friction-section instructions that say to re-derive.
- [x] 1.2 Apply this change's `workflow-readability-boundaries` delta: the
      row-scoped re-derivation obligation on the enumeration requirement, and the
      complete-row-without-`policy.rs` allowance plus the shared-arithmetic
      no-duplication statement on the pure-policy requirement. Nothing beyond
      those two statements may be absorbed. The **facade line budget** and the
      **bounded coordination role set** are not amended by this change.
- [x] 1.3 **Retroactive-amendment obligation — four rows, checked individually.**
      Under section 8, re-check `WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`,
      `WFR-DOCUMENT-SAVE`, and `WFR-DOCUMENT-LOAD` against both statements. Both
      are *expected* to be confirmations, but 3b proved that "it must already
      hold" is not a discharge: its promoted reentrancy constraint found one of
      three migrated rows genuinely lacking the proof. Per row, record whether its
      measured cells were re-derived row-scoped with the pooled population named,
      and whether it declares a `policy` role at all. Fill any gap **in this
      change**. Record each verdict in the matrix's amendment re-check section as
      a new `### Slot 4 amendment re-check` subsection.
- [x] 1.4 Re-confirm in that same subsection that the other settled conventions
      are untouched: role file names, the bounded coordination role set, the
      facade budget number, the seam value-object shape, the evidence-surface
      visibility rule, the reentrancy constraint, cross-cutting eligibility, and
      the evidence pointer form.
- [x] 1.5 Update standing guidance where a reader would look for the two new
      statements: `.agents/rules/rust.md`'s Workflow Vocabulary section for the
      complete-row-without-policy case and the shared-arithmetic rule, and
      `.agents/rules/documentation.md` if the row-cell re-derivation obligation
      changes what a workflow-structure change must update. Run
      `make check-agent-docs` and `make check-agent-skills`.

## 2. Shared-ownership decisions that must precede the four migrations

Every one of these is a boundary two or more of this slot's rows would otherwise
decide twice, from two sides. Decide each **before** touching the workflow that
would absorb it, and record the decision with its reason.

- [x] 2.1 **`next_install_boundary` — the alias 3b left open.** The arithmetic is
      one function, `model::buffer_replacement::next_replacement_boundary`, and
      `model/file_load.rs` re-exports it as `next_install_boundary`. The 3b matrix
      row records that it must **not** be duplicated. Decide explicitly between:
      the alias stays as a named domain synonym for the load path (recording why
      two names for one function is the readable choice), or load callers reach the
      cross-cutting owner directly and the alias is removed. Either way
      `model/buffer_replacement.rs` keeps the implementation and the
      paragraph-boundary contract is unchanged. **If the name goes, the fan-out is
      four places plus two doc links**, all updated in the same breath:
      `crates/lushtext-core/benches/benchmarks.rs`;
      `crates/lushtext-core/tests/properties/file_load.rs` (three sites, which is
      why task 10.8 exists — that target is gated behind
      `required-features = ["property-tests"]`, so no default lane would catch a
      broken import); `ui/editor_page/load/policy.rs`; and the **two rustdoc
      intra-doc links** in `load/policy.rs` and `load/execution.rs`, which is
      exactly the CI-only gate shape task 9.4 catches. Confirm the amendment's
      shared-arithmetic scenario is satisfied either way.
- [x] 2.2 **`ui/window/startup_data.rs` (435 lines) — shared or owned?** It feeds
      both session restore and draft restore at startup. Decide by owning
      workflows, not by call count. If two rows own it, it stays shared with its
      ownership recorded in the module doc, exactly as
      `ui/editor_page/document_identity.rs` did across save and load. Do not let
      the session row absorb it by default because it happens to be migrated
      first.
- [x] 2.3 **`services/recovery_metadata.rs` (1,636 lines) — shared between drafts
      and session.** Behavior unchanged. Decide its seam ownership and whether it
      holds buried pure policy that belongs to one row, per slot 2b's
      `services/search_backup.rs` precedent. Do not split it by guess, and do not
      relocate it: a `services -> ui` dependency inversion is forbidden outright,
      which is how 2b settled `workspace_search.rs` and 3b settled `file_load.rs`.
- [x] 2.4 **`session.save_failed` — ownership transferred by slot 3a.** 3a's
      planning named `window.imp().session.save_failed` as a document-save
      priority site and then found it is *session-file* save failure, written and
      cleared only by `ui/window/session_persistence.rs`. This change owns it.
      Carry 3a's correction forward verbatim in the record: **a field whose name
      contains "save" is not thereby save-workflow state.** Its three widget-test
      read sites go to the session evidence surface.
- [x] 2.4a **Split the three shared imp state groups deliberately, not by
      default.** `.agents/rules/rust.md` asks that accumulated imp state be
      grouped by clear workflow ownership; three groups this slot touches are
      currently shared with workflows outside it, and no other task decides them.
      Decide each explicitly, and record which fields each of this slot's
      workflows owns, which it only reads, and which stay shared:

      - **`SessionState` (`ui/window/imp.rs`).** It holds genuine session fields
        *and* migrated save's close-save identity pair
        (`next_close_save_identity` / `active_close_save_identity`), which
        `ui/window/dialogs.rs` drives end to end. It also holds
        `close_safety_inflight` and `close_safety_bypass`, whose own doc comments
        say **"draft/session"** — so task 4.5's claim on both is a *hypothesis to
        test*, not a settled fact. If either is genuinely shared between the draft
        and session rows, say so and record the shared ownership rather than
        letting the first-migrated row absorb it. The save identity pair belongs
        to a migrated workflow: decide whether it moves to save's home, stays with
        a recorded owner, or is renamed for clarity — and do not restructure what
        3a migrated beyond what the decision requires.
      - **`DraftState`.** Enumerate its fields, name which are draft-owned, and
        name any read by another workflow.
      - **`LocalHistoryState` (`ui/editor_page/imp.rs`).** Migrated save reads it
        as its `SaveCompletionTicket` freshness identity, and migrated load reads
        it at two sites. `ui/editor_page/save/mod.rs` **already documents it as
        slot 4's to own**, so this is an inherited obligation rather than a
        discovery. Decide the ownership boundary such that save and load keep
        reading it through a named operation rather than a field reach.

      A field this slot moves out from under a migrated workflow's reader is a
      compile-checked change; a field it *claims* without checking is the archetype
      defect at state-group scale. Record all three in A.3.
- [x] 2.5 **Confirm the closed boundaries and do not re-open them.**
      `ui/editor_page/restore_position.rs` is cross-cutting with **five** owning
      workflows — this slot's session-restore row is one of them — and the
      decision that it MUST NOT move is closed and recorded in the matrix. Call
      it; do not absorb it. Likewise `ui/plain_disposal.rs` (21 files, 10
      workflows), `ui/buffer_snapshot.rs` (`WFR-BUFFER-SNAPSHOT`, slot 7),
      `model/editor_memory.rs` (exempt, no slot), and
      `ui/editor_page/document_identity.rs` (owned by neither document workflow).
      Record the confirmation so a reader does not think the question is open.
- [x] 2.6 **Excluded by scope: `WFR-NOTES-BOOKMARKS`.**
      `NoteSourceRefreshCoordinator`, `services/palette/notes.rs`, and the notes
      browser surfaces belong to slot 5. They will look adjacent — notes are
      sidecars like local history, and both go through `NoteSourceAdmission` — but
      pulling them in is the overload the slot boundaries exist to prevent.
      Record the exclusion where a reader will hit the adjacency.
- [x] 2.7 **Check the `journal` role name once, for all three durable rows, before
      creating any module.** Slot 3a reserved it for this slot; the test is *does
      a later stage of the same workflow restore from the record*, not *does it
      touch the disk*. Apply it separately to: the draft manifest and bodies
      (startup recovery reads them back — expected to pass), the session file
      (startup restore reads it back — expected to pass), and local-history
      sidecars (restore reads them back — expected to pass). Per slot 2b's
      definition, the mutual-exclusion gate serializing a record's writes and any
      byte reservation those writes take live **inside** the journal, not in a
      separate `admission`. If a row's durable record genuinely fails the test,
      say so and pick from the bounded set rather than stretching a name. Record
      the mapping in A.8 per workflow, and only amend
      `gtk-adapter-module-boundaries` if a genuinely novel coordination job
      appears — which would be an escalation, not an absorption.

## 3. `WFR-BUFFER-REPLACEMENT` — migrate first, because everything else calls it

- [x] 3.1 From the code, record this workflow's cohesive coordination jobs. It has
      one stage order (install or clear slices resuming per turn, each ending on a
      paragraph boundary, then projection-suspension release, then caller
      completion) and one already-reified inversion. `execution` is the expected
      fit; confirm from the code rather than assuming, and apply the
      cohesion test 3b recorded — *is the job cohesive enough that a reader would
      look for it under its own name* — rather than grouping by adjacency.
- [x] 3.2 **Role home decision by collision analysis.** `ui/editor_page/` hosts
      eight workflows and two (`save`, `load`) already use per-workflow
      subdirectories. This workflow needs an `evidence.rs` and would collide, so a
      per-workflow subdirectory `ui/editor_page/buffer_replacement/` is expected.
      Confirm the collision explicitly, then create the home with `mod.rs` as the
      facade. Re-verify after the move that `check-workflow-boundaries` resolves
      the nested paths — two adopters prove the shape, but a third move still
      needs its own confirmation.
- [x] 3.2a **Re-derive the caller inventory from the code, because the matrix's is
      wrong.** `BufferReplacementWorkflow`'s variants are the authority:
      `MemoryEviction`, `DraftRecovery`, `LocalHistoryRestore`,
      `LocalHistoryUndo`, `SaveFormatting`. Authoring found **five call sites of
      `replace_buffer_bounded` across four owning workflows** —
      `ui/window/drafts.rs` (`WFR-DRAFT-RECOVERY`), `ui/window/local_history.rs`
      twice (`WFR-LOCAL-HISTORY` restore and undo), `ui/editor_page/mod.rs`
      (eviction, `WFR-EDITOR-MEMORY`, **exempt, no slot**), and
      `ui/editor_page/save/execution.rs` (save formatting, `WFR-DOCUMENT-SAVE`,
      **migrated, 223-line facade**) — plus direct consumers of the pure module in
      `model/file_load.rs` and `ui/editor_page/load/policy.rs`
      (`WFR-DOCUMENT-LOAD`, migrated). **Replace All undo is not a caller**;
      `LocalHistoryUndo` is local history's own undo. Verify all of that against
      the tree rather than inheriting this list, and note the two out-of-slot
      workflows explicitly: this change may call them and must not restructure
      them.
- [x] 3.3 **Probe for owned pure policy before concluding `policy: none`.** The
      delta this change applies requires the workflow's pure logic to be
      **entirely** cross-cutting for the row to be complete without a `policy.rs`,
      so the conclusion must be a finding with evidence, not the starting premise.
      Run the same probe tasks 4.3, 5.3, and 6.4 run: examine the GTK adapter's
      candidate pure decisions — the slice accounting in `delete_one_slice`, the
      terminal classification in `finish_session`, and the supersession decision in
      `clear_owner_and_start_pending` — and for each, either extract it into a
      `policy.rs` (making this an ordinary gain-from-zero extraction) or record the
      negative finding with the reason it is not separable pure policy. Note the
      asymmetry with 3b's `file_load.rs` decision: that module stayed in `model/`
      *and* its workflow still owned a `policy.rs`, so "the domain module stays"
      has never by itself implied "the workflow owns no policy".
- [x] 3.3a **Record the `policy` role outcome with its reason.** If the probe finds
      nothing separable, this becomes the programme's first migrated row declaring
      `policy: none`, which is why task 1 states that case normatively.
      `model/buffer_replacement.rs` (186 lines, 93 of them co-located tests) stays
      either way: cross-cutting on the same grounds as `plain_disposal`, with the
      caller set from task 3.2a. Name the module **and the correct other owning
      workflows** in the matrix entry — this row is the first to exercise the
      amendment's "name the other owning workflows" sentence, so a wrong set here
      would discredit the sentence on its first use. **Do not fork, copy, or
      re-implement any part of the shared module beneath this workflow** to
      manufacture a local `policy.rs`; the paragraph-boundary arithmetic is
      precisely the shared limit that would fork.
- [x] 3.4 Audit the existing seam value objects against the two-boundary rule:
      `BufferReplacementTicket` and `BufferReplacementSession` are recorded as
      existing. Confirm each crosses two or more boundaries or is reconstructed at
      two or more call sites, confirm no value is renamed while crossing a seam
      (the archetype defect), and confirm no
      `#[expect(clippy::too_many_arguments)]` is introduced. If the audit finds an
      unreified bundle, reify it using the established `Ticket` + `Facts` +
      `*_is_current` idiom rather than a parallel shape.
- [x] 3.5 Build `ui/editor_page/buffer_replacement/evidence.rs` with one accessor.
      Fold in the state the four existing inspection seams expose
      (`buffer_replacement_in_progress_for_test`,
      `buffer_replacement_projection_suspended_for_test`,
      `buffer_replacement_slice_count_for_test`,
      `buffer_replacement_terminal_diagnostic_for_test`) and retire them, leaving
      no per-field getter. The matrix records this row's evidence surface as
      `none` today, so this is a new surface. Honor all three obligations:
      tight-borrow (compute derived scalars and drop each borrow before the struct
      literal), **`TemplateChild::try_get()` for any field derived from a template
      child** — this workflow reads the source view's buffer, so it is squarely in
      slot 3a's disposed-widget trap — and the reentrancy proof test in the
      correct direction (drive each mutating operation, read the surface **after**
      each one, assert repeated reads of unchanged state are identical). A test
      that reads the surface *while* a borrow is held is the failure, not the
      proof.
- [x] 3.6 Classify the four actuation seams (`replace_buffer_for_test`,
      `replace_buffer_returning_cancelled_body_for_test`,
      `dispose_buffer_replacement_for_test`,
      `make_buffer_replacement_stale_after_slices_for_test`) and **preserve**
      them: they drive steps reachable only through a caller workflow or a
      resumed slice turn, which is the programme-level deferred category. Count
      them; do not grow them.
- [x] 3.7 Make every caller from task 3.2a delegate to named operations rather than
      reaching into replacement state. **Two of the five are outside this slot**:
      memory eviction (`WFR-EDITOR-MEMORY`, exempt — call it, do not migrate it)
      and save formatting (`WFR-DOCUMENT-SAVE`, migrated — its facade is at 223 of
      370, so any narration this adds must not push it over, and its structure is
      not this change's to rework). The three in-slot callers are draft restore and
      the two local-history paths. Confirm the facade's exported operation names
      are intent-first, and that a migrated caller needs no more than a renamed
      call.
- [x] 3.8 Write `evidence/mutation-buffer-replacement.md` recording task 3.3's
      outcome with its evidence — either the gain-from-zero numbers for whatever
      pure logic the probe extracted, or the negative finding and why each
      candidate is not separable pure policy. Either way it records the
      `make mutants-diff` invocation used to confirm that
      `model/buffer_replacement.rs`'s existing coverage is **unchanged** by the
      move: the amendment's no-duplication rule means that coverage must neither
      drop nor be re-generated under a new path. Use file-level anchors, never
      line-precise ones.

## 4. `WFR-SESSION-RESTORE` — the row closest to the target shape

- [x] 4.1 From the code, record the coordination jobs. The matrix trace is
      startup → `startup_data` → restore plan → `plan_turn()` →
      `SessionRestoreAdmission` per descriptor → `open_document_from_session_restore`
      per tab, one bounded turn per GTK turn re-armed while `needs_next_turn()`,
      then `release_permit`. Expect `admission` plus `journal` (the session file
      the next startup restores from, per task 2.7). Confirm the persistence half
      (`session_persistence.rs`) and the restore half (`session_restore.rs`) are
      **one** workflow with two stage orders rather than two workflows; if they
      are two stage orders, the stage-order qualifier rule applies and only the
      newly created module is qualified — do not rename a stable sibling for
      symmetry.
- [x] 4.2 **Role home decision.** Derive how many workflows `ui/window/` hosts from
      the matrix rather than asserting a number, then record it: the fixed
      `policy.rs` / `evidence.rs` names cannot be shared across them, so a
      per-workflow subdirectory is expected (`ui/window/session_restore/` or a name
      from the workflow's own domain vocabulary). Record the collision analysis.
      Confirm after the move that the `ui/**/policy.rs` glob resolves and
      `check-workflow-boundaries` passes. That glob is **depth-agnostic and already
      proven at `ui/editor_page/{load,save}/policy.rs`**, so this is a cheap
      confirmation for a first adopter under a new parent directory, not a novel
      risk.
- [x] 4.3 Extract pure policy into that home's `policy.rs`. The bounded-turn
      policy already exists as explicit policy — determine whether it is already
      pure and merely mislocated (a relocation owing **parity** numbers) or
      partly inline in the GTK adapter (an extraction owing **gain-from-zero**
      numbers). Report the two categories separately; do not mix them. Confirm
      `model/session.rs` (8 consumers) stays in `model/` as domain, with the
      consumer count re-derived per task 0.3 and substring false positives named.
- [x] 4.4 Audit `SessionRestorePlanPermit` + `SessionRestoreAdmission` against the
      two-boundary rule and confirm no value is renamed while crossing a seam.
      **Preserve the contract 3b fixed and handed here**: every load terminal
      either carries a parked request's background planning owner into a restart
      or releases it, because `SessionRestorePolicy::release_permit` counts exactly
      those releases to decide when to open the next document. Add or keep a test
      that pins it, and do not re-introduce a drop.

      **Call the load operations 3b named for exactly this**, rather than reaching
      into load state — the handoff table is in 3b's archived `tasks.md`
      appendix B.2, and this task depends on it:
      `load_file_async_with_planning_terminal(path, on_terminal)` is **session
      restore's entry point** and the operation that carries the terminal;
      `cancel_load()` and `dispose_load_resources()` are the two retirement
      entries; `connect_load_completed_once` / `connect_load_failed_once` are the
      named operations that replaced the window's direct
      `.imp().load.*_callback` writes; and **`load_evidence()` is read instead of
      reaching into `imp().load*`**. Also call
      `ui/editor_page/document_identity.rs` and
      `ui/editor_page/restore_position.rs` rather than absorbing either.
- [x] 4.5 Complete the evidence surface. `SessionRestoreEvidence` and its
      `evidence()` accessor already exist and are described as the tree's only
      canonical accessor, so this is folding and finishing rather than inventing:
      fold `SessionRestoreRuntimeSnapshot` in (retiring
      `session_restore_runtime_snapshot_for_test`) so no second typed path
      remains, retire `startup_session_descriptors_pending_for_test`, and take
      over `session.save_failed`, `close_safety_inflight`, `close_safety_bypass`,
      `restore_cancel`, and `failure_detail` as surface fields so the widget
      tests' 18 reads stop reaching into `imp()`.
- [x] 4.6 **Resolve the stored-evidence oddity.**
      `ui/window/imp.rs` holds `Cell<Option<SessionRestoreEvidence>>`
      (`last_restore_evidence`). Decide explicitly whether that is a *last-restore
      outcome record* — legitimate workflow state that the surface projects — or a
      **cached evidence surface**, which the convention forbids because reading
      evidence must derive from live state and must not require the workflow to be
      in a particular stage. If it is an outcome record, rename it so it does not
      read as a cache and record the distinction where the surface is defined.
- [x] 4.7 Honor the three surface obligations (tight-borrow, `try_get()` for any
      template-child-derived field, reentrancy proof test in the correct
      direction) and confirm reading the surface mutates no timer, queue,
      generation, or admission state.
- [x] 4.8 Classify the actuation seams (`restore_session_for_test`,
      `cancel_session_restore_for_test`) and preserve them; they drive startup and
      cancellation paths no headless test can otherwise reach. Count, do not grow.
- [x] 4.9 Write `evidence/mutation-session-restore-policy.md` with the exact
      `make mutants-diff` invocation, before/after generated and killed counts,
      and a per-mutant accounting of any survivor. Distinguish relocation parity
      from gain-from-zero explicitly. File-level anchors only.

## 5. `WFR-LOCAL-HISTORY` — the two-directory row

- [x] 5.1 From the code, record the coordination jobs across **both** halves:
      capture (baseline on first edit, periodic timer) in
      `ui/editor_page/local_history.rs`, and browse/preview/restore in
      `ui/window/local_history.rs`. The trace records six inversions, all
      ticket-guarded; expect more. Expect `execution` (or stage-order-qualified
      `capture_execution` / `preview_execution` if one workflow genuinely owns two
      stage orders of the same shape) plus `journal` for the sidecars per task
      2.7.
- [x] 5.1a **Map the row's real consumer surface before deciding its home**, because
      the row's cells understate it and the home decision depends on it. Beyond the
      two `local_history.rs` files, the workflow is reached from: the **sidebar
      context menu** (`ui/sidebar/workspace_section/context_menus.rs` →
      `workspace_section/mod.rs` → `ui/sidebar/callbacks.rs` → a window callback) —
      an entry point the row's `Entry points` cell omits entirely;
      `ui/window/actions.rs` (`win.show-local-history`);
      `ui/window/documents.rs` at two sites (post-rename sidecar migration, and
      restore-undo); and **`ui/editor_page/save/execution.rs`**, where a Save
      captures a snapshot — a migrated workflow driving this one. Verify each
      against the tree, and note that the row therefore spans **more than two
      directories**, which is the fact task 5.2 must resolve against.
- [x] 5.2 **The role-home decision this row exists to force.** The fixed role
      names are one `policy.rs` and one `evidence.rs` **per workflow**, and this
      workflow spans two directories of *owned* code — plus the wider consumer
      surface from task 5.1a. It therefore cannot own role files in both. Resolve
      it with the split slot 3b already
      used for the recent-documents surface — **the coordination/presentation
      line**: give the workflow one canonical per-workflow role home containing
      its facade, coordination, policy, and evidence, and leave the other
      directory's file as a **called surface** whose ownership is recorded in its
      module doc, exactly as load records `ui/window/documents.rs` and
      `ui/window/encoding.rs` as files it calls and does not own. State which
      half is coordination and which is presentation, and why. **If an honest
      split cannot be made** — if both halves genuinely own ordered coordination
      stages that need pure policy — do not ship two `policy.rs` files for one
      row: escalate as a convention amendment with the measured evidence, and
      pay the four-row retroactive re-check in this change.
- [x] 5.3 Extract pure policy into the canonical home's `policy.rs`: the
      availability/size classification, capture-freshness predicates, retention
      and preview-install decisions, and anything the GTK adapter currently
      decides inline. Confirm `model/local_history.rs` (6 consumers, 173 lines of
      which 63 are tests) stays in `model/` as domain, with the count re-derived
      and substring false positives named. Expect gain-from-zero rather than
      relocation; report the categories separately.
- [x] 5.4 Audit the three existing seam value objects — `BaselineCaptureTicket` +
      `BaselineCaptureFacts`, `PeriodicCaptureTicket` + `PeriodicCaptureFacts`,
      `LocalHistoryReplacementTicket` — against the two-boundary rule, and confirm
      none is renamed while crossing a seam. Note that
      `LocalHistoryReplacementTicket` hands off to `BufferReplacementTicket` from
      task 3: confirm the handoff is a named operation on the migrated buffer
      replacement facade rather than a reach into its state.
- [x] 5.5 Build the evidence surface, folding in **both** pre-convention typed
      observations the matrix names — `LocalHistoryPreviewCoordinatorSnapshot` and
      `LocalHistoryPreviewInstallSnapshot` — rather than leaving second paths.
      Retire the inspection seams across both halves:
      `local_history_preview_install_snapshot_for_test`,
      `local_history_preview_install_delay_for_test`,
      `local_history_baseline_candidate_present_for_test`,
      `local_history_baseline_retry_pending_for_test`,
      `local_history_automatic_capture_inflight_for_test`,
      `local_history_periodic_snapshot_inflight_for_test`,
      `local_history_periodic_timer_pending_for_test`, and
      `has_local_history_restore_undo_for_test`. A surface spanning a workflow
      whose roles live in one directory but whose called surface lives in another
      must still be **one** surface with one accessor.
- [x] 5.6 Collapse the configuration seams into **one** test-policy value behind
      `#[cfg(feature = "test-utils")]`, keeping the public setter names:
      `set_local_history_baseline_failures_for_test`,
      `set_local_history_baseline_delay_for_test`,
      `set_local_history_availability_for_test`,
      `set_local_history_preview_install_delay_for_test`, plus the re-exported
      `set_local_history_preview_read_delay_for_test` — decide whether that
      re-export stays in the service that owns the behavior (the `editor_io`
      precedent says it does) or moves. No override storage may compile without
      the test feature.
- [x] 5.7 Honor the three surface obligations. The capture half reads the editor's
      buffer, so the **`try_get()` disposed-widget rule applies here too**.
- [x] 5.8 Classify and preserve the actuation seams
      (`capture_local_history_baseline_for_test`,
      `run_local_history_periodic_capture_for_test`,
      `delay_baseline_capture_for_test`, `fail_baseline_capture_for_test`).
      Count, do not grow.
- [x] 5.9 Write `evidence/mutation-local-history-policy.md` with the exact
      `make mutants-diff` invocation, before/after counts, survivor accounting,
      and gain-versus-parity separated. File-level anchors only.

## 6. `WFR-DRAFT-RECOVERY` — the largest row, migrated last

- [x] 6.1 From the code, record the coordination jobs for **all three** stage
      orders separately: autosave (first-dirty `SupersedingTimer` → in-flight gate
      that sets `autosave_pending` rather than queueing → candidate collection →
      staged worker pipeline → durable manifest write), restore (startup scan →
      candidate queue → worker body resolution under a disposal reservation →
      `draft_restore_is_current(ticket, facts)` → `apply_draft` → bounded buffer
      install), and orphan cleanup (inspection → execution under manifest reload,
      target guard, and inode recheck). The matrix records **seven distinct worker
      handoffs, the highest inversion count of any workflow**; expect more and
      narrate from the code.
- [x] 6.2 Map the jobs onto role names, applying task 2.7's `journal` verdict. The
      draft manifest and bodies are the record a later stage of this same workflow
      restores from, so `journal` is expected to fit — the name slot 3a reserved
      here after rejecting it for save. Per slot 2b's definition the
      mutation-serialization gate and the byte reservation those writes take live
      **inside** the journal rather than in a separate `admission`. Orphan cleanup
      destroys payloads the workflow is finished with, so check `retirement`
      against it, and check whether the `DraftCleanupContinuation` bounded
      resumable loop belongs with the journal it protects or with retirement.
      Apply the cohesion test, not adjacency. If more than one stage order needs a
      module of the same shape, qualify **only the new one** with the stage order
      it serves.
- [x] 6.3 **Role home decision**, using task 4.2's derived `ui/window/` workflow
      count rather than re-asserting one; a
      per-workflow subdirectory (`ui/window/drafts/`) is expected. Record the
      collision analysis and re-verify glob reach after the move.
      `ui/window/draft_ordering.rs` (119 lines, 69 of them tests) is a pure-policy
      candidate: decide whether it is this workflow's owned policy — in which case
      it becomes part of `policy.rs`, keeping its co-located tests — or
      cross-cutting, in which case it stays and the row records it. Count owning
      workflows, not references.
- [x] 6.4 Extract pure policy into `policy.rs`: autosave admission and the
      in-flight/pending decision, candidate collection and ordering, the draft
      limit, eviction and retention decisions, orphan-cleanup **planning** (the
      pure half — the inode/guard/recheck execution stays coordination), the
      restore freshness predicate, and the mutation-intent classification.
      Confirm `model/draft.rs` (9 consumers, 442 lines of which 207 are tests)
      stays in `model/` as domain, count re-derived and substring false positives
      named. Expect gain-from-zero; report categories separately.
- [x] 6.5 Audit the three existing seam value objects — `DraftRestoreTicket` +
      `DraftRestoreFacts`, `DraftMutationIntent`, `DraftCleanupContinuation` —
      against the two-boundary rule. **Pay particular attention to the archetype
      defect** on the cleanup path: a value that means "the inode recorded at
      inspection" must not be received by a parameter naming it something else,
      because that mismatch is invisible to both review and tests while both names
      denote the same value, and here it would authorize deleting the wrong body.
      Reify anything the audit finds using the established idiom. Confirm no
      `#[expect(clippy::too_many_arguments)]` is introduced; the workspace count
      is 1 and the survivor is the exempt domain catalog constructor.
- [x] 6.6 Build the evidence surface, folding in `OrphanCleanupRuntimeSnapshot`
      rather than leaving a second path, and retiring every inspection seam:
      `draft_autosave_inflight_for_test`,
      `draft_pipeline_max_retained_bodies_for_test`,
      `orphan_cleanup_runtime_snapshot_for_test`,
      `lazy_draft_restore_inflight_for_test`, `draft_restore_inflight_for_test`,
      `draft_delete_tombstoned_for_test`, and
      `draft_mutation_inflight_for_test`. The surface must make the durable path
      observable: manifest authority, autosave in-flight and pending state, the
      restore ticket's identity, retained body weight and its high-water mark,
      cleanup continuation progress, tombstone state, and the terminal outcome of
      each stage order including a refused-as-stale verdict.
- [x] 6.7 Collapse the configuration seams into **one** test-policy value behind
      `#[cfg(feature = "test-utils")]`, keeping the public setter names. This is
      the slot's largest configuration population — authoring counted ten setters
      plus a delay hook in `drafts.rs` and eight more delay/fail hooks in its
      lower half — so verify the count from task 0.3 rather than this list. Note
      the six load-side `test-utils` overrides in `services/editor_io.rs` are
      **shared** with save and load and stay in the service. No override storage
      may compile without the test feature.
- [x] 6.8 Honor the three surface obligations, and treat the disposed-widget rule
      as a live hazard rather than a formality: the autosave lane reads the
      editor's buffer through a template child, and slot 3a's first save surface
      panicked with "Failed to retrieve template child" on exactly this shape in a
      teardown test. A disposed widget is a stage.
- [x] 6.9 Classify and preserve the actuation seams (`autosave_tick_for_test`,
      `schedule_orphan_cleanup_for_test`, `dispose_orphan_cleanup_for_test`, the
      `delay_*` and `fail_*` hooks, and `set_next_draft_body_disposal_probe_for_test`
      as a probe). `autosave_tick_for_test` is the record's own named example of
      the deferred category. Count, do not grow.
- [x] 6.10 **Migrate the widget-test reach-through from task 0.7, drafts side
      first.** Follow slot 3a's finding: **an ungated `imp()` write is usually a
      real drive in disguise** — four of its five write sites became real drives
      once an existing configuration seam held the workflow in flight. Try that
      route for each of the 11 draft writes (`drafts.manifest`,
      `drafts.preloaded`, `drafts.autosave_inflight`, `drafts.autosave_pending`)
      before adding any seam. Every added seam is counted and justified
      individually in the record, and the total must be reported as a delta
      against the corrected pre-change count of **35** sites, not 3a's 40.
      Include the `ui/window/dialogs.rs` seam population in the sweep: it hosts
      three chooser-bound actuation seams and drives the close-save session over
      `imp().session`, so its seams and any reach-through into them are part of
      this slot's classification even though the close-save identity itself
      belongs to migrated save (task 2.4a).
- [x] 6.11 Write `evidence/mutation-draft-recovery-policy.md` with the exact
      `make mutants-diff` invocation, before/after counts, survivor accounting,
      and gain-versus-parity separated. File-level anchors only.
- [x] 6.12 Write `evidence/widget-test-reach-through-migration.md` with the
      per-site categorization for all sites from task 0.7 across drafts, session,
      local history, and buffer replacement, and the before/after ungated-site
      count.

## 7. The two data-safety candidates routed here, and the pass over this diff

- [x] 7.1 **Candidate 1 — `installation_incomplete` versus the draft-autosave
      lane.** Produce the missing evidence 3b named: the `draft_dirty` transition
      trace in `ui/window/drafts.rs`. Establish whether a draft can hold unsaved
      edits the file does not at the moment a cancelled load leaves a partially
      installed buffer, and whether one keystroke after the cancelled-clear
      terminal's `set_modified(false)` can let the next autosave batch write the
      near-empty buffer over that draft. Slot 3a's save path already refuses on
      this flag with `IncompleteLoadInstallation`; decide whether the autosave
      lane needs the same guard. Record the verdict with the trace, and if it is a
      defect, **fix it in this change** per
      `.agents/rules/preexisting-blockers.md` with a regression test pinning the
      guard's precondition and the safety property, and with any deliberately
      un-automated interleaving stated as a deferral with its reason.
- [x] 7.2 **Candidate 2 — the planning completion's dead-editor early return.**
      Produce the missing evidence: prove or disprove that no path skips GTK
      dispose reaching `dispose_load_resources`, from **this** change's scoped
      files, which include the sequencer 3b could not see. Worst case is a stalled
      session-restore sequencer, **never over-admission or lost content** — state
      both, because over-admission is the property `release_permit` exists to
      protect and dropping it from the worst case understates what the
      sequencer guarantees. If an unfired terminal is
      observable, decide whether to release it explicitly rather than depend on
      dispose ordering, and pin it with a test. If it is genuinely unreachable,
      record the proof rather than the assumption.
- [x] 7.3 Re-run the `data-safety` skill in explicit mode over the **actual** diff
      and record every confirmed finding with its severity, its fix, and its
      regression coverage — or a stated reason a deterministic test would trade a
      real bug for an unreliable signal. Confirm that the contracts recorded in
      task 0.5 are byte-for-byte preserved in behavior: the orphan-cleanup
      inode/guard/recheck order, the paragraph-boundary slicing, and the durable
      write ordering with honest `BeforeRename`/`AfterRename` classification.
      Append the before/after comparison to `evidence/durability-contracts.md`.

## 8. Automation: project from evidence without widening

- [x] 8.1 Identify this family's exported surface exactly, from
      `model/automation.rs` and `docs/automation-reference.md` rather than from
      memory: the `local_history` snapshot object (`browse_available`,
      `automatic_capture_available`, `availability`,
      `active_document_file_backed`), `tabs[].draft_present`, the
      `draft-autosave` and `session-restore` readiness blockers, and the
      predicates those blockers gate including `session-restore-complete` and
      `recovery-restore-complete`. Record the pre-change values.
- [x] 8.2 Make those fields project from the new evidence surfaces instead of
      re-deriving from widgets, keeping their names, types, and semantics
      **unchanged**. Where a readiness blocker needs one bool, read it through a
      cheap facade accessor identical by construction rather than building a whole
      surface per tab per poll — the pattern 3a and 3b both used and documented.
- [x] 8.3 Add the corresponding `Evidence Projection Map` rows in
      `docs/automation-reference.md`, keyed by evidence type and attributed by the
      binding the field is read through. **Check whether the drift gate must grow
      again**: 3b taught `scripts/check-automation-docs.py` per-binding
      attribution because two surfaces projected into one `tabs` object;
      `tabs[].draft_present` makes that **three**. If the gate cannot distinguish
      three surfaces projecting into one object, extend it, and prove the
      extension by confirming it still rejects a real rename.
- [x] 8.4 Confirm every other new evidence field — generations, tickets, retained
      weights, continuation progress, tombstones, admission and disposal state —
      is internal and reaches no snapshot. Confirm no draft body, session content,
      or local-history content can reach the schema; the existing redaction tests
      (`draft_body`, `local_history_contents`, `draft_id`) are the contract.
- [x] 8.5 Prove no widening rather than asserting it: run `make automation-smoke`
      on a pre-change tree and on the changed tree under isolated headless Mutter
      and a private D-Bus session with the same fixtures, diff the `tabs[]` keys
      and values, the `local_history` object, and **all** readiness predicates to
      zero differences, and record the normalizations applied and why each is
      about the fixture rather than the contract. Write to
      `evidence/automation-no-widening.md`.
- [x] 8.6 Carry `WFR-AUTOMATION-SPINE` forward as `(partial)`: on slot 4's
      complete ledger line and on slot 5's outstanding line, and in slot 5's
      remaining-scope row. It stays `pending` in the matrix rather than
      `migrated`, because it continues per migrated workflow. Marking it
      `migrated` to satisfy the gate would be a false claim.
- [x] 8.7 Run `make check-automation-docs` and, if the client changed,
      `make automation-client-self-test`.

## 9. Facades, matrix, and record completion

- [x] 9.1 Write each of the four facades' module-doc narration **from the code**,
      not from the census trace, naming every inversion and the point where
      control resumes. Delegate every stage; a facade owns no timer, no admission
      bookkeeping, no generation counter, and no widget mutation. **Each of the
      four facades carries a "State this workflow shares with others" table**, the
      form the load facade established — it is how a reader learns that the
      restore-position group has five owning workflows and that document identity
      belongs to neither document workflow without opening those files. Populate
      each from task 2.4a's state-group split and task 2's shared-ownership
      decisions, and count the table's lines against the budget in task 9.2 like
      any other narration.
- [x] 9.2 **Measure each facade and hold each to 370 physical lines**, measured
      independently. Record all four measurements in
      `evidence/facade-measurements.md`. The draft facade is the risk: three stage
      orders and the programme's highest inversion count. If it does not fit,
      apply the sequence that brought slot 2b from 379 to 369 — delegate stage
      bodies, compress every inversion bullet, fold module-ownership detail into a
      role table, shorten per-method docs that duplicate the module doc — and only
      then **escalate in-change with the measured count**, which costs a four-row
      retroactive re-check. Do not edit the budget line quietly.
- [x] 9.3 **Protect the other facades' headroom.** `ui/search_panel/mod.rs` sits
      at 369 of 370: do not add a physical line to it. Re-measure the save (223)
      and load (253) facades and confirm neither is pushed over.
- [x] 9.4 **Run the rustdoc lint gate.** It is in neither `make check` nor
      `make pre-commit` nor `make check-policy`; CI's `Lint` job enforces it, and
      slot 3a shipped this exact failure. Four new `pub` facades naming their own
      private coordination modules and `pub(crate)` seam types is precisely the
      `rustdoc::private_intra_doc_links` shape. The command is in
      `.agents/rules/build.md`; the fix is always to drop the link and keep the
      name in backticks, never to widen visibility.
- [x] 9.5 Add four `### WFR-*` subsections under `Migrated Workflow Roles`, each
      naming its facade, coordination, policy (or `none` with the cross-cutting
      owner named, for `WFR-BUFFER-REPLACEMENT`), evidence, and mutation-parity
      evidence pointer. **Pointers in live
      `openspec/changes/migrate-user-content-restore-workflow-readability/evidence/<file>.md`
      form** — an archive-prefixed pointer fails the gate immediately because the
      archive directory does not exist yet. Rewriting them to archive form is part
      of archiving.
- [x] 9.6 Update each row's `Current size`, **`Entry points`**, `Seams (i/c/a/p)`,
      `Seam value object`, `Evidence surface`, `Owned pure policy`, `Risk`, and
      `Status` cells from tasks 0.3 through 6, naming the pooled populations the
      old cells had shared and the rows that share them. `Entry points` is not
      optional here: `WFR-LOCAL-HISTORY`'s cell omits the sidebar context-menu
      path and the Save-origin capture that task 5.1a found, and an entry point
      missing from the census is how slot 3b discovered an outright census gap.
- [x] 9.7 Update the `Seam Value Objects` section for every seam this change
      reified or re-audited, and the `Workflow Stage Traces` entries so each names
      the real inversion count from task 0.4 rather than the census floor.
- [x] 9.8 Update the `Policy Module Census` for `model/draft.rs`,
      `model/session.rs`, `model/local_history.rs`, and
      `model/buffer_replacement.rs`: corrected consumer counts, the confirmed
      domain/cross-cutting decisions, and the `next_install_boundary` outcome from
      task 2.1. Leave pointers where a reader following the old snapshot would
      otherwise think a decision is still open.

      **One correction is mandatory and already identified.** The
      `buffer_replacement.rs` census row reads
      `2 (WFR-LOCAL-HISTORY, Replace All undo)` and is wrong in **both** halves:
      the count is four owning workflows, and Replace All undo is not among them.
      Replace it with task 3.2a's verified set — `WFR-DRAFT-RECOVERY`,
      `WFR-LOCAL-HISTORY` (restore and undo), `WFR-EDITOR-MEMORY` (exempt), and
      `WFR-DOCUMENT-SAVE` (migrated) — plus the `WFR-DOCUMENT-LOAD` consumers of
      the pure module, and correct the consumer-file list beside it. This is the
      row whose entry the new "name the other owning workflows" sentence governs
      first, so a wrong set here would discredit the sentence on its first use.
- [x] 9.9 Advance `docs/next/workflow-readability.md`: flip slot 4's ledger line
      to `complete` with `WFR-AUTOMATION-SPINE (partial)`, add
      `WFR-AUTOMATION-SPINE` to slot 5's outstanding line and remaining-scope row,
      record the change name in the slot/name table, update the status paragraph
      and the remaining-scope table, and add a **"Baseline after slot 4"** table
      reporting workflows migrated, share of `ui/` + `model/` migrated with the
      corrected footprints, relocation candidates remaining, seams addressed,
      seams reified, long signatures shortened, automation projections, facade
      budget outcomes for all four facades, role names and homes used, and any
      convention change.
- [x] 9.10 Add a **"Convention friction slot 4 hit, recorded for slots 5 through
      7"** section. Candidates already visible: four facades in one change against
      one budget; the first `policy: none` row; the first two-directory row and
      how the coordination/presentation split resolved it; whether `journal` fit
      all three durable rows; whether census cells were wrong again and in which
      direction; the retroactive-amendment cost now standing at **eight** rows;
      whether a census cell was wrong about *ownership* rather than only about
      counts, as `buffer_replacement.rs`'s "Replace All undo" entry was; whether
      the shared imp state groups split cleanly; and whichever of the two routed
      candidates turned out to be real. Also record the cost warning for slot 5
      explicitly.
- [x] 9.11 Update `AGENTS.md`, `README.md`, and any `.agents/rules/*.md`,
      `.agents/skills/**`, or `docs/**` reference naming a moved path or a retired
      seam. **Grep `scripts/accessibility_warning_allowlist.py` for every renamed
      module that logs** — 3b's A.9c records that the allowlist keys on module
      paths, and a rename silently turns an expected `tracing::error!` into an
      "unexpected warning". A grep at authoring found **no current coupling to
      this family's modules**, so budget this as a confirmation rather than as
      work; if a coupling has appeared, update it and re-verify it still
      **rejects** both an unrelated path and the stale module name so it has not
      become a blanket
      match.

## 10. Verification

- [x] 10.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries`, `check-automation-docs`,
      `check-accessibility-policy`, `check-visual-proof-policy`, and
      `check-filesystem-boundary` — the last because this family owns durable
      writes and sidecar mutation.
- [x] 10.2 The rustdoc lint gate from task 9.4, clean. Recorded as its own line
      because it is CI-only and has already been shipped broken once.
- [x] 10.3 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines**, and no retry relied upon. A recovered flake is a blocker under
      `.agents/rules/preexisting-blockers.md`: root-cause it, fix the cause, and
      rerun in isolation. Record before/after project test counts in
      `evidence/test-counts.md`; the count must not decrease.

      **Carry 3b's flake-causation lesson so a flake here is diagnosed rather than
      rediscovered.** 3b hit one load-amplified flake whose cause was a **shared**
      wait budget too tight for a heavier module under parallel load; it raised
      that one helper deliberately and left the other budgets alone. This change
      adds real work to the heaviest widget module in the tree
      (`tests/widget/window.rs`), so a timeout there is the expected shape.
      Classify the wait first — synchronous UI flip versus async
      `spawn_blocking_then` or realization — then fix the cause per the
      `gtk-testing` skill's Flake Discipline: adequate budget for async waits,
      correct predicate, **shared** helper from `tests/widget/common.rs` and never
      a private copy, and rerun **in isolation** to separate a real break from
      load. A load-amplified flake is still a real fragility.
- [x] 10.4 `make mutants-diff` clean, with the four evidence files from tasks 3.8,
      4.9, 5.9, and 6.11 attached, every survivor accounted for, and
      gain-from-zero reported separately from relocation parity. Confirm every new
      `policy.rs` is reachable by `examine_globs` and imports no GTK-family crate.
- [x] 10.5 **Restore behavior equivalence**, each case with a test asserting the
      user-visible outcome and the resulting buffer or manifest content, recorded
      in `evidence/restore-behavior-equivalence.md`:

      - crash recovery of a file-backed draft and of an untitled draft;
      - a draft large enough to require chunked installation, and one whose
        largest paragraph exceeds the slice budget (which must install in one turn
        per the paragraph-boundary contract);
      - first-dirty autosave, a superseded autosave, an autosave that fails, and
        an autosave whose editor closes before the pipeline returns;
      - orphan-body cleanup where the inode matches, and where an autosave
        replaced the body between inspection and execution (which must **not**
        delete);
      - session restore of zero, one, and many descriptors; a cancelled restore; a
        restore whose editor closes mid-turn; and a restore whose session file is
        malformed;
      - local-history baseline capture, periodic capture, a stale capture rejected
        by its ticket, a preview install, a restore, and a restore undo;
      - a buffer replacement cancelled mid-slice, and one whose caller is gone
        when a slice resumes.

      Cover the state extremes the UI rules require for collection surfaces: no
      drafts, no snapshots, no session; one; and many with long paths.
- [x] 10.6 `make crash-recovery-smoke` clean. This is the family's own end-to-end
      lane: real-process `SIGKILL` and relaunch with the same app data. Note that
      `scripts/crash-recovery-smoke-driver.py` asserts against **draft manifest
      layout** and **`tabs[].draft_present`**, so it is coupled to this family's
      durable format and to the one tab field this change re-sources from
      evidence. Confirm the driver still passes unmodified; if it needs a change,
      that is a signal the format or the projection moved, which the non-goals
      forbid — investigate rather than loosening the assertion.
- [~] 10.7 **Lane passes; the baseline comparison is DEFERRED.** `make performance-smoke` clean, plus a Criterion baseline comparison
      recorded with the invocation used. Bounded installation and the draft
      pipeline are performance contracts, and the performance-smoke fixtures
      already cover malformed metadata, pending migrations, duplicate sidecars,
      many local-history lineages, and first-dirty autosave persistence.

      Lane **PASS**, including the two-startup multi-page draft-repair survival
      proof and the headless reentrant buffer-replacement proofs. The Criterion
      timing comparison is recorded in appendix A.14 as **not interpretable on
      this machine**: every family regressed uniformly with zero improvements
      while the user's game and two Go test suites saturated the CPU at load
      average 13.77. A meaningful `make bench-compare` is outstanding and blocked
      on the same quiet machine as task 10.10.
- [x] 10.8 `make test-prop` if any property target is touched — it is gated behind
      `required-features = ["property-tests"]` so no default lane runs it.
- [x] 10.9 The mandatory proof lanes for `ui/` and widget-test changes, each from
      a **clean artifact root**: `make visual-geometry-smoke`,
      `make accessibility-smoke`, `make visual-smoke`. Order these **after all
      source, documentation, and rules edits**: the accessibility policy gate
      fingerprints the *contents* of accessibility-relevant files, so any edit
      after a lane runs voids the proof and the lane must be rerun. If a lane
      fails wholesale, suspect a stale shared-`target/` artifact before suspecting
      the change — 3b lost a run to a `env!("CARGO_MANIFEST_DIR")` path baked from
      a deleted worktree, and the failure named a GSettings schema, not a path.
- [~] 10.10 **Live run — DEFERRED FOR LIVE PROOF, pending user availability.**
      `make run` against restored workspaces with real drafts: make an edit and
      let autosave fire; kill and relaunch to exercise recovery; discard and save
      a restored draft from the inline alert; browse and restore local history;
      and resize while the sidebar animates. Watch stderr for
      `Trying to measure GtkBox ...`, pixman, `Gtk-CRITICAL`, `Gtk-WARNING`, and
      `GLib-GObject-WARNING`.

      **Not ticked.** A live-session run was started under isolated XDG
      directories after confirming no LushText instance was running, and it
      interfered with the user's active fullscreen desktop session. The user
      directed that all display-dependent work move to headless paths and that
      this single proof be deferred rather than completed. Every launch was
      terminated, the input socket was removed, and the user's own applications
      were never signalled. The partial evidence obtained before the stop — which
      is genuine real-session evidence and is worth keeping — plus the precise
      remaining scope are recorded in `evidence/live-run.md`. The paned-warning
      acceptance gate for this change is therefore **outstanding**, and the
      change should not be treated as having cleared it.
- [x] 10.11 Cold-read check: with this change's conversation set aside, read each
      of the four facades alone and confirm you can answer "what happens when the
      app restarts after a crash", "what happens on the first dirty edit", "what
      happens when the user restores a snapshot", and "what happens when
      replacement is cancelled mid-slice" without opening a coordination module.
      If any answer needs a second file, the facade is not narrating.
- [x] 10.12 `openspec validate migrate-user-content-restore-workflow-readability
      --strict` passing.

## 11. Handoff

- [x] 11.1 Confirm the programme record and the matrix agree: all four rows
      `migrated` with complete `Migrated Workflow Roles` subsections naming real
      paths, slot 4's ledger line `complete`, `WFR-AUTOMATION-SPINE` carried onto
      slot 5's outstanding line and remaining-scope row, and
      `make check-workflow-boundaries` passing. Record in B.1.
- [x] 11.2 Hand slot 5 (`WFR-WORKSPACE-TREE`, `WFR-NOTES-BOOKMARKS`) the facts it
      needs, in B.2: the named operations on this slot's four facades that notes
      and workspace-tree code should call rather than reaching into; the
      `WorkspaceWatchTicket` and `NotesBrowserTicket` seams still unreified and
      recorded as slot 5's; the sidecar and `NoteSourceAdmission` adjacency this
      change deliberately did not touch; any shared-ownership decision from task 2
      that slot 5 inherits; the role-home precedent for `ui/window/` per-workflow
      subdirectories, now with its first adopters; the corrected per-row census
      method and the pooled populations named; the retroactive-amendment cost now
      standing at **eight** rows; and the reminder to run the rustdoc gate before
      shipping a facade.

---

## Course-correction record

**An earlier pass of this change stopped after sections 0-3 and 7**, on the
reasoning that migrating the crash-recovery startup path without running its
acceptance battery would be worse than not starting it. That boundary was
**rejected on review** and the reasoning was wrong on the facts: the four lanes
cited as unavailable — `make crash-recovery-smoke`, `make automation-smoke`,
`make performance-smoke`, and a live `make run` — are ordinary local make targets
that slots 3a and 3b ran in this same environment, and the live-run protocol
(check for the user's running instance, never kill it, isolated-XDG substitution)
is documented in prior slots' evidence.

Recorded rather than deleted, because the mistake is instructive: **the constraint
was assumed rather than checked.** All four rows are now migrated and all four
lanes are run; see the gate matrix below.

---

## Appendix A — orientation record

Filled in during implementation. Each subsection is required by the task that
names it; leaving one empty means that task is not done.

### A.1 Gate evidence (task 0.1)

Verified mechanically on a clean tree at HEAD `1769119`:

- `openspec/changes/archive/` contains `2026-08-25-normalize-workflow-readability-boundaries` (slot 1), `2026-08-25-migrate-command-palette-workflow-readability` (2a), `2026-08-25-complete-search-replace-workflow-readability` (2b), `2026-08-26-migrate-document-save-workflow-readability` (3a), and `2026-08-26-migrate-document-load-workflow-readability` (3b).
- `openspec/specs/` holds `workflow-readability-boundaries`, `workflow-evidence-surfaces`, `gtk-adapter-module-boundaries`, `mutation-testing`, and `dbus-automation-spine`.
- `docs/workflow-readability-matrix.md` marks `WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`, and `WFR-DOCUMENT-LOAD` `migrated`, each with a `Migrated Workflow Roles` subsection.
- `docs/next/workflow-readability.md`'s slot ledger marks 1, 2a, 2b, 3a, 3b complete and slot 4 outstanding.
- `make check-workflow-boundaries` passes: *"4 workflow policy module(s) are pure and mutation-scoped, every migrated matrix row names complete, existing roles, and the programme record's slot ledger agrees with the matrix"*.

The two-proof rule for tier-3 is satisfied five times over.

### A.2 Premise re-verification, four rows (task 0.3)

Full derivation in `evidence/census-reverification.md`. Summary: all four rows' `Current size` cells were **too high**, in every case because the census pooled whole shared service files. Row-scoped production sizes are buffer replacement **976**, session restore **1,297**, local history **2,461**, draft recovery **2,297**. All four pure-policy "consumer counts" were **consuming-file counts, not owning-workflow counts**; re-derived as owning workflows they are 2, 1, 2, and 3/4 respectively, and all four modules stay where they are — three because a service depends on them (the 3b `file_load.rs` dependency-direction precedent), one because it is cross-cutting.

### A.3 Shared-ownership decisions, including the three imp state groups (tasks 2.1–2.6, 2.4a)

Full text in `evidence/shared-ownership-decisions.md`. Headlines:

- **2.1** the `next_install_boundary` alias **stays** as a named domain synonym; it is a one-line delegation with no body to drift, and removing it would edit a migrated workflow's call sites and doc links for no gain. Its doc comment now names the cross-cutting owner and the paragraph-boundary contract.
- **2.2** `ui/window/startup_data.rs` is owned by **neither** slot-4 row — the task's framing assumed it was one of the two. It is the startup format-upgrade gate, whose census home is `WFR-NOTES-BOOKMARKS` (slot 5); it *calls* both rows' entry points from `continue_startup_data_flow` and shares no state group with either.
- **2.3** `services/recovery_metadata.rs` stays in services, shared by all three durable rows. But `startup_recovery_status_message` in `session_persistence.rs` is pure classification and moves into the session row's `policy.rs` as a gain-from-zero extraction.
- **2.4** the session row owns `save_failed`; 3a's lesson carried forward verbatim — *a field whose name contains "save" is not thereby save-workflow state.*
- **2.4a** `SessionState` splits **three** ways, not two: 12 session-owned fields, `tab_projection_publications` shared with the tab workflow, the close-save identity pair left with migrated save as its recorded owner, and `close_safety_inflight`/`close_safety_bypass` **genuinely shared between the draft and session rows** — task 4.5's claim on both is refuted by the code and the doc comments were right. `DraftState` is entirely draft-owned; the session worker's three-field write becomes one named draft operation. `LocalHistoryState` is local-history-owned with save and load keeping named operations.
- **2.5/2.6** closed boundaries confirmed; notes/format-upgrade adjacency recorded where a reader will hit it.

### A.4 Current ordered stages and real inversion counts, four workflows (task 0.4)

Full traces in `evidence/stage-traces.md`, written from the code. Deferred inversions counted separately from synchronous ownership hand-outs, because the convention only asks the facade to name the first.

| Row | Census | Real deferred | Delta |
| --- | --- | --- | --- |
| `WFR-BUFFER-REPLACEMENT` | 1 | **1** mechanism, 3 phase resume points, 4 hand-outs | **0** — the only row of the four the census got right |
| `WFR-SESSION-RESTORE` | 1 | **7** | +6 |
| `WFR-LOCAL-HISTORY` | 6 | **16** | +10 |
| `WFR-DRAFT-RECOVERY` | 7 | **17** | +10 |

The census counted only `spawn_blocking_then` sites. The 26 it missed are timers, main-loop polls, disposal capacity wakeups, chunked buffer snapshots, and buffer-replacement terminals.

### A.5 Durability contracts as implemented today (task 0.5)

Recorded verbatim in `evidence/durability-contracts.md` before any edit. The orphan-cleanup order is confirmed stronger than the rule's minimum: manifest write lock → trusted-manifest reload → per-candidate path-mismatch and duplicate-ID refusals → `TargetWriteGuard::acquire` → **then** `fs_metadata::inode` recheck → delete. The paragraph-boundary contract has one owner and four call sites, one of them through the 2.1 alias. Durable write ordering and the `BeforeRename`/`AfterRename` classification honesty are recorded with the three draft-specific orderings that depend on them.

### A.6 Amendment basis and the four-row retroactive re-check (tasks 1.1, 1.3)

Recorded in full as `### Slot 4 amendment re-check` in `docs/workflow-readability-matrix.md`.

**Basis confirmed before amending.** (a) `scripts/check-workflow-boundaries.py` lists `policy` in `OPTIONAL_ROLE_VALUES`, so `policy: none` was already gate-tolerated, and no scenario in `openspec/specs/workflow-readability-boundaries/spec.md` states such a row is complete — the permission read as tolerance, not convention. (b) Three friction-section instructions say to re-derive: slot 3b's "**Re-derive, and expect the answer to move in either direction**", slot 3a's "**Re-derive row-scoped counts before sizing evidence work**", and slot 3a's adjacent "**Write the narration from the code, every time**".

**Verdicts, per row, checked individually.**

- Amendment (a) is a **confirmation for all four**: none declares `policy: none`; each declares a relocated or extracted `policy.rs`.
- Amendment (b) is **not** a confirmation. `WFR-DOCUMENT-SAVE` and `WFR-DOCUMENT-LOAD` are compliant. `WFR-SEARCH-REPLACE` had a **gap** — its size cell still carried the census figure with a pooled `services 5,895` subtotal and no sharing rows named. `WFR-COMMAND-PALETTE` had a **partial gap** — the `ui` subtotal was re-derived and one misattribution named, but `services 7,897` stayed pooled.

**Both gaps filled in this change**, in the re-check section and in the two product-matrix cells: `WFR-SEARCH-REPLACE` is **14 files, 5,527 production lines**; `WFR-COMMAND-PALETTE` is **10 files, 2,534 production lines**; and each names its pooled service population with the rows that share it (notably `services/palette/notes.rs`, 2,163 lines shared with `WFR-NOTES-BOOKMARKS`, so slot 5 does not re-derive it from the other side). One measurement trap is recorded with them: `services/palette/tests.rs` is 1,223 lines behind `#[cfg(test)] mod tests;` in its own file, which a naive per-file production scan counts as production.

This is the **second consecutive amendment** where "it must already hold" was not a discharge.

**Task 1.4 — other settled conventions re-confirmed unchanged**: facade budget 370; bounded coordination role set unchanged (slot 4's `journal` verdict used the existing name for all three durable rows and needed no addition); seam value-object shape; evidence-surface visibility rule; slot 3b's reentrancy constraint; cross-cutting eligibility, whose already-implied consequence is all amendment (a) states; evidence pointer form; and the per-workflow subdirectory role home, with slot 4 as its third through sixth adopters.

**Task 1.5 — standing guidance updated**: `.agents/rules/rust.md` gains the complete-row-without-`policy.rs` case and the shared-arithmetic rule inside "Policy purity", plus a new "Re-deriving a row's measured cells" section; `.agents/rules/documentation.md` gains the re-derivation obligation on the workflow-structure trigger. `make check-agent-docs` and `make check-agent-skills` pass.

### A.7 Widget-test reach-through sites and categorization (tasks 0.7, 6.10, 6.12)

Full per-site table in `evidence/widget-test-reach-through-migration.md`. The corrected pre-change baseline is **35** sites (21 `.imp().drafts.`, 14 lines / 15 field occurrences of `.imp().session.`), all in `crates/lushtext/tests/widget/window.rs`. Both sweeps 3a did not catalogue are **zero** and recorded as zero: no `local_history`/`replacement` reach-through in `editor_page.rs`, and none in any widget module other than `window.rs`.

**Zero session writes** — every session site is a read. The 11 draft writes split into 8 journal-record installs needing counted actuation seams and 3 that become real drives through existing configuration seams, per slot 3a's finding.

### A.8 Coordination role mapping per workflow, and the `journal` verdict (tasks 2.7, 3.1, 4.1, 5.1, 6.2)

`journal` **passes outright for all three durable rows** — the draft manifest and bodies, the session file, and the local-history sidecars are each read back by a later stage of their own workflow. `gtk-adapter-module-boundaries` needs no amendment. Mapping table and the per-row reasoning are in `evidence/shared-ownership-decisions.md`.

One finding worth carrying: **orphan cleanup is `journal`, not `retirement`.** Task 6.2 asked the question; applying the cohesion test answers no. `retirement` in this codebase means the disposal lane's off-GTK destruction of an in-memory payload, while orphan cleanup reloads the same manifest under the same `manifest_write_lock` the journal's writes take, is gated by the same authority, and merges back into the same record. `DraftCleanupContinuation`'s `manifest_offset` is an offset into that record, so the bounded loop goes with the journal it protects.

### A.9 Data-safety passes and the two routed candidates (tasks 0.6, 7.1, 7.2, 7.3)

Full evidence in `evidence/data-safety.md`.

**Candidate 1 — CONFIRMED DEFECT, fixed in this change.** The draft-autosave lane never consulted `installation_incomplete`. A cancelled bounded load installation empties the buffer and clears `modified` **without clearing `draft_dirty`**, so one keystroke afterwards made a near-empty buffer look like an ordinary dirty candidate and the next autosave batch wrote it over a draft still holding real unsaved work. Demonstrated, not argued: with the guard removed the regression test reports the draft body becoming `Some("x")` where it should stay `Some("important unsaved work\n")`. The guard now sits at five decision points (the autosave and close admission passes, the synchronous close flush, and both post-snapshot rechecks) and is evaluated from three call sites, because the two admission passes share one collector and the close flush calls `policy::draft_candidate_is_eligible` rather than restating its terms — so an added term reaches every point automatically. The candidate is **skipped rather than deferred**, and `window::test_incomplete_load_installation_blocks_draft_autosave_over_a_good_draft` pins both the precondition and the safety property.

**Candidate 2 — UNREACHABLE, with a proof.** Two independent reasons: `finish_load_planning` sits outside the planning completion's `if`/`else`, so even a stale ticket releases the terminal; and `upgrade()` fails only after finalization, while GObject runs `dispose()` strictly before `finalize()` and `LushtextEditorPage::dispose` unconditionally calls `dispose_load_resources`, which takes and calls the stored callback. The invariant is now recorded at the site, including the corrected worst case: a dropped terminal would **stall** the session-restore sequencer and could never over-admit, because a missing `release_permit` can only under-admit.

### A.10 Automation no-widening proof (task 8.5)

Full detail in `evidence/automation-no-widening.md`.

**What changed:** the `local_history` snapshot object now projects from
`LocalHistoryEvidence` instead of re-deriving from widgets, and the
`session-restore`, `close-safety`, and `draft-autosave` readiness blockers read
their workflows' cheap facade accessors instead of `imp()` fields. Names, types,
and semantics are unchanged.

**The drift gate had to grow, and the growth was proved rather than asserted.**
`local_history` is a third projecting surface, so it was registered in
`scripts/check-automation-docs.py`'s `EVIDENCE_PROJECTIONS`. The extension was
verified by **deliberately renaming `LocalHistoryEvidence::browse_available` and
confirming the gate rejects it** — which it did not before registration, and did
after, naming both the vanished documented field and the undocumented projected
one. The rename was then reverted.

**One contingency did *not* fire, and that is a finding rather than an omission.**
Task 8.3 anticipated `tabs[].draft_present` making three surfaces project into one
`tabs` object. It does not: `draft_present` is a **per-tab document-identity fact**
read through the editor page's existing `draft_id()` operation, while the draft
workflow's evidence surface is **window-level**. Fabricating a projection row for
it would have made the map claim something the projection function does not do,
which is exactly what the gate's "the authority is the Rust snapshot function"
rule forbids. Recorded rather than forced.

### A.11 Role-home collision analyses, including the derived `ui/window/` workflow count and the two-directory decision (tasks 3.2, 4.2, 5.2, 6.3)

Derivation in `evidence/shared-ownership-decisions.md`: every non-plumbing `ui/window/` file mapped to its owning matrix row gives **15 distinct rows in one directory**, so the fixed `policy.rs`/`evidence.rs` names cannot be shared and every slot-4 row there takes a per-workflow subdirectory. `ui/editor_page/` hosts 8 workflows with `save/` and `load/` already holding both fixed names, so `WFR-BUFFER-REPLACEMENT` collides and takes `ui/editor_page/buffer_replacement/`. A prefixed `buffer_replacement_policy.rs` is not available in either directory: it would leave the `ui/**/policy.rs` mutation glob, which is a blocking coverage regression.

### A.12 Facade measurements, four facades (task 9.2)

Measured with `wc -l` on the raw file, which is what the budget counts.

| Facade | Lines | Budget |
| --- | --- | --- |
| `ui/window/session_restore/mod.rs` | **165** | 370 — the slot's smallest, for a row with two stage orders and 7 inversions |
| `ui/editor_page/buffer_replacement/mod.rs` | **168** | 370 |
| `ui/window/local_history/mod.rs` | **215** | 370 — two stage orders across two directories, 16 inversions |
| `ui/window/drafts/mod.rs` | **289** | 370 — **the programme's largest facade**, three stage orders and 17 inversions, with 81 lines spare |
| `ui/search_panel/mod.rs` | 369 | **untouched**; its 1 line of headroom is intact |
| `ui/editor_page/save/mod.rs` | 223 | **untouched** |
| `ui/editor_page/load/mod.rs` | **271** (was 253) | +18, from the `has_incomplete_load_installation` accessor the confirmed data-safety defect required |

**No escalation, and the budget line was not edited.** The draft facade was the
declared risk — three stage orders and the programme's highest inversion count —
and reaching 289 needed exactly the sequence slot 2b recorded: delegate every
stage body, compress each inversion to one line, and fold module-ownership detail
into the role table and the shared-state table rather than the prose.

The load facade grew because the draft workflow now reads the load workflow's
incomplete-installation flag through a named operation rather than reaching into
`imp().load`. That is the convention working as intended: the fix crossed a
workflow boundary, so it crossed through a facade.

### A.13 Lane consequences of the module renames (tasks 9.11, 10.9)

Five modules moved and two were retired:

| Before | After |
| --- | --- |
| `ui/editor_page/buffer_replacement.rs` | `ui/editor_page/buffer_replacement/{mod,execution,policy,evidence}.rs` |
| `ui/window/session_restore.rs` + `ui/window/session_persistence.rs` | `ui/window/session_restore/{mod,journal,admission,execution,policy,evidence}.rs` |
| `ui/window/local_history.rs` | `ui/window/local_history/{mod,journal,preview_execution,restore_execution,policy,evidence,test_policy}.rs` |
| `ui/window/drafts.rs` + `ui/window/draft_ordering.rs` | `ui/window/drafts/{mod,journal,admission,autosave_execution,restore_execution,seams,policy,evidence,test_policy}.rs` |

- **`scripts/accessibility_warning_allowlist.py`** — checked, because 3b's A.9c
  warns that a rename can silently turn an expected `tracing::error!` into an
  "unexpected warning". The allowlist keys on exactly one module path under the
  moved trees' vicinity — `ERROR lushtext_core::ui::editor_page::load::execution` —
  and **that module did not move**, so the coupling is intact and needs no edit.
  Every `tracing::error!` in the four migrated workflows was checked against it:
  none is allowlisted, because none is expected.
- **`.cargo/mutants.toml`** — no edit needed. The `ui/**/policy.rs` glob is
  depth-agnostic and resolved at all four new paths, including the first three
  under `ui/window/`. Confirmed twice: `make check-workflow-boundaries` reports
  **8** policy modules (up from 4), and `make mutants-list` names 248 mutants
  across them.
- **`scripts/check-automation-docs.py`** — one registry entry added for the new
  `local_history` projection, and the extension proved by rejection.
- **`scripts/crash-recovery-smoke-driver.py`** — **unmodified**, which is the
  point: it asserts against draft manifest layout and `tabs[].draft_present`, so
  it passing unchanged is direct evidence that neither the durable format nor that
  projection moved.
- **`docs/workflow-readability-matrix.md`** — every stale path claim corrected;
  `make check-workflow-boundaries` validates that every claimed path exists, so
  this is machine-checked rather than eyeballed.
- **`AGENTS.md`** — the `ui/window/` and `ui/editor_page/` layout blocks name the
  four new role homes, and the local-history browser's file reference is updated.
- **`README.md`** — the readability-convention section names the new adopters and
  points at buffer replacement as the smallest complete example.

### A.14 Benchmark comparison (task 10.7)

**The lane passes. The timing comparison is not interpretable on this machine, and
that is stated rather than resolved in either direction.**

Invocation:

```
$ timeout 3600 make performance-smoke
PASS: performance smoke completed for filters 'file_index_search
palette_pipeline_hardening_100000 file_index_rebuild end_to_end_boundedness
quality_gap_scale content_search_smoke search_interactive_policies
markdown_render_planning save_admission_policy editor_memory_policy
json_persistence editor_file_io transient_file_load workspace_watch_pressure
replace_preview_generation replace_undo_workflows recovery_performance'
Artifacts: build/smoke/performance
```

The lane's own assertions all passed, including the ones this change is most
exposed to: the **two-startup multi-page draft-repair survival proof**, the
**headless reentrant buffer-replacement proofs**, the **bounded editor-load slice
cancellation proof**, and the **headless workspace-persistence fault matrix**.
Those are pass/fail behavioral proofs, not timings, so they are unaffected by what
follows.

**Why the Criterion deltas are being discarded as input.** Every family in the run
reported `Performance has regressed`, with **zero** improvements and **zero**
no-change results:

| Family | regressed | improved | no change |
| --- | --- | --- | --- |
| `recovery_performance` | 9 | 0 | 0 |
| `file_index_search` | 14 | 0 | 0 |
| `editor_file_io` | 24 | 0 | 0 |
| `json_persistence` | 6 | 0 | 0 |
| `replace_preview_generation` | 5 | 0 | 0 |
| `markdown_render_planning` | 4 | 0 | 0 |
| `content_search_smoke` | 1 | 0 | 0 |

The magnitudes cluster tightly in the +26% to +72% band across all of them. **This
change cannot produce that shape.** It touches drafts, session restore, local
history, and buffer replacement; it does not touch file index search, content
search, Markdown render planning, or editor file I/O, and a uniform proportional
slowdown of unrelated pure-computation benchmarks is the signature of a slower
machine, not of slower code.

The machine was measured while the comparison ran, and the cause is concrete
rather than assumed:

```
$ cat /proc/loadavg
13.77 15.95 15.02 8/11326

$ ps -eo pcpu,pid,args --sort=-pcpu | head -4
299  goplint.test        -test.parallel=4        # user's unrelated Go suite
196  goplint-race.test   -test.parallel=2        # user's unrelated Go suite
96.3 dota2                                       # user's fullscreen game

$ cat /sys/devices/system/cpu/cpufreq/policy0/scaling_governor
powersave
```

A game at ~96% of a core plus two Go test binaries at ~300% and ~196%, on a
`powersave` governor, at load average 13.77. Re-running now would be equally
invalid, and `build.md` is explicit that this lane "should stay forgiving enough to
avoid routine shared-runner noise" — which is why it passed.

**The distinction being drawn deliberately.** `preexisting-blockers.md` forbids
blaming the machine for a *flaky test*, and that rule is not being sidestepped
here: no test failed. What is being set aside is a *timing measurement* taken on a
saturated machine, which is invalid input rather than a tolerated failure. The
honest options were to claim no regression, to claim a regression, or to say the
measurement cannot support either claim; the third is the true one.

**What remains outstanding.** A meaningful `make bench-compare` against the
`main` baseline (which exists on disk, including
`recovery_performance/first_dirty_autosave_persist_manifest_batch/main`, dated
2026-07-18) needs a quiet machine. It should be run alongside the deferred
live-session proof in task 10.10, since both are blocked on the same thing: the
user's machine not being in active use. The performance risk it would cover is
narrow and specifically named, so a reviewer knows what to look at: the
first-dirty autosave persist path and the bounded-installation slice loop, both of
which the lane's behavioral proofs already exercise for correctness and
boundedness.

### A.15 Cold-read result, four questions (task 10.11)

Each question was answered from **one** facade's module documentation, without
opening a coordination module.

| Question | Answered by | Where |
| --- | --- | --- |
| What happens when the app restarts after a crash? | `session_restore/mod.rs` stages 5–9, handing to `drafts/mod.rs` stage order B | the session record and the draft manifest are read in **one** worker pass "because the descriptors and the draft manifest have to agree", then pages mount four per turn in persisted order |
| What happens on the first dirty edit? | `drafts/mod.rs` stage A1 | a 750 ms first-dirty timer, armed sooner than the 5 s tick "because brand-new unsaved work is the most valuable and least protected" |
| What happens when the user restores a snapshot? | `local_history/mod.rs` stage B8 | the **current** buffer is captured as a `RestoreSafety` snapshot *first* — "a restore never destroys what it replaces" — then installed through `replace_buffer_bounded` |
| What happens when replacement is cancelled mid-slice? | `buffer_replacement/mod.rs` stage 6 | the body returns to its owner with its disposal reservation intact; a session that **has already mutated** must empty the partial buffer in bounded turns first, "the whole reason the workflow has a fourth phase" |

Two observations from reading them cold rather than as author.

**The facades cross-reference by named operation, not by field.** The draft facade
says startup records arrive "through `adopt_startup_draft_records`" and the session
facade says the draft half "is handed over through the draft workflow's own named
operation". Reading either one alone, the boundary is visible and the other side is
findable by name — which is what makes the two-workflow crash-restart answer
readable from one file.

**The narration earns its length by carrying the *reason*, not the mechanism.**
Every stage that would otherwise read as arbitrary states why: paragraph-boundary
slicing "keeps recovering a 33 MB single-line draft linear instead of quadratic";
a 1 ms timeout beats an idle source because "an idle source can starve behind
higher-priority work while the buffer is still half-mutated"; cleanup refuses
untrusted metadata because it "deletes user content". Those are the sentences a
future reader would otherwise have to reconstruct from the code, and they are the
reason the draft facade is the programme's largest at 289 lines and still inside
the 370 budget.

### A.16 Tail simplify pass, after full verification

Run against the verified tree, before commit. Every edit had to be provably
behavior-equivalent, because the tree already carried green gates and live smoke
proof; anything needing judgement about ordering was routed to B.3 instead.

**Duplicated logic removed** — in each case the same computation existed twice in
code this change itself introduced:

| Where | What was duplicated | How it was resolved |
| --- | --- | --- |
| `drafts/autosave_execution.rs` | `collect_dirty_draft_candidates` and `collect_close_draft_candidates` were line-for-line identical except the `require_draft_dirty` argument and the discard skip | both `pub(super)` entry points kept, each now one line over a shared `collect_draft_candidates`. Step order preserved verbatim: eligibility, draft id, discard, **then** `advance`, so a skipped tab still never consumes a mutation epoch |
| `drafts/journal.rs` | `flush_dirty_drafts` restated the eligibility terms as a `||` chain — the one `installation_incomplete` decision point that copied the logic instead of calling it, and the only one outside the mutation scope | calls `policy::draft_candidate_is_eligible(.., false)`, which is exactly equivalent because the `require_draft_dirty = false` expansion is `modified && !evicted && !installation_incomplete`. A term added to the policy now reaches the close flush — the worst place to miss one — automatically |
| `session_restore/journal.rs` | the 9-line startup-read cancel/identity predicate appeared verbatim at both the pre-dispatch guard and the worker-completion guard | one `startup_journal_read_superseded`, same operands, same short-circuit order |
| `local_history/restore_execution.rs` | two adjacent identical 3-statement browser rollbacks | `return_snapshot_to_browser(state)`. The error arm clones the window **before** the move so its message still publishes after the rollback, preserving observable order |
| `local_history/preview_execution.rs` | the `Missing` and `Err` arms were the same 4-call sequence differing only in a title that is also the accessible name | `show_preview_error(title)`, which is what stops the visible label and the announced name drifting apart |
| `local_history/journal.rs` | two user-facing strings published from two sites each, one restating the 50 MB browsing threshold in prose | named consts beside their only publisher |

**Dead logic and factual defects removed:** an `if had_manifest_updates { clear;
return } clear; Ok(())` whose arms were identical; the `buffer_replacement/mod.rs`
caller table pointing at `ui/window/drafts.rs` and `ui/window/local_history.rs`,
**two files this change deletes**; two consecutive identically-`cfg`'d single-item
re-exports in `editor_page/mod.rs`; and one widget-test predicate reading the
evidence surface twice on a 113-character line.

**Measured cells were wrong before this pass, and in three different directions.**
Re-deriving all four rows found the matrix, `tasks.md`'s facade table, and the tree
disagreeing: facades read 279/167/216 in the matrix and 310/167/216 in the facade
table against an actual 289/168/215; `execution 913 (of 964)` was actually
`921 (of 972)`; the capture surface read 706 against 707. All four row totals, the
facade table, and `durability-contracts.md`'s pasted guard-site grep — whose two
post-snapshot labels were also swapped — now match the tree. This is the drift the
re-derivation obligation exists to catch, and it appeared *within* the change that
performed the re-derivation, which is worth stating plainly: a figure copied
forward once during authoring reads as authoritative afterwards.

## Appendix B — handoff

### B.1 Programme and matrix agreement (task 11.1)

`make check-workflow-boundaries` **passes**, which mechanically confirms all of:

- all four slot-4 rows are `migrated` with complete `Migrated Workflow Roles`
  subsections naming facade, coordination, policy, evidence, and mutation-parity
  roles at paths that **exist**;
- the programme record's slot ledger agrees with the matrix — slot 4's line is
  `complete` with `WFR-AUTOMATION-SPINE (partial)`, and that row is carried onto
  slot 5's outstanding line and its remaining-scope row;
- **8** workflow policy modules are pure and mutation-scoped, up from 4 before this
  change, so every new `policy.rs` is reachable by `examine_globs` and imports no
  GTK-family crate.

`WFR-AUTOMATION-SPINE` stays `pending` in the matrix rather than `migrated`,
because it continues per migrated workflow. Marking it `migrated` to satisfy the
gate would be a false claim.

### B.2 To slot 5 (`WFR-WORKSPACE-TREE`, `WFR-NOTES-BOOKMARKS`) — task 11.2

**Named operations on slot 4's four facades** that notes and workspace-tree code
should call rather than reaching into:

| Need | Operation |
| --- | --- |
| install a whole buffer | `LushtextEditorPage::replace_buffer_bounded`, and read `buffer_replacement_evidence()` |
| observe or drive session restore | `session_restore_evidence()`, `session_restore_in_progress()`, `close_safety_in_progress()` |
| observe drafts | `draft_evidence()`, `draft_delete_is_tombstoned(&str)`, `draft_workflow_blocks_readiness()` |
| hand draft records over from a startup read | `adopt_startup_draft_records` |
| observe or influence local-history capture | `local_history_evidence()`, `suspend_local_history_capture()` / `set_local_history_capture_suppressed(bool)`, `local_history_{editor,path,edit}_generation()` |
| browse local history for a path | `show_local_history_for_path` — **the sidebar's existing entry point**, whose omission from the census cell slot 4 corrected |

**Files and decisions slot 5 inherits:**

- **`ui/window/startup_data.rs` is slot 5's**, not slot 4's. It is the startup
  app-data format-upgrade gate, and `WFR-NOTES-BOOKMARKS`'s row title already
  claims format upgrade. It shares no state group with either restore row; it
  *calls* `load_session_and_drafts` and `start_autosave_timer` from one function.
  Full reasoning in `evidence/shared-ownership-decisions.md` §2.2.
- **`services/palette/notes.rs` is 2,163 production lines shared** between
  `WFR-COMMAND-PALETTE` and `WFR-NOTES-BOOKMARKS`, named as a pooled population by
  slot 4's amendment re-check so slot 5 does not re-derive it as wholly its own.
  The measurement trap that goes with it: `services/palette/tests.rs` is 1,223
  lines behind `#[cfg(test)] mod tests;` **in its own file**, which a naive
  per-file production scan counts as production.
- **`services/file_tree.rs` carries 11 pre-existing surviving field-deletion
  mutants** and belongs to `WFR-WORKSPACE-TREE`. They are baseline, not
  regressions, and slot 5 inherits them.
- **`cargo-mutants` 27's `--re` does not filter struct-field-deletion mutants**,
  so a "focused" run carries every field-deletion mutant in scope as a floor.
  Budget for it and do not attribute its survivors to your change.
- **`WorkspaceWatchTicket` and `NotesBrowserTicket` remain unreified** and are
  slot 5's, unchanged by slot 4. The `NoteSourceRefreshCoordinator` retirement slot
  2a deferred is still open, and the reason is still that deduping the type
  changes `NotesBrowserRuntimeSnapshot`'s shape.
- **The notes/sidecar adjacency slot 4 deliberately did not touch**:
  `ui/window/local_history/restore_execution.rs` calls `resolve_notes_for_editor`
  from two restore terminals, and `journal.rs` records a `MigrationKind` through
  the shared migration ledger. Those stay calls.
- **The per-workflow subdirectory role home now has six adopters**, three of them
  under `ui/window/`, so it is no longer a special case in either directory. A
  workflow-prefixed policy file is still not an option: it leaves the
  `ui/**/policy.rs` mutation glob.
- **A two-directory workflow is resolvable**, and `WFR-LOCAL-HISTORY` is the
  worked example: canonical role home on the coordination side, called surface on
  the presentation side with its ownership in its own module doc, and — the part
  that makes it honest — **the called surface imports its freshness tickets from
  the canonical `policy.rs`**.
- **The retroactive-amendment cost now stands at eight rows.** Slot 4's own
  re-check found **two of four** rows genuinely non-compliant, so assume the next
  amendment is real work rather than a confirmation.
- **Run the rustdoc gate before shipping a facade.** It is in neither
  `make check` nor `make pre-commit` nor `make check-policy`; CI's `Lint` job
  enforces it, and slot 3a shipped that failure once. Four new `pub` facades in one
  change is exactly its shape.

**Three hazards of this convention's mechanical work, all hit by slot 4:**

1. **Moving an `if` condition into a `match` scrutinee extends a borrow's
   lifetime** and can produce a latent `BorrowMutError`. A `match` scrutinee's
   temporaries live for the whole match; a plain `if` condition's drop before the
   block. Bind the value to a local first.
2. **A "tautological" extraction is a smell.** `terminal_is_complete(c)` was
   `c.is_none()`; it bought nothing and forced a dead default at the call site.
   Count the decisions a reader could get wrong, not the functions you can name.
3. **An assertion comparing a value against the constant it came from cannot
   detect the constant changing.** This was slot 4's single most common mutation
   survivor. Pin policy constants to concrete literals in units a reader would
   sanity-check.

**One more, specific to writing evidence surfaces:** the **disposal proof test
earns its place**. Local history's first surface actually panicked in it, because
`live_local_history_availability()` derefs a template child. Write the disposal
proof before believing the surface is safe.

### B.3 Residual cleanups deferred to slot 7, found by the tail simplify pass

The post-verification simplify pass applied every duplication it could prove
equivalent (recorded in A.16). Four candidates were **deliberately not applied**,
three of them routed here. Each is recorded with the evidence already gathered so
slot 7 does not re-derive it, and none is a defect: they are scope decisions taken
at the tail of an otherwise verified tree, where every `ui/**` edit voids live
accessibility, visual, and visual-geometry proof fingerprints.

| # | Candidate | Evidence gathered | Why deferred |
| --- | --- | --- | --- |
| 1 | **`LushtextWindow::flush_dirty_drafts` has no production caller.** ~100 production lines in `drafts/journal.rs` implementing a second, synchronous close flush | `grep -rn flush_dirty_drafts crates/` — production close goes through `flush_dirty_drafts_async` from `ui/window/dialogs.rs:706`; the synchronous entry point is reached only from `crates/lushtext/tests/widget/window.rs` (`test_flush_dirty_drafts_skips_close_discarded_editors`, `test_flush_dirty_drafts_fails_when_manifest_cannot_be_saved`) and `crates/lushtext/tests/widget/app.rs:560`. It is `pub` on the window | Retiring it removes a `pub` API **and** the close-discard and manifest-failure coverage those three tests carry, which would have to be ported to the async path first. That is a scoped decision, not a simplify edit. Both its own doc comment and `autosave_execution`'s module doc now state plainly that no production path reaches it, so the next reader is not misled while it stands |
| 2 | **`cancel_session_restore_runtime`'s `publish_projection` parameter is `false` at every call site**, making one arm unreachable | all three call sites pass `false` | The parameter is a documented symmetric seam in the facade's narration. Removing it is a narration change as much as a code change |
| 3 | **`current_window_width` exists twice**, same name and same body, in `ui/window/imp.rs` and `ui/window/local_history/preview_execution.rs` | identical helper bodies | Cross-directory dedup requires an ownership decision (which module owns the helper) for marginal gain, and would void the fingerprints a second time |

**The fourth was rejected outright, not deferred:** three candidates to split long
functions in `drafts/autosave_execution.rs` and `local_history/restore_execution.rs`
sit directly on safety-capture and body-write ordering. Length is not the problem
those functions have, and cutting them along a seam that is not already a boundary
is exactly how an ordering invariant gets lost. Do not revisit.
