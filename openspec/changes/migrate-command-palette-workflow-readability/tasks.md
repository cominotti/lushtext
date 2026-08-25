## 0. Prerequisites and orientation

- [x] 0.1 Confirm the prerequisite still holds. Slot 1 is archived at
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/`
      with its five deltas merged into `openspec/specs/`; verify in particular that
      the bounded coordination role names are present in
      `openspec/specs/gtk-adapter-module-boundaries/spec.md` and the facade-budget
      requirement in `openspec/specs/workflow-readability-boundaries/spec.md`, since
      both of this change's deltas are `MODIFIED` requirements against specs that
      must already exist there. This is cheap insurance, not expected work.
- [x] 0.2 Read, in this order: `docs/next/workflow-readability.md` (the programme
      record, especially "Slot 1 residue" and the slot ledger),
      `docs/workflow-readability-matrix.md` rows `WFR-COMMAND-PALETTE`,
      `WFR-SEARCH-REPLACE`, and `WFR-AUTOMATION-SPINE` plus the "Settled
      Conventions", "Facade size budget", "Migrated Workflow Roles", and
      "Completion Rule" sections, and the five capability specs the matrix header
      names. Then read the exemplar, `crates/lushtext-core/src/ui/search_panel/`
      — `mod.rs` is the facade shape to copy, including its module-doc stage
      orders, inversion notes, and role table.
- [x] 0.3 Re-verify this change's own premise against the tree before writing
      code: the palette inventory in the proposal was measured at authoring time.
      Confirm the current line counts, the `*_for_test` set, and the automation
      call sites, and record any drift in this file rather than silently working
      from stale numbers. One known discrepancy to resolve rather than rediscover:
      the matrix row's `ui 2,528` subtotal exceeds the 1,672 lines in
      `ui/command_palette/` itself, because the census attributed
      palette-serving code outside that directory to the row — expect the window's
      focus-indexing and palette-hosting code to account for the difference.
      Confirm which files those are, because they decide whether any of them is
      part of this migration's facade or stays `WFR-SHELL-LAYOUT` surface.

## 1. Split slot 2 and record the split

- [x] 1.1 In `docs/next/workflow-readability.md`, replace the single slot-2 row of
      the remaining-scope table with two rows: `2a` (this change —
      `WFR-COMMAND-PALETTE`, first `WFR-AUTOMATION-SPINE` projections, facade
      budget; tier-2) and `2b` (`complete-search-replace-workflow-readability` —
      `WFR-SEARCH-REPLACE` replace/undo half; tier-3). Do **not** renumber slots 3
      through 7: their numbers are cited from the matrix and from per-row `Slot`
      cells.
- [x] 1.2 In the same file's "Slot ledger (machine-readable)" subsection, replace
      the `- slot 2 (outstanding): ...` line with exactly two lines:
      `- slot 2a (outstanding): WFR-COMMAND-PALETTE, WFR-AUTOMATION-SPINE` and
      `- slot 2b (outstanding): WFR-SEARCH-REPLACE (partial), WFR-AUTOMATION-SPINE`.
      Run `make check-workflow-boundaries` immediately: it must still pass with
      the split before any code moves.
- [x] 1.3 Update the record's "Naming and finding a slot's change" paragraph and
      its slot table so both change names are registered:
      `migrate-command-palette-workflow-readability` (2a) and
      `complete-search-replace-workflow-readability` (2b). A cold session must be
      able to find both without searching.
- [x] 1.4 Update the record's ordering paragraph ("Slot 2 must resolve one
      ordering question before it starts") to state the decision that was taken —
      split into 2a and 2b — and why: the two-proof rule wants a *completed*
      migration, and completion is observable only at the change boundary, so as
      two changes the gate is enforced by `make check-workflow-boundaries` instead
      of by a task-list promise. Mirror the same resolution in the matrix's "Slot
      2 is the one place where that rule needs an explicit decision" paragraph and
      its "Migration Order And Risk Tiers" table.
- [x] 1.5 Correct the record's artifact expectation. Both the record (section 3,
      "Migration changes are expected to need only a proposal and tasks") and the
      matrix ("Artifacts each slot is expected to need") state an expectation that
      `openspec validate --strict` cannot satisfy: it fails any change with no
      `specs/` delta ("Change must have at least one delta"). Restate the
      expectation as "proposal and tasks, plus the minimum spec delta strict
      validation requires", and note that needing a delta which *adds obligations
      or capabilities* is still the signal of an incomplete Phase-0 contract. This
      is a documentation-accuracy fix, not a licence to widen scope.

## 2. Set the normative facade line budget

- [x] 2.1 Measure the exemplar facade again
      (`wc -l crates/lushtext-core/src/ui/search_panel/mod.rs`) and confirm it is
      still 350 physical lines. If it has drifted, use the measured value as the
      input and say so.
- [x] 2.2 Declare the budget in `docs/workflow-readability-matrix.md`'s "Facade
      size budget" section as exactly one line,
      `- normative facade line budget: 370`, replacing the "No budget is declared
      yet." sentence. Rewrite the section's future-tense prose ("slot 2 sets it")
      into the settled past tense, keeping the measurement and the reason 370 was
      chosen: 350 measured plus modest headroom, and the section's own finding
      that a budget below roughly 370 would force the exemplar's narration to
      split. Record the **20-line headroom as a stated risk**: it is deliberately
      tight because a loose budget enforces nothing, and the consequence is that a
      facade narrating two stage orders may not fit on the first attempt. The
      response is always to delegate more (task 5.8), never to raise the number.
      If an honest split still cannot fit 370, that is real evidence the number is
      wrong and must be corrected **in this change**, while only one other
      workflow is migrated.
- [x] 2.3 Apply both of this change's deltas via the normal sync/archive flow: the
      `workflow-readability-boundaries` delta, so the facade-budget requirement no
      longer reads as an instruction to a future migration to set the number; and
      the `gtk-adapter-module-boundaries` delta, which adds the stage-order
      qualification rule this change's own split exercises (task 5.1) plus the note
      that the bounded role set is reviewed, not gated. Mirror the qualification
      rule and the not-gated note into the matrix's "Role file names" table and
      `.agents/rules/rust.md`'s bounded role-name list, then run
      `make check-agent-docs`.
- [x] 2.4 Prove the previously inert check is now live and passing: run
      `make check-workflow-boundaries` and confirm it reports the budget and
      accepts the exemplar's 350-line facade. Then deliberately break it (a
      temporary local edit lowering the declared budget below 350, or padding the
      facade) and confirm the check fails naming the row, path, measured size, and
      budget. Revert the deliberate break. A budget that cannot be observed
      failing is not enforced.
- [x] 2.5 Retroactive-amendment check: confirm every row currently marked
      `migrated` in the matrix satisfies the new number, and record the per-row
      measured sizes in this file. Today that is `WFR-SEARCH-REPLACE` only; this
      change's own palette facade is checked in task 5.

## 3. Read the palette workflow before changing it

- [x] 3.1 Write down, in this file, the palette's current ordered stages with
      every inversion and resumption point, from the code rather than from the
      matrix. The matrix's "WFR-COMMAND-PALETTE" stage trace names five
      inversions; the authoring inventory found more, because the incremental
      index-mutation path has its own debounce, a disposal-capacity retry wakeup,
      a worker, and a tail reschedule. The facade narration in task 5 is written
      from this trace, not from the matrix.
- [x] 3.2 Classify every test seam in `ui/command_palette/**` as inspection,
      configuration, actuation, or lifecycle probe, and reconcile the count with
      the matrix row's `15/10/2/0 = 27 fns, 40 sites, 4 override statics` cell.
      Where the reconciliation fails, correct the matrix cell in this change and
      say which unit each figure uses (gated declarations versus gate attribute
      sites), per `workflow-evidence-surfaces`. Note explicitly which seams live
      in `services/palette/**` and are therefore shared with
      `WFR-NOTES-BOOKMARKS` (slot 5) rather than owned here.
- [x] 3.3 Record the boundary decision for `NoteSourceRefreshCoordinator`, getting
      the fact right: there are **two independent instances**, not one shared one —
      `command_palette_note_refreshes` on the window imp (`ui/window/imp.rs:464`,
      read through `ui/automation.rs:724`'s `has_work()`) serving the palette, and
      `source_refreshes` (`ui/window/notes/mod.rs:198`, read through
      `NotesBrowserRuntimeSnapshot`) serving the Notes browser. The deferral reason
      is therefore **not** "shared state"; it is that deduping the *type* changes
      the notes-browser snapshot's shape, which is `WFR-NOTES-BOOKMARKS` surface
      area. Record that in the matrix under `WFR-NOTES-BOOKMARKS` so slot 5
      inherits the correct reason and a named task.
- [x] 3.4 Record the boundary decision for the three process-global retirement
      counters (`FULL_REPLACEMENT_RETIREMENTS`,
      `ACCEPTED_INCREMENTAL_RETIREMENTS`, `REJECTED_INCREMENTAL_RETIREMENTS` and
      their `FileIndexRetirementSnapshot`). They are inspection-shaped but
      process-global rather than per-widget, so folding them into a per-widget
      evidence surface changes their meaning. Decide explicitly: either keep them
      as lifecycle probes with a stated reason, or make them per-widget state and
      fold them in. Record the decision **in the `WFR-COMMAND-PALETTE` row's
      `Seams (i/c/a/p)` cell and its `Migrated Workflow Roles` notes**, the same
      places task 3.3 writes to, so the reasoning survives with the row rather
      than only in this file.

## 4. Extract pure palette policy

- [x] 4.1 Create `crates/lushtext-core/src/ui/command_palette/policy.rs` with no
      `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5` import. Every function
      must take primitives or plain value objects, never `&self` on a GObject.
- [x] 4.2 Move the queued-index-update admission policy into it: the per-update
      retained-byte weight (`FileIndexUpdate::retained_byte_weight`), the
      capacity-growth byte estimate and cap arithmetic currently inline in
      `retain_bounded_index_update`, the `MAX_PENDING_INDEX_UPDATES` /
      `MAX_PENDING_INDEX_UPDATE_BYTES` ceilings, and the escalate-to-full-rebuild
      decision that fires on overflow. The adapter keeps the `Cell`/`RefCell`
      reads and writes; the decision moves.
- [x] 4.3 Move the batch-kind selection rule (rebuild-pending versus incremental)
      and the flush guard predicate.
- [x] 4.4 Move the generation arbitration and replay-on-loss rule: accept an
      applied batch when the live index generation still equals the batch's base
      generation, otherwise reject and request a rebuild. This predicate is
      written out three times today.
- [x] 4.5 Move the retirement-cap classification predicate
      (`last_owned && previous.len() == MAX_INDEXED_FILES` and the full /
      accepted-incremental / rejected-incremental tagging). Also written three
      times today.
- [x] 4.6 Move the result-navigation policy from `imp.rs`: first-activatable,
      next-activatable directional scan with its fallback-to-current rule, and the
      activatable predicate. Express it over an activatable-flag sequence rather
      than over `gio::ListStore`, so it is testable without GTK.
- [x] 4.7 Move the presentation-decision policy from `imp.rs`: the no-results
      visibility conjunction, the result-count pluralization, and the
      selected-versus-searching-versus-count precedence used for the accessible
      value text. These are pure string and boolean decisions wrapped in GTK
      reads today.
- [x] 4.8 Give every moved decision at least one co-located `#[cfg(test)]` unit
      test asserting the behavior that was previously only reachable through the
      widget, including each rejected clause of the generation arbitration and
      each cap boundary (at, one under, one over).
- [x] 4.9 Name the timing and limit constants the module now owns as typed
      constants beside their decision (`INDEX_UPDATE_DEBOUNCE_MS`, the search
      debounce interval, `MAX_RESULTS_PER_SOURCE`), per the literal-ownership rule
      in `.agents/rules/rust.md`. Do not create a shared constants dump.
- [x] 4.10 Capture the mutation baseline for this logic **before** it moves, so
      task 9 can report the outcome honestly. Follow the two-part scoping precedent
      in
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-baseline-search-policy.md`.
      Expect the baseline to be **zero generated mutants by construction**:
      `ui/command_palette/**` is not in `examine_globs` today. Record that as a
      baseline of zero with the reason, and note the asymmetry the evidence file
      must state — moving into a scoped `policy.rs` *gains* mutants that must all
      be killed, which is strictly stronger than the requirement's equal-counts
      phrasing (that phrasing governs moves between two already-scoped locations).
      Do not report this as "0 → 0, parity holds".

## 5. Assign the palette's roles

- [x] 5.1 Decide and record the coordination role split. The palette owns two
      stage orders — the query flight, and the incremental file-index mutation —
      so it owns more than one coordination module, which the convention permits.
      The recommended mapping, to be confirmed or replaced with a recorded reason:
      `execution.rs` for the query flight (request submit, worker spawn,
      completion arbitration, pending chaining), `admission.rs` for the
      index-update queue (bounded retention, debounce, disposal-capacity retry,
      flush, applied-batch arbitration), and `retirement.rs` for file-index
      retirement accounting. Every name must come from the bounded set in
      `openspec/specs/gtk-adapter-module-boundaries/spec.md`. Where two of the
      palette's stage orders need a module of the same shape, use the stage-order
      qualification rule this change's own delta adds (task 2.3) rather than
      forcing one of them onto a less accurate bounded name.
- [x] 5.2 If a palette coordination job fits none of `admission`, `execution`,
      `retirement`, or `watch` **even with the qualification rule**, stop and
      amend `gtk-adapter-module-boundaries` again to add the role name, with a
      scenario, in this change. Do not overload an existing name and do not invent
      an unlisted one. Per the retroactive-amendment rule, adding a role name
      requires re-checking every already-migrated row against the amended set in
      the same change. Note that no gate enforces the set: the workflow boundary
      check validates only that declared role paths exist, so this decision is
      review-carried.
- [x] 5.3 Retire `ui/command_palette/runtime.rs`. `runtime` is not a role name:
      it says only that the module is machinery. Its three contents go to three
      different owners — the `CommandPaletteSearchRequest` value object to the
      coordination module that submits it, `execute_search` to that same module as
      its worker entry point, and the `SEARCH_DELAY_MS` static to
      `test_policy.rs`.
- [x] 5.4 Reify the file-index mutation seam as a value object. This is the
      palette's one genuinely unreified bundle: the applied batch's base
      generation is compared clause-by-clause against live state at the worker
      completion, and the retirement-cap triple is duplicated. Use the shape the
      codebase already uses — a ticket captured at dispatch plus either live facts
      or an `is_current(&owner)` predicate — matching `SaveCompletionTicket` or
      `ReplacePreviewTicket` + `ReplacePreviewFacts`. Construct it once at the
      point the batch is admitted and validate it as a unit. Do not introduce a
      differently shaped freshness convention.
- [x] 5.5 Confirm and record that the **query** seam needs no new value object:
      its coordinator already owns the generation and exposes `is_current`, which
      `workflow-readability-boundaries` accepts as the seam value object. Update
      the matrix row's "Seam value object" cell to name both: the existing
      coordinator identity for the query side, and the new ticket for the mutation
      side.
- [x] 5.6 Rename cross-module palette operations from mechanism names to
      workflow-intent names, leaving mechanism names on private helpers inside the
      module that owns the mechanism. Every `pub`, `pub(crate)`, `pub(super)`, and
      cross-module operation is in scope.
- [x] 5.7 Write the narrative facade in `ui/command_palette/mod.rs`: both stage
      orders in order with their intent named, every inversion documented with the
      point where control resumes, and a role table like the exemplar's. The
      facade must own no timer, no ledger, no generation counter, no admission
      bookkeeping, and no widget mutation beyond the trivial entry-point surface
      reads and writes the exemplar's module doc carves out explicitly.
- [x] 5.8 Measure the new facade and confirm it is within the 370-line budget
      declared in task 2. If it is not, the split in task 5.1 is wrong — move work
      into the coordination modules rather than raising the budget, which would be
      a convention amendment requiring re-migration of every migrated row.

## 6. Consolidate palette evidence

- [x] 6.1 Create `crates/lushtext-core/src/ui/command_palette/evidence.rs` with
      one typed surface reachable through an `evidence()` accessor, at the
      narrowest visibility its readers require. `ui/automation.rs` reads it
      in-crate and the external widget harness needs to name the type, so follow
      the exemplar's pattern: an internal type with a `#[cfg(feature =
      "test-utils")]` re-export rather than an unconditional widening of the
      crate's public API.
- [x] 6.2 Expose every field the retired inspection functions exposed:
      `index_update_worker_running_for_test`, the five components of
      `index_update_queue_snapshot_for_test` as **named typed fields** rather than
      a tuple (queue length, queued bytes, rebuild-pending flag, and the two caps),
      `search_runtime_snapshot_for_test`, `observed_search_cancellations_for_test`,
      and `last_cancelled_search_examined_for_test`.
- [x] 6.3 Fold in the existing typed observation rather than leaving a second path:
      the coordinator snapshot the palette reads for its query flight becomes a
      field of the evidence surface, and the widget no longer exposes a separate
      snapshot accessor.
- [x] 6.4 Make the surface read-only in the strong sense: reading it must not
      touch timers, queues, generation counters, or coordinator state, and must
      not require the workflow to be in a particular stage. Respect the reentrancy
      constraint the exemplar's `evidence.rs` documents — the accessor takes shared
      borrows, so no field may be read from inside a `borrow_mut()`.
- [x] 6.5 Stop compiling test-serving counters into production. The
      `observed_search_cancellations` and `last_cancelled_search_examined` fields
      are ungated `Cell`s today, maintained only so tests can read them. Gate them
      with the workflow's test feature, or justify in this file why the workflow
      needs them in production.
- [x] 6.6 Migrate `crates/lushtext/tests/widget/command_palette.rs` (2,499 lines)
      and the palette call sites in `crates/lushtext/tests/widget/window.rs` to
      read the evidence surface. Do not add a replacement per-field accessor for
      anything: a test needing a fact the surface lacks extends the surface.
- [x] 6.7 Delete the retired per-field inspection functions and confirm no caller
      remains anywhere in the workspace, including benches and integration tests.
      The project test count must not decrease; record the before and after
      counts.
- [x] 6.8 Create `ui/command_palette/test_policy.rs` holding one per-workflow
      test policy value that collapses the palette's timing and limit overrides
      (the search delay and the index-update delay at minimum). Put the whole
      module behind `#[cfg(feature = "test-utils")]` so a production build
      compiles no override storage, and confirm that with a default-feature
      build. Override declarations must not occupy the opening section of a
      workflow module ahead of its logic.

## 7. Project automation from evidence

- [x] 7.1 Rewrite `command_palette_snapshot` in
      `crates/lushtext-core/src/ui/automation.rs` to read the palette evidence
      surface once instead of calling seven separate widget accessors. Keep
      `command_palette.visible` as window shell state, exactly as
      `content_search.visible` is kept — this is the second instance of the
      pattern `docs/automation-reference.md` already documents.
- [x] 7.2 Keep the exported contract byte-identical: the same eight field names,
      the same types, the same bounded-text and bounded-length treatment, and the
      same meanings. Evidence fields that are not part of the documented contract
      (queue byte counters, caps, cancellation counters, coordinator high-water
      marks) must not be serialized.
- [x] 7.3 Project the palette half of the two readiness blockers from evidence
      while preserving their documented semantics exactly:
      `command-palette-search` (one current active or latest palette query still
      owns background search work) and `command-palette-index` (a rebuild or
      mutation, **or** a bounded note-source refresh, is active or retained). The
      note-source disjunct stays as it is; it belongs to slot 5.
- [x] 7.4 Update `docs/automation.md` and `docs/automation-reference.md` for the
      projection, extending the existing "Where a workflow exposes an internal
      typed evidence surface" paragraph to name `window.command_palette` as the
      second projection rather than writing a parallel paragraph. Include the
      evidence-field-to-snapshot-field mapping the new check in task 7.6 reads, so
      the gate run at the end of this section is meaningful rather than a
      no-op pass.
- [x] 7.5 Prove the contract is unchanged rather than asserting it: capture an
      Automation1 snapshot for the same app state before and after, and diff the
      `command_palette` object and the readiness fields to zero differences.
- [x] 7.6 **Implement the evidence-to-snapshot drift check that Phase 0 specified
      and never built.** `openspec/specs/workflow-evidence-surfaces/spec.md`'s
      "Projection drift is detected" scenario requires
      `make check-automation-docs` to fail, naming both the evidence field and the
      snapshot field, when a projected evidence field is added, removed, or
      renamed. `scripts/check-automation-docs.py` has no evidence-surface
      awareness today and nothing in `scripts/` references `SearchPanelEvidence`.
      Slot 1 could leave that unnoticed with a single projection; this change makes
      projections plural, so the unimplemented requirement becomes load-bearing and
      enters scope now per `.agents/rules/preexisting-blockers.md`. Add the check to
      `scripts/check-automation-docs.py`, following that script's existing
      structure and conventions.
- [x] 7.7 Scope the check to both projections, not just the new one: the existing
      `SearchPanelEvidence` → `window.content_search` mapping and this change's
      palette evidence → `window.command_palette` mapping. Fixing the gap while
      covering only the new projection would leave the slot-1 projection
      unprotected and the requirement still unmet.
- [x] 7.8 Add `--self-test` cases for the new check, matching the convention the
      script already uses: an in-agreement tree passes; an evidence field renamed
      without a doc update fails and the failure names both the evidence field and
      the snapshot field; an evidence field removed while still documented as
      projected fails; a newly projected field absent from the docs fails; and a
      non-projected internal evidence field (a counter, a cap, a high-water mark)
      is correctly ignored rather than demanding documentation.
- [x] 7.9 Prove the check is live and can fail, the same way task 2.4 proves the
      facade budget: run `make check-automation-docs` clean, then make a temporary
      local edit renaming one projected evidence field and confirm the check fails
      naming both fields, then revert. A gate that has never been observed failing
      is not a gate.

## 8. Reduce the palette's duplicated single-flight machinery

- [x] 8.1 Express `FileIndexBuildCoordinator` in
      `crates/lushtext-core/src/services/palette/index.rs` through the shared
      `SingleFlightCoordinator`, the way `services/palette/runtime.rs` already
      aliases `PaletteSearchCoordinator`. It is a semantically equivalent
      hand-rolled duplicate, differing only in its snapshot (4 fields against the
      shared type's 6, which add high-water marks) and in two shared methods it
      does not expose (`clear_pending()`, `active_generation()`) plus the shared
      type's generic request parameter. Paralleling a shape the codebase already
      has is what the seam requirement's "reuse the established shape" rule
      forbids.
- [x] 8.2 Confirm the blast radius before changing it: the snapshot type gains
      the shared type's two high-water fields, and its only readers are
      `services/palette/tests.rs` and `benches/benchmarks.rs`. If any other
      reader exists, or if the shared coordinator's semantics differ in any
      observable way (start, finish, supersede, cancellation-request counting),
      stop and record the difference instead of forcing the alias.
- [x] 8.3 Leave `NoteSourceRefreshCoordinator` alone, for the reason task 3.3
      establishes: not because the state is shared (there are two independent
      instances) but because deduping the type changes the notes-browser
      snapshot's shape, which is `WFR-NOTES-BOOKMARKS` surface area. Confirm the
      matrix records that under `WFR-NOTES-BOOKMARKS` as a named slot-5 task with
      the correct reason.

## 9. Mutation coverage

- [x] 9.1 Confirm the new `policy.rs` is examined through the `ui/**/policy.rs`
      convention glob with no hand-listed path added. **Expected `exclude_re`
      retirements: none** — the only palette-adjacent entries name
      `services/palette/index.rs`'s `truncate_to_index_limit` and the
      `commands.rs` property-test bridge, and this change moves neither. If an
      entry does become retirable, retire it and say which.
- [x] 9.2 Run the focused scoped mutation run against the new
      `ui/command_palette/policy.rs`. Use the two-part scoping the slot-1 evidence
      file documents — `MUTANTS_RE` alone does not filter the `delete field`
      mutant kind, so a `MUTANTS_EXCLUDE` glob is required to reduce the listed
      scope to the target module. Record the exact commands. Every newly generated
      mutant must be caught, or unviable with a stated reason.
- [x] 9.3 Write the evidence to
      `openspec/changes/migrate-command-palette-workflow-readability/evidence/mutation-parity-palette-policy.md`,
      following the structure of
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`:
      scope re-verification with exact commands, the before/after table of
      generated, caught, missed, unviable, and timeout counts, and per-survivor
      disposition. State the asymmetry from task 4.10 plainly — the baseline is
      zero by construction because the source files were unscoped, so this is a
      coverage **gain** that must be fully killed, not an equal-counts parity
      claim.
- [x] 9.4 Run `make mutants-diff` for the change and close survivors with added
      tests rather than scope changes. If the wrapper's `git diff origin/main...`
      cannot see working-tree edits, use the explicit merge-base diff workaround
      recorded in the slot-1 evidence file and record both runs.
- [x] 9.5 Name the matrix row's `mutation parity` role with the path from task
      9.3. If this change relocates no policy, the role may be the literal `none`
      — but given task 4 it should not be.

## 10. Matrix and record completion

- [x] 10.1 Add a `### WFR-COMMAND-PALETTE` subsection to the matrix's "Migrated
      Workflow Roles" section naming facade, coordination, policy, evidence, and
      mutation parity with real paths, in the documented format.
- [x] 10.2 Set the `WFR-COMMAND-PALETTE` row status to `migrated`, update its
      "Owned pure policy", "Seams (i/c/a/p)", "Seam value object", and "Evidence
      surface" cells to describe what the migration did rather than the
      pre-migration census, and update its `Slot` cell to `2a`.
- [x] 10.3 Keep `WFR-AUTOMATION-SPINE` accurate: it must **not** be marked
      `migrated`, because it continues in later slots. Record the palette
      projection in its "Evidence surface" cell, change its `Slot` cell from
      `2 onward, incrementally per migrated workflow` to `2a onward, ...` so the
      cell does not name a slot that no longer exists, list it as
      `WFR-AUTOMATION-SPINE (partial)` on the now-`complete` slot 2a ledger line,
      and keep it on slot 2b's outstanding line. Marking it migrated to satisfy
      the gate would be a false claim; omitting it from every outstanding line
      would fail the gate.
- [x] 10.4 Flip the slot 2a ledger line to `complete` and update the record's
      status line, its baseline section (share of `ui/` + `model/` migrated,
      policy modules relocated, test seams addressed, seams reified — reporting
      seams reified as the primary unit and stating which long-signature
      definition any secondary figure uses), and its remaining-scope table.
- [x] 10.5 Update the "Slot 1 residue" section of
      `docs/next/workflow-readability.md`: strike the facade-budget item as
      discharged by this change, and confirm the remaining four items are 2b's.
      Do not delete the section. Make the **same** edit to the matrix's parallel
      paragraph, "Slot 1 residue that slot 2 inherits" (in
      "Migration Order And Risk Tiers"), which lists the identical six obligations
      including "the normative facade line budget number" — the two documents must
      not disagree about what is still owed.
- [x] 10.6 Update `AGENTS.md`, `README.md`, and any `.agents/rules/*.md` or
      `.agents/skills/**` reference that names a path this change moved or a fact
      it changed, per `.agents/rules/documentation.md`. Then run
      `make check-agent-docs` and `make check-agent-skills`.

## 11. Verification

- [x] 11.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries` with the now-active facade-budget rule and the
      record/matrix ledger agreement, and `check-automation-docs`.
- [x] 11.2 `make test` and `make test-widget-headless` clean with **zero
      `FLAKY:` lines**. A recovered flake is a blocker per
      `.agents/rules/preexisting-blockers.md`: read the real failure, classify the
      wait, fix the cause, and rerun in isolation. Record the test counts before
      and after and confirm the project total did not decrease.
- [x] 11.3 `make mutants-diff` clean with the task 9.3 parity evidence attached.
- [x] 11.4 `make check-automation-docs` and `make automation-client-self-test`
      clean.
- [x] 11.5 The proof lanes that `crates/lushtext-core/src/ui/` and
      `crates/lushtext/tests/widget/` changes make mandatory:
      `make visual-geometry-smoke` from a clean artifact root (required by
      `check-visual-proof-policy`), plus `make accessibility-smoke` and
      `make visual-smoke` (required by `check-accessibility-policy`'s source
      fingerprint). Run `make check-accessibility-policy` and
      `make check-visual-proof-policy` afterwards and confirm both pass against
      the current tree.
- [x] 11.6 Behavior equivalence for the palette across its real state extremes,
      each proven by a widget test and, where an anchor already exists, an AT-SPI
      smoke case: no results; one result; representative mixed results across
      Files, Notes, Commands, and All; dense results requiring scrolling with
      long paths; a mode switch mid-query; a superseded query whose stale
      completion must publish nothing; and a constrained-width palette. Results,
      ordering, grouping, headers, auto-selection, focus, and accessible metadata
      must be identical to the pre-migration behavior.
- [x] 11.7 Behavior equivalence for the incremental index-mutation path, which is
      the half with no user-visible surface and therefore the easiest to break
      silently: a create, a delete, and a rename each reaching the index; a batch
      that exceeds the count cap and one that exceeds the byte cap, both escalating
      to a full rebuild; a mutation whose applied batch loses to a concurrent full
      replacement, which must be rejected and replayed; and a disposal-capacity
      refusal that retries through the wakeup rather than dropping the update.
- [x] 11.8 (evidence: [Appendix A.16](#a16-live-run-task-118--what-was-run-and-the-one-gap)) Live run: `make run`, then exercise palette open and close, all four
      modes, a dense query, a no-results query, focus restoration on close, and a
      file rename in the sidebar that drives an incremental index mutation.
      Confirm stderr has no new `Gtk-WARNING`, `Gtk-CRITICAL`,
      `GLib-GObject-WARNING`, pixman `*** BUG ***`, or `Trying to measure`
      output. Per the slot-1 precedent, if the maintainer's environment makes the
      literal target unsafe to run, record exactly what was run instead and what
      remains uncovered rather than silently substituting a headless run.
- [x] 11.9 Cold-read verification: with this change's conversation set aside, read
      only the new facade and confirm both palette stage orders and every
      inversion are followable without opening the coordination or policy
      modules. If they are not, the role split is wrong and task 5 must be
      revisited before this change is archived.
- [x] 11.10 `openspec validate migrate-command-palette-workflow-readability
      --strict` clean.

## 12. Handoff to slot 2b

- [x] 12.1 Confirm the two-proof precondition 2b depends on is now genuinely met
      and observable: the matrix marks `WFR-COMMAND-PALETTE` migrated with
      complete roles, the ledger marks slot 2a complete, and
      `make check-workflow-boundaries` passes. Record this in the record's slot
      table as the gate 2b checks.
- [x] 12.2 Record any convention friction this migration hit that 2b or slots 3
      through 7 would otherwise rediscover — in particular whether the bounded
      coordination role set was sufficient, whether the 370-line budget held on a
      second facade, and whether the evidence-surface visibility pattern needed
      any deviation. Convention corrections are cheapest now; after 2b lands,
      three workflows are migrated and the retroactive-amendment rule gets more
      expensive.

## Appendix A. Recorded findings

### A.0 Premise re-verification (task 0.3)

Measured 2026-08-25 against the working tree. **No drift** in any headline number
the proposal cited:

| Fact | Proposal | Measured | Verdict |
| --- | --- | --- | --- |
| `ui/command_palette/mod.rs` | 667 | 667 | exact |
| `ui/command_palette/runtime.rs` | 59 | 59 | exact |
| `ui/command_palette/imp.rs` | 761 | 761 | exact |
| `ui/search_panel/mod.rs` (exemplar facade) | 350 | 350 | exact |
| `crates/lushtext/tests/widget/command_palette.rs` | 2,499 | 2,499 | exact |
| `services/palette/notes.rs` | 3,428 | 3,428 | exact |

**The matrix `ui 2,528` subtotal is resolved exactly.** `ui/command_palette/`
holds 1,672 lines (`imp.rs` 761 + `mod.rs` 667 + `item.rs` 185 + `runtime.rs` 59).
`2,528 − 1,672 = 856`, which is exactly `crates/lushtext-core/src/ui/window/focus_indexing.rs`
(856 lines). That is the only palette-serving file outside the directory the census
attributed to the row, and it is **not** part of this migration's facade: it owns
the window's editor-residency focus-indexing workflow and reaches the palette only
to hand it rebuilt sources. It stays `WFR-SHELL-LAYOUT`/`WFR-EDITOR-MEMORY`-adjacent
window code. No other `ui/window/**` file contributes to the subtotal.

**`search_runtime_snapshot_for_test` readers.** The proposal noted three readers of
`PaletteSearchCoordinatorSnapshot`. Confirmed, but only one reads it *through the
palette widget accessor*: `crates/lushtext/tests/widget/command_palette.rs`. The
other two (`ui/window/notes/mod.rs`, `services/bookmark_excerpt.rs`) read the
shared alias for their own coordinators and are untouched here.

### A.1 Palette stage orders, from the code (task 3.1)

**Stage order 1 — query search.** Entry points: `Ctrl+Shift+P` → `open()`;
`set_search_mode`; `set_query`; source replacement (`set_guarded_file_index`,
`set_open_tabs`, `set_guarded_note_entries`, `set_workspace_group_label`,
`set_sources`); `Tab`/`ISO_Left_Tab` mode cycling; search-entry edits.

1. Capture the query and mode plus the four shared source snapshots into one
   compact request (`CommandPaletteSearchRequest`).
2. Debounce typed input 150 ms; empty queries bypass the debounce so cleared
   queries repaint immediately. Every direct entry point advances the debounce so
   a stale timer cannot re-fire.
3. Admit one flight through `PaletteSearchCoordinator::submit`, which either
   starts it or retains it as the one replaceable latest request.
4. Run grouped fuzzy search on a worker, capped per source.
5. Publish rows with one `splice`, then auto-select the first activatable row and
   refresh the accessible projection.

*Inversions (3):* the 150 ms debounce (`Debounce::schedule` → resumes in the
debounce callback); the `spawn_blocking_then` worker (resumes in the completion
closure, which re-validates `is_current(generation)`); and the pending-chain
re-entry, where a completion that finds a retained latest request starts it from
inside the completion rather than returning to the facade.

**Stage order 2 — incremental file-index mutation.** Entry points:
`update_index_file_created` / `_deleted` / `_renamed`, driven by sidebar file
operations and watcher reconciliation. It has **no visible surface**, which is why
task 11.7 exists.

1. Retain the mutation in a bounded queue under a count cap and an exact retained
   byte cap that includes `Vec` capacity-growth bytes; overflow escalates to a
   full filesystem rebuild instead of dropping the update.
2. Debounce the flush 75 ms so bursts coalesce.
3. Reserve disposal capacity for the replacement index; a refusal arms the
   disposal-capacity wakeup and returns.
4. Select the batch kind (rebuild-pending vs incremental) and take the queue.
5. Clone-and-mutate (or rebuild) on a worker under the mutation ledger.
6. Arbitrate the applied batch against the live index generation: accept and
   bump the generation, or reject, request a rebuild, and replay.
7. Reschedule a tail flush when the queue refilled meanwhile.

*Inversions (5):* the 75 ms flush debounce; the disposal-capacity wakeup
(`DisposalCapacityWakeup::arm` → resumes in the epoch-change callback, which
re-enters the flush); the `spawn_blocking_then` mutation worker (resumes in the
completion closure holding the base generation); the replay-on-loss reschedule
(the rejecting completion re-arms the flush debounce rather than calling the
worker); and the tail reschedule for a queue that refilled during the worker.

**Total: 8 inversions across two stage orders.** The matrix's
`WFR-COMMAND-PALETTE` trace said "five inversions, all coordinator-guarded",
which under-counted: three of the eight are timer/wakeup inversions that no
coordinator guards. The matrix trace is corrected in this change.

### A.2 Test seam classification (task 3.2)

`ui/command_palette/**` holds **12** `fn *_for_test` definitions and 22
`#[cfg(feature = "test-utils")]` attribute sites (19 in `mod.rs`, 3 in
`runtime.rs`). Classified:

| Seam | Kind |
| --- | --- |
| `index_update_worker_running_for_test` | inspection → retired into `evidence.rs` |
| `index_update_queue_snapshot_for_test` | inspection → retired into `evidence.rs` |
| `search_runtime_snapshot_for_test` | inspection → retired into `evidence.rs` |
| `observed_search_cancellations_for_test` | inspection → retired into `evidence.rs` |
| `last_cancelled_search_examined_for_test` | inspection → retired into `evidence.rs` |
| `file_index_retirement_snapshot_for_test` | probe (process-global; see A.3) — retained |
| `set_search_delay_for_test` | configuration → collapsed into `CommandPaletteTestPolicy` |
| `set_index_update_delay_for_test` | configuration → collapsed into `CommandPaletteTestPolicy` |
| `delay_search_for_test` (private) | configuration read → `test_policy::delay_search_worker` |
| `delay_index_update_for_test` (private) | configuration read → `test_policy::delay_index_update_worker` |
| `refresh_accessibility_state_for_test` | actuation — retained (deferred category) |
| `apply_palette_row_accessibility_for_test` | actuation — retained (deferred category) |

**Reconciliation with the matrix cell `15/10/2/0 = 27 fns, 40 sites, 4 override
statics`.** The cell is a **row-scoped** census figure and counts the whole
`WFR-COMMAND-PALETTE` footprint, not just `ui/command_palette/**`. The remaining
15 of the 27 functions live in `services/palette/**` (chiefly `notes.rs`, plus
`index.rs` and `commands.rs`) and in `ui/window/notes/**`, and those are **shared
with `WFR-NOTES-BOOKMARKS` (slot 5)**, not owned here. The `4 override statics`
figure counts the two palette-owned delay statics (`SEARCH_DELAY_MS`,
`INDEX_UPDATE_DELAY_MS`) plus two in `services/palette/**`; the three retirement
counters are probe accumulators, not overrides, and were correctly excluded. The
`40 sites` figure is gate-attribute sites and is the denominator the matrix header
already defines; `ui/command_palette/**` contributes 22 of them. The cell is
updated in this change to record what the migration did rather than the census
tuple, per the Product Matrix convention for a `migrated` row.

### A.3 `NoteSourceRefreshCoordinator` boundary decision (task 3.3)

Confirmed at the cited lines: there are **two independent instances**, not one
shared one — `command_palette_note_refreshes` on the window imp
(`ui/window/imp.rs`, read through `ui/automation.rs`'s `has_work()` in the
`command-palette-index` blocker) serving the palette, and `source_refreshes`
(`ui/window/notes/mod.rs`, read through `NotesBrowserRuntimeSnapshot`) serving the
Notes browser. **The deferral reason is therefore not "shared state".** It is that
deduping the *type* changes the notes-browser snapshot's shape, which is
`WFR-NOTES-BOOKMARKS` surface area and belongs to slot 5. Recorded in the matrix
under `WFR-NOTES-BOOKMARKS` as a named slot-5 task.

### A.4 Process-global retirement counters (task 3.4)

Decision: **keep them as lifecycle probes**, with the reason stated. They are
`AtomicUsize` process accumulators that count *last-owned at-cap* index
retirements across every palette instance in the process, so a widget test can
prove the bounded worker lane was reached at all. Folding them into a per-widget
evidence surface would change their meaning from "this process observed a
last-owned at-cap retirement" to "this widget currently holds N", which no test
asks and which a monotonic counter cannot express per widget. They keep their
`#[cfg(feature = "test-utils")]` gate — they already compile out of production —
and they move from `mod.rs` into `retirement.rs`, beside the classification
policy they instrument. `file_index_retirement_snapshot_for_test` therefore stays
as a probe rather than being retired.

### A.5 Coordination role split (task 5.1), with the reason for replacing the recommendation

The recommended mapping was `execution.rs` (query flight) + `admission.rs`
(index-update queue, *including* flush, worker, and applied-batch arbitration) +
`retirement.rs`. **Replaced**, for one reason: the index stage order has two
distinct coordination jobs, not one. Bounded queue retention with its byte caps
and its disposal-capacity retry is an *admission* job; batch construction, the
`spawn_blocking_then` worker, generation arbitration, replay-on-loss, and the tail
reschedule are an *execution* job. Putting the worker inside `admission.rs` would
overload that role name exactly the way the bounded-set requirement forbids.

The palette therefore owns two coordination modules of the **same shape** in one
directory, which is precisely the collision this change's
`gtk-adapter-module-boundaries` delta closes. Both take the stage-order
qualification:

| Module | Stage order | Bounded role |
| --- | --- | --- |
| `query_execution.rs` | query search | `execution` |
| `index_admission.rs` | file-index mutation | `admission` |
| `index_execution.rs` | file-index mutation | `execution` |
| `retirement.rs` | file-index mutation (only) | `retirement` — unqualified, no collision |

`retirement.rs` stays unqualified because only one of the two stage orders
retires anything. No coordination job needed a role name outside the bounded set,
so task 5.2's escape hatch was not used.

### A.6 Facade budget: measurement, proof, and retroactive check (tasks 2.1, 2.4, 2.5)

**Task 2.1 — exemplar re-measured.** `wc -l crates/lushtext-core/src/ui/search_panel/mod.rs`
= **350** physical lines. No drift from the recorded measurement, so 350 is the
input and 370 is the declared budget (350 + 20 headroom, bounded below by the
matrix's own finding that a budget under ~370 would force that facade's narration
to split).

**Task 2.4 — the previously inert check is live and was observed failing.**

```
$ ./scripts/check-workflow-boundaries.py            # budget 370 declared
workflow boundary policy passed: 1 workflow policy module(s) are pure and
mutation-scoped, every migrated matrix row names complete, existing roles, and
the programme record's slot ledger agrees with the matrix
$ exit 0

# deliberate break: declared budget lowered to 349
$ ./scripts/check-workflow-boundaries.py
workflow boundary policy violations:
  - docs/workflow-readability-matrix.md:933 row WFR-SEARCH-REPLACE declares
    facade `crates/lushtext-core/src/ui/search_panel/mod.rs`, which is 350 lines
    and exceeds the normative facade line budget of 349
$ exit 1

# reverted to 370
$ ./scripts/check-workflow-boundaries.py; exit 0
```

The failure names the row, the facade path, the measured size, and the budget, as
`workflow-readability-boundaries`' "Declared budget is mechanically enforced"
scenario requires. Note the matrix's `How to declare it` code fence contains the
literal template `- normative facade line budget: <integer>` *above* the real
declaration; the parser requires `\d+`, so the template is skipped and the real
line is read. That was verified by the pair of runs above rather than assumed.

**Task 2.5 — retroactive-amendment check.** Rows marked `migrated` at the moment
the budget was declared, with measured facade sizes:

| Row | Declared facade | Physical lines | Budget 370 |
| --- | --- | --- | --- |
| `WFR-SEARCH-REPLACE` | `crates/lushtext-core/src/ui/search_panel/mod.rs` | 350 | pass (20 lines spare) |

That is the only such row. This change's own `WFR-COMMAND-PALETTE` facade is
measured under task 5.8.

### A.7 Spec-delta application (task 2.3) — one deferral, flagged

Task 2.3 asks for both deltas to be applied "via the normal sync/archive flow".
`openspec` exposes no standalone sync command — `openspec archive` is what
"updates main specs" — and archiving this change is a separate step performed
after review, not part of implementation. **The `openspec/specs/` merge is
therefore left to that archive step**, which is the normal flow. What task 2.3
asks for beyond the merge is done here:

- both delta files exist and validate (`openspec validate ... --strict`, task 11.10);
- the stage-order qualification rule and the "reviewed, not gated" note are
  mirrored into the matrix's "Role file names" section and into
  `.agents/rules/rust.md`'s bounded role-name list;
- `make check-agent-docs` passes.

Consequence to be aware of until archive runs:
`openspec/specs/workflow-readability-boundaries/spec.md` still states the
facade-budget requirement in the future tense while the matrix now declares the
number. That inconsistency is exactly what the delta removes, and it closes when
the change is archived.

### A.8 Evidence-to-snapshot drift gate (tasks 7.6–7.9)

**Design.** The authority for "is this evidence field projected" is the Rust
snapshot function, not the doc: `evidence_projection_findings()` in
`scripts/check-automation-docs.py` parses the evidence struct's public fields,
parses the `evidence.<field>` reads inside the named projection function, and
compares both against an `Evidence Projection Map` table in
`docs/automation-reference.md`. A field the projection never reads is internal by
definition and must **not** be documented, which is how the "ignore internal
counters" requirement is satisfied without an allowlist. Both projections are
registered in `EVIDENCE_PROJECTIONS`: the slot-1 `SearchPanelEvidence` →
`window.content_search` mapping (22 rows) and this change's
`CommandPaletteEvidence` → `window.command_palette` mapping (7 rows).

The map is a reader-visible markdown table rather than an HTML-comment marker, so
the same artifact serves the doc and the gate.

**Self-test cases added** (`run_evidence_projection_self_tests`), each asserting
both that the check fails and that the failure names the field a reader needs:

1. baseline in-agreement tree produces no findings (guards the four cases below
   from being vacuous);
2. an evidence field renamed without a doc update — fails naming both
   `open_tab_source_count` and `command_palette.open_tab_source_count`;
3. an evidence field removed while still documented as projected — fails naming
   `result_count` and `command_palette.result_count`;
4. a newly projected field absent from the docs — fails naming
   `index_rebuild_pending` and "documents no snapshot field";
5. a documented row whose evidence field is no longer projected — fails naming
   `queued_index_updates` and "does not project it";
6. internal, non-projected fields are ignored: the nine internal palette fields
   (queued counters, both declared caps, the coordinator snapshot, both
   reservation weights) are asserted to exist *and* to be absent from the map,
   so a future blind check cannot pass this case by accident.

**Task 7.9 — observed failing on the real tree.** Renaming
`CommandPaletteEvidence::open_tab_source_count` to `open_tab_count` and updating
only the two Rust call sites:

```
$ ./scripts/check-automation-docs.py
evidence projection map: missing 2 item(s)
  - window.command_palette: Evidence Projection Map documents evidence field
    `CommandPaletteEvidence.open_tab_source_count` -> snapshot field
    `command_palette.open_tab_source_count`, but that evidence field no longer exists
  - window.command_palette: evidence field `CommandPaletteEvidence.open_tab_count`
    is projected but the Evidence Projection Map in docs/automation-reference.md
    documents no snapshot field for it
$ exit 1
```

Reverted; `--self-test` passes at exit 0.

### A.9 Test counts (tasks 6.7, 11.2)

| Lane | Before | After |
| --- | --- | --- |
| `cargo nextest run --workspace --all-features` | 1538 passed, 11 skipped | **1565** passed, 11 skipped |
| `#[test]` declarations in `crates/lushtext/tests/widget/**` | 1106 | **1109** |
| `#[test]` declarations in `ui/command_palette/policy.rs` | 0 (file did not exist) | **27** |

The non-widget total rose by exactly **27**, which is exactly the 27 co-located
`policy.rs` unit tests task 4.8 requires — the delta and the test count agree, so
nothing else changed count. The widget total rose by 3, the three
incremental-index tests A.14 records as added. No widget test was removed: the
migration rewrote call sites in place. The project total did not decrease.

Scope matters for these numbers and the command is part of the measurement:
`--workspace --all-features` reports 1565, `--workspace` with default features
reports 1518, and `-p lushtext-core --all-features` reports 1342. The before/after
pair above is the same command on both trees.

**Retired inspection functions (5), no callers remain anywhere in the workspace
including benches and integration tests:** `index_update_worker_running_for_test`,
`index_update_queue_snapshot_for_test`, `search_runtime_snapshot_for_test`,
`observed_search_cancellations_for_test`, `last_cancelled_search_examined_for_test`.
Also retired: the two configuration setters `set_search_delay_for_test` and
`set_index_update_delay_for_test`, collapsed into `CommandPaletteTestPolicy`.
Retained by classification: `file_index_retirement_snapshot_for_test` (lifecycle
probe, see A.4), `refresh_accessibility_state_for_test` and
`apply_palette_row_accessibility_for_test` (actuation), and the renamed
`restart_query_for_test` (actuation; it replaces the tests' direct
`palette.imp().rebuild_results(...)` reach-through, which was neither gated nor
named as a seam before).

### A.10 New module inventory (tasks 5.1, 5.3, 5.7, 5.8)

| File | Lines | Role |
| --- | --- | --- |
| `ui/command_palette/mod.rs` | 335 (was 667) | narrative facade — within the 370 budget, 35 lines spare |
| `ui/command_palette/policy.rs` | 860 new (`#[cfg(test)]` starts at line 420, so 419 production lines and 441 co-located unit tests) | pure policy |
| `ui/command_palette/evidence.rs` | 193 new | evidence surface |
| `ui/command_palette/query_execution.rs` | 186 new | coordination (query stage order, `execution`) |
| `ui/command_palette/index_admission.rs` | 119 new | coordination (index stage order, `admission`) |
| `ui/command_palette/index_execution.rs` | 176 new | coordination (index stage order, `execution`) |
| `ui/command_palette/retirement.rs` | 99 new | coordination (`retirement`: classification and accounting) |
| `ui/command_palette/test_policy.rs` | 100 new, wholly `#[cfg(feature = "test-utils")]` | test policy |
| `ui/command_palette/imp.rs` | 705 (was 761) | adapter detail |
| `ui/command_palette/item.rs` | 185 (unchanged) | adapter detail |
| `ui/command_palette/runtime.rs` | **deleted** | was the name the convention rejects |

### A.11 Intent-first renames (task 5.6)

| Before | After | Scope |
| --- | --- | --- |
| `CommandPaletteSearchRequest` | `PaletteQueryRequest` | `pub(super)` seam value |
| `runtime::execute_search` | `query_execution::search_palette_sources` | worker entry |
| `imp.search_runtime` | `imp.search_flight` | `pub(super)` field |
| `imp::rebuild_results` / `rebuild_results_owned` | `start_query_flight` (facade-facing) + `imp::restart_query` / `restart_query_if_open` | cross-module |
| `imp::apply_search_rows` | `imp::publish_search_rows` | cross-module |
| `imp::spawn_search` | `query_execution::dispatch_query_worker` | private, now mechanism-named inside the module that owns the mechanism |
| `enqueue_index_update` | `admit_index_update` | cross-module |
| `schedule_index_update_flush` | `schedule_index_flush` | cross-module |
| `flush_index_updates` (worker half) | `dispatch_index_mutation` + `settle_index_mutation` | cross-module |
| `set_guarded_file_index` body | `install_replacement_file_index` | cross-module |
| `close()` cancel block | `abandon_query_flight` | cross-module |
| `set_search_delay_for_test` / `set_index_update_delay_for_test` | `CommandPaletteTestPolicy::with_*` | test surface |
| tests' `palette.imp().rebuild_results(..)` | `palette.restart_query_for_test(..)` | test surface (was an ungated reach-through) |

`mode()`, `query()`, `result_count()`, `is_searching()`, `file_index_len()`,
`open_tab_source_count()`, and `pending_index_update_count()` keep their names and
move into `evidence.rs`, matching the exemplar: the evidence surface owns the
workflow's observation accessors and composes them.

### A.12 Premise corrections found during implementation

Two things the artifacts asserted that the code did not support. Both are flagged
rather than silently absorbed.

1. **The palette accelerator is `Ctrl+Shift+P`, not `Ctrl+P`.** The proposal, the
   matrix's `WFR-COMMAND-PALETTE` entry-points cell, and `.agents/rules/ui.md` all
   said `Ctrl+P`; `ui/window/actions.rs` binds `win.toggle-command-palette` to
   `<Control><Shift>p`, and `AGENTS.md` already said `Ctrl+Shift+P`. Corrected in
   the facade narration, `.agents/rules/ui.md`, the matrix cell (with a note), the
   matrix stage trace, **`proposal.md`'s own measured-problem paragraph**, and this
   appendix's stage trace — the last three were caught by the code review, which
   is fair: fixing the accelerator everywhere except the document that first
   asserted it would have left the change contradicting itself. `Ctrl+K` in that
   same matrix cell belongs to the recent-Open popover (`ui/open_popover/`), not to
   a palette mode. `README.md`'s `| Print | Ctrl+P |` row is correct and untouched:
   `Ctrl+P` really is Print.
2. **Task 5.3's three-way split of `runtime.rs` needed a fourth destination.**
   The task assigned the value object and `execute_search` to "the coordination
   module that submits it" and `SEARCH_DELAY_MS` to `test_policy.rs`. That holds,
   but `execute_search`'s *call site* was in `imp.rs`, not in `runtime.rs`, so the
   worker-spawn and completion-arbitration code moved out of `imp.rs` too. The
   task's premise (three contents, three owners) was right about `runtime.rs`; it
   just understated where the query flight actually lived.

Neither changes the design the artifacts specified.

### A.13 Cold-read verification (task 11.9)

Read `crates/lushtext-core/src/ui/command_palette/mod.rs` alone, without opening
`policy.rs`, `query_execution.rs`, `index_admission.rs`, `index_execution.rs`, or
`retirement.rs`:

- both stage orders appear in order with their intent named (4 query stages, 7
  index stages);
- all **eight** inversions are named with the module and callback where control
  resumes, and the two that re-enter an earlier stage say which stage
  (the flush debounce and the capacity wakeup both re-enter stage 3; a rejected
  batch re-enters via the flush debounce);
- the one structural fact a reader could not otherwise infer is stated
  explicitly: the *full* index replacement path bypasses the queue and advances
  the same generation counter, which is what makes stage 6's arbitration
  necessary at all. Without that sentence the arbitration reads as defensive
  code against nothing;
- the role table names every module and classifies `imp`/`item` as adapter
  detail, so a reader knows which files are *not* part of the narrative.

Two things the facade does **not** answer, deliberately, because they are the
coordination tier's business and the facade would become a second
implementation: the exact byte arithmetic of queue admission, and the per-source
result cap's value. Both are named as concepts and located by module.

### A.14 Behavior-equivalence coverage (tasks 11.6, 11.7)

**Task 11.6 — palette state extremes.** All already covered by widget tests that
were rewritten in place rather than replaced, so equivalence is asserted against
the same assertions as before the migration:

| State extreme | Widget test |
| --- | --- |
| no results | `test_command_palette_no_results_label_on_no_match`, `test_command_palette_notes_mode_empty_source_has_no_fake_rows`, `test_command_palette_files_mode_empty_workspace_index_clears_workspace_group` |
| one / few representative results | `test_command_palette_search_filters_results`, `test_command_palette_set_file_index` |
| mixed results across Files, Notes, Commands, All | `test_command_palette_all_mode_groups_sources_by_priority`, `test_command_palette_files_mode_groups_open_tabs_before_workspace_files`, `test_command_palette_notes_mode_groups_note_records_by_category`, `test_command_palette_commands_mode_keeps_note_commands_with_notes_subtitle` |
| dense results, long paths | `test_command_palette_notes_mode_handles_dense_awkward_rows` |
| mode switch mid-query | `test_command_palette_latest_mode_index_and_scope_snapshot_wins`, `test_command_palette_tab_syncs_mode_dropdown` |
| superseded query publishes nothing | `test_command_palette_rapid_queries_keep_one_active_one_latest_and_final_accessibility`, `test_command_palette_close_cancels_active_and_pending_without_stale_projection` |
| constrained-width palette | `test_palette_dismissal_handles_dense_results_in_constrained_window` |
| headers, auto-selection, focus, accessible metadata | `test_command_palette_headers_do_not_activate`, `test_command_palette_open_focuses_entry`, `test_palette_accessibility_tracks_busy_and_selected_result_value`, `test_palette_row_accessibility_metadata_is_positioned_selected_and_clearable`, `test_command_palette_controls_expose_accessibility_roles` |

**Task 11.7 — incremental index mutation.** Two real gaps existed and were
closed with new widget tests; the rest was already covered.

| Path | Widget test | Status |
| --- | --- | --- |
| create reaches the index | `test_command_palette_incremental_index_worker_publishes_then_clears_readiness` | existed |
| **delete reaches the index** | `test_incremental_index_delete_reaches_the_index` | **added** — only Create was covered end to end |
| **rename reaches the index** | `test_incremental_index_rename_reaches_the_index` | **added**, asserting the new path is present, the old one retired, and the count unchanged |
| byte cap escalates to a rebuild | `test_incremental_index_update_queue_coalesces_overflow_to_one_rebuild` (8 KiB path segments) | existed |
| **count cap escalates to a rebuild** | `test_incremental_index_count_cap_escalates_independently_of_the_byte_cap` | **added** — the pre-existing overflow test uses 8 KiB segments, so 2,000 updates cross the 4 MiB byte ceiling long before the 1,024-update count ceiling; the count ceiling was untested. The new test uses short paths and asserts the queued bytes stay under half the byte ceiling, so it cannot silently become the byte case |
| applied batch loses to a concurrent full replacement, rejected and replayed | `test_command_palette_retires_last_owned_rejected_incremental_index_off_gtk` | existed |
| disposal-capacity refusal retries through the wakeup | `test_incremental_index_capacity_retry_is_paced_and_resumes_after_release` | existed |

### A.15 Exported D-Bus contract proven unchanged (task 7.5)

Not asserted — measured. `make automation-smoke` writes
`build/smoke/automation/assertions/snapshot-initial.json`, a full Automation1
`GetSnapshot` for the same deterministic app state, so the same fixture can be
captured on both trees:

```
# after: migrated tree
$ make automation-smoke                 # PASS
$ cp build/smoke/automation/assertions/snapshot-initial.json snap-after.json

# before: stash the change, keeping the openspec artifacts
$ git stash push -u -- crates scripts docs AGENTS.md README.md .agents
$ make automation-smoke                 # PASS
$ cp build/smoke/automation/assertions/snapshot-initial.json snap-before.json
$ git stash pop                         # clean, no conflicts
```

Result:

```
command_palette BEFORE: {"file_index_count": 0, "mode": "all", "open_tab_source_count": 1,
                         "pending_index_update_count": 0, "query": "", "result_count": 0,
                         "searching": false, "visible": false}
command_palette AFTER : {"file_index_count": 0, "mode": "all", "open_tab_source_count": 1,
                         "pending_index_update_count": 0, "query": "", "result_count": 0,
                         "searching": false, "visible": false}
command_palette identical: True
idle: before=True after=True                    same=True
idle_blocker: before=None after=None            same=True
surfaces.command_palette_visible: False False   same=True

ENTIRE snapshot identical (excluding app_version/build_profile)
```

The diff is zero not just for the `command_palette` object and the readiness
fields but for the **whole snapshot**, which is the stronger statement the task
was reaching for. `app_version` and `build_profile` were excluded from the
whole-snapshot comparison as build identity rather than behavior; both were in
fact also equal.

Eight fields in, eight fields out, same names, same types, same values, and the
`visible` flag still read from the window's palette revealer rather than from
evidence.

### A.16 Live run (task 11.8) — what was run, and the one gap

No LushText instance was running before or after (`flatpak ps`, `busctl --user
list`, and `pgrep -af '/target/debug/lushtext$'` all empty), so nothing of the
maintainer's was disturbed. Two live sessions were run.

**Session 1 — the literal `make run`.** Real desktop, real XDG state, freshly
built debug binary. Driven through `scripts/lushtext-automation.py` against the
live app:

| Step | Result |
| --- | --- |
| palette open | `visible=true`, 35 rows, `searching=false` |
| all four modes | `all` 35 rows, `files` 5, `notes` 0, `commands` 29 |
| dense query (`e`) | 30 rows |
| no-results query | 0 rows, `searching=false` |
| palette close | `visible=false`, `query=""`, `result_count=0` |
| focus restoration / readiness | `idle=true`, `idle_blocker=null` after close |

**stderr audit: zero** occurrences of `Gtk-WARNING`, `Gtk-CRITICAL`,
`GLib-GObject-WARNING`, pixman `*** BUG ***`, or `Trying to measure`. The only
non-clean lines were a Mesa `radv` notice and two pre-existing
`Cannot stat .../…: No such file or directory` errors from session restore of
paths that no longer exist — unrelated to the palette and present before this
change.

**Session 2 — a seeded workspace, because `make run` has no indexed folder.**
`file_index_count` was 0 in session 1: the maintainer's config registers no
workspace folder, so neither the file index nor the incremental mutation path
could run. Rather than write the maintainer's real `workspaces.json`, session 2
launched the same freshly built binary on the same live desktop with isolated
state (`LUSHTEXT_DATA_DIR`, `XDG_*_HOME`, `GSETTINGS_BACKEND=memory`) seeded with
one workspace over a 43-file fixture. This is a deviation from the literal target
and is recorded as one; it is strictly safer than mutating real user config.

| Step | Result |
| --- | --- |
| workspace restore | `workspace_count=1`, `scope_kind=workspace`, `scoped_folder_count=1` |
| **file index build** | `file_index_count=43` |
| palette in Files mode, query `alpha` | 2 rows |
| sidebar tree expansion (AT-SPI `listitem.expand`) | 43 file rows materialized |
| three open/query/close cycles | ended `visible=false`, `query=""`, `idle=true` |

**stderr audit: zero** GTK/pixman/measure warnings; the only non-clean line was
the Mesa `radv` notice.

**The one uncovered live step, and why.** The incremental index mutation is driven
**only** by sidebar context-menu file operations —
`ui/window/documents.rs`'s `handle_sidebar_file_renamed` / `_deleted` /
`_created` are the sole callers of `update_index_file_*`. It is **not**
watcher-driven: an external `mv alpha.md renamed-omega.md` inside the workspace
folder was verified live to leave the index untouched (`file_index_count` stayed
43 and the palette still matched `alpha`). That is pre-existing behavior this
change does not alter.

Driving it live therefore needs the sidebar context menu, and that is not
reachable here:

- the `section.*` file actions (`new-file`, `rename`, `delete`) are
  `exposure: widget-scoped` in the action catalog, so they are deliberately not
  on the window's `org.gtk.Actions` D-Bus surface;
- AT-SPI synthetic mouse events need absolute screen coordinates, and on this
  Wayland session **every** sidebar row reports extents `(0, 0, w, h)` —
  `beta.rs`, `dense-1.txt`, and the folder row all report `x=0, y=0`. A
  context-click computed from those extents lands at the top-left of the sidebar
  and opens no menu, which is what was observed. This is a GTK4-on-Wayland AT-SPI
  coordinate limitation, not an app defect.

**What covers it instead.** The incremental path is covered end to end by widget
tests, and this change *added* two of them because the coverage was genuinely
incomplete: create (pre-existing), **delete (added)**, **rename (added)**, byte-cap
escalation (pre-existing), **count-cap escalation (added)**, rejection-and-replay
(pre-existing), and disposal-capacity retry (pre-existing). See A.14. The gap is
in *live* proof of the sidebar-driven variant only, and it is an environmental
gap that predates this change.

### A.17 Review-pass fixes

Applied after the independent code review returned approve-with-fixes.

**Stage-label drift (was a real defect).** The facade's query stage order has
four stages, but `query_execution.rs` labelled its three operations 1-and-3, 4,
and 5 — so stage 2 was unlabelled, a non-existent stage 5 was claimed, and stage
4 was claimed twice because `imp::publish_search_rows` also legitimately claims
it. Renumbered to 1-and-2, 3, and 4, with `publish_search_rows` relabelled
"Query stage 4's widget mutation" and pointed at the coordination function that
owns the stage, so stage 4 has one owner and one adapter half.

The index order had the same class of error at the other end:
`index_execution.rs` labelled the tail-turn flush "Index stage 7" while the
facade defines stage 7 as retirement. **Resolved by promoting the tail flush to
stage 8 in the facade** rather than folding it into stage 6, because it is a
genuinely ordered step with its own inversion (the facade already documented that
inversion) and it runs *after* retirement in `settle_index_mutation`. Folding it
under 6 would have made the facade's own inversion list disagree with its stage
list. `settle_index_mutation` is now "stages 6 and 7" and the dispatch entry is
"stages 4 to 8".

Every stage label in all seven palette modules was then re-read: query claims
1, 2, 3, 4 exactly once each; index claims 1, 2, 3, 4-8, 5, 6-7, 8 consistently
with the facade's eight stages; the facade's "Five inversions" count still
matches the five it lists (both debounces, the capacity wakeup, the replay, the
tail). Zero drift remains.

**Drift-gate hardening (FIX 4).** The check validated the map's *evidence*
column against the Rust surface but not its *snapshot* column, so a snapshot
field renamed in Rust would leave the map naming a dead key — the pre-existing
anchor check catches the renamed documentation anchor, but the anchor and the map
are separate strings, so the map itself could rot silently.
`projected_snapshot_keys()` now parses the projection function's struct-init keys
and every documented row is checked against them, plus a prefix check that the
row's snapshot field actually lives under that projection's object. Two
self-test cases added (stale snapshot column; row pointing at the wrong object),
and the stale-map direction was observed failing on the real tree:

```
$ # map row's snapshot column changed to command_palette.open_tab_count
$ ./scripts/check-automation-docs.py
evidence projection map: missing 1 item(s)
  - window.command_palette: Evidence Projection Map documents evidence field
    `CommandPaletteEvidence.open_tab_source_count` -> snapshot field
    `command_palette.open_tab_count`, but `command_palette_snapshot` writes no
    such snapshot field
$ exit 1
```

**This is hardening, not the spec requirement.**
`workflow-evidence-surfaces`' "Projection drift is detected" scenario asks for a
failure naming the evidence field and the snapshot field when a *projected
evidence field* is added, removed, or renamed; that was already satisfied before
this fix (A.8). The snapshot-column check closes an adjacent hole the scenario
does not require. The spec claim in A.8 stands unchanged.

**`retirement.rs` role name — kept, with the counter-position recorded (FIX 5).**
The reviewer questioned whether the module earns a coordination role name at all:
the `DisposalOwned` drop at the `index_execution` call sites is what actually
retires the index, and this module only records whether the release qualified.
That observation is correct. The judgment is to **keep** the module and the name,
for two reasons: the classification predicate has three call sites and existed as
duplicated inline conjunctions before, so the module removes real duplication;
and the retirement/accounting split is inherited structure rather than something
this change invented. The module doc now says plainly that it owns retirement
*classification and lifecycle accounting* while the disposal lane
(`ui::plain_disposal`, cross-cutting) performs the retirement itself.

**Slot 2b and slot 5 should know:** if a later migration finds the same
split — a palette-local classifier plus a cross-cutting executor — and concludes
the role name belongs to the executor, that is a `gtk-adapter-module-boundaries`
question about whether a coordination role may name the *deciding* half of a job
whose *doing* half is cross-cutting. It was not resolved here because resolving
it would re-open `plain_disposal`'s settled cross-cutting placement.

**Recorded counts corrected.** The facade, `retirement.rs`, `imp.rs`, and the
test-count figures in A.9 and A.10 were stale, having been written before the
final rounds of edits. All were re-measured (see A.9's scope note: the number
depends on the command, so the command is now recorded with it). One figure in
the review did not reproduce: the reviewer reported 1576 non-widget tests, while
`--workspace --all-features` reports 1565, `--workspace` reports 1518, and
`-p lushtext-core --all-features` reports 1342. 1565 is the figure recorded,
because it is the same command as the 1538 baseline and its delta is exactly the
27 `#[test]`s in `policy.rs`.

### A.18 Pre-existing blocker fixed in this work stream: default-feature Clippy

Found by the code review, and in scope per `.agents/rules/preexisting-blockers.md`.

`cargo clippy -p lushtext-core --lib` with **default** features failed with three
`clippy::unused_self` errors in files this change never touched:

```
crates/lushtext-core/src/ui/sidebar/workspace_section/watch_targets.rs:218  record_touched_rows(&mut self, ...)
crates/lushtext-core/src/ui/window/drafts.rs:1818                          note_complete_draft_body_admitted(&self) {}
crates/lushtext-core/src/ui/window/drafts.rs:1826                          note_complete_draft_body_released(&self) {}
```

**Why it was never seen.** The declared blocking gate is
`cargo clippy --workspace --all-targets --all-features -- -D warnings`, and under
`--all-features` the `test-utils` body *does* read the receiver, so the lint does
not fire. This is the same trap as the recorded feature-gated-import incident:
an all-features-only gate cannot see a default-feature-only diagnostic.

**Cause, in all three cases the same shape.** Each method's receiver is read only
inside a `#[cfg(feature = "test-utils")]` body, with a production counterpart
that is either an empty no-op method or a `let _ = arg;` fallthrough. In a
default-feature build the receiver is genuinely dead.

**Fix, following the repo's own precedent** — feature-gated no-ops in this tree
are already written as *free functions* (`fn delay_replace_reload_facts_for_test()
{}` in `ui/window/search.rs`), not as methods with a dead receiver. These three
keep needing instance state under `test-utils`, so the equivalent move is to gate
the definition together with its call sites rather than keep a production no-op:

- `record_touched_rows` is now `#[cfg(feature = "test-utils")]`, and its three
  call sites are gated, with `let _ = touched;` retained in the two that would
  otherwise have an unused parameter.
- `note_complete_draft_body_admitted` / `_released` lost their
  `#[cfg(not(feature = "test-utils"))]` no-op twins, and their two call sites in
  the draft-body worker handoff are gated.

No `#[allow]` and no `#[expect]` was added: in every configuration where these
functions now exist, the receiver is genuinely used, so there is no lint to
suppress. Both surviving definitions carry a doc comment stating why they are
gated with their call sites, so the next reader does not "simplify" the production
no-op back in.

**Verification:** `cargo clippy -p lushtext-core --lib` (default features) exit 0;
`cargo clippy -p lushtext-core --lib -- -D warnings` exit 0; the full
`cargo clippy --workspace --all-targets --all-features -- -D warnings` gate still
exit 0; and the two touched modules' tests pass (113 selected, 113 passed).

**Standing-guidance gap this exposes.** `.agents/rules/build.md` documents the
blocking Clippy command as the all-features one and requires it to stay identical
everywhere. That is still right, but it means default-feature-only lints have no
gate. Raised here rather than silently fixed, because adding a second Clippy
invocation to `make check` is a build-policy change that belongs in its own
change with its own runtime budget discussion.

### A.19 Second pre-existing blocker: a real flake, diagnosed and fixed

The post-fix widget lane reported one `FLAKY:` line —
`workspace_section::test_large_reconciliation_is_batched_supersedable_and_preserves_state`
passed only on attempt 2. Per `.agents/rules/preexisting-blockers.md` a recovered
flake is a blocker, not accepted noise, so it was read rather than re-run away.

**Which wait, exactly.** Not the obvious ones. The panic is
`condition was not met within 30s` at the proof harness's `wait_until`, and it
always lands *after* the test's `workspace-cache-runtime-evidence` print, which
locates it at the final wait of the test:

```rust
section.imp().refresh_button.emit_clicked();
wait_until(Duration::from_secs(30), || {
    section.reconciliation_metrics_for_test().4 > 0   // .4 = child_reconcile_sources.len()
});
```

**Root cause — a transient the waiter destroys, with the two timings coupled.**
That predicate samples an *in-flight* child-reconcile source, which by
construction exists only *between* batches: `schedule_next_reconcile_batch` arms
a `glib::timeout_add_local_once(batch_delay, ..)` and inserts it into
`child_reconcile_sources`, and `finish_child_reconciliation` arms nothing, so the
count returns to zero the moment the last batch lands. Meanwhile the harness's
`wait_until` is `predicate(); sleep(DEFAULT_POLL_INTERVAL); flush_events();` with
`DEFAULT_POLL_INTERVAL = 20ms` and `flush_events()` draining **every ready source
to exhaustion**.

The test sets the batch delay to **20 ms** — *exactly* the poll interval. So
whenever applying a batch (256 changed rows over a 1001-row tree) makes one drain
span the next 20 ms timer, that single drain consumes the remaining batches, the
source is released, and **no later poll can ever observe a non-zero count**. The
30 s budget is irrelevant: once missed, the transient is gone forever. That is
also why it is load-sensitive — a busier machine makes the drain longer — and why
a bigger timeout would have been the wrong "fix".

**Attribution: not caused by this change.** Measured symmetrically, one isolated
test per subprocess, `--retries 0`:

| Tree | Isolated runs | Failures |
| --- | --- | --- |
| clean (change stashed) | 20 | 0 |
| this change | 25 | 1 |

Fisher exact on 1/25 vs 0/20 is p ≈ 0.44 — statistically indistinguishable. The
single failure landed on a cold first run right after a heavy build, and inside a
1109-test lane a ~2–4% per-test rate surfaces often. There is also **no
mechanism**: the addendum's only edit in this directory is
`watch_targets.rs::record_touched_rows`, a write-only probe counter that is
`cfg`-identical under `test-utils` (the build widget tests use), while the failing
predicate reads `child_reconcile_sources` in `refresh.rs` — a different module
this change never touches, and one the flaky test reaches without going through
watch targets at all.

**Fix — decouple the two timings, test-only.** The final phase now raises the
batch delay to 200 ms before its `emit_clicked()`, so one poll-plus-drain cannot
consume the whole reconciliation. This changes only timing the test already owns
through `set_reconciliation_batch_delay_for_test`; no product code changed, and
the assertion's meaning is preserved exactly — the subsequent drop check still
proves disposal releases a *genuinely live* source's weak owner, which is why the
predicate was not weakened to a monotonic counter instead.

**Deliberately not done:** widening the 30 s budget (cannot help — the transient
is already unobservable), and changing `wait_until`'s drain-to-exhaustion
mechanism (its doc marks that behavior load-bearing for `gtk-lush-tasks` idle
completions, and `preexisting-blockers.md` forbids changing a working wait
helper's mechanism without proving the replacement against the real async
delivery path).

**Note for slot 5.** `WFR-WORKSPACE-TREE` owns this test. The general lesson is
worth carrying: any widget predicate that samples an in-flight GLib source is
unobservable unless its lifetime comfortably exceeds one
`DEFAULT_POLL_INTERVAL` plus one drain. Slot 5 should check the other
`reconciliation_metrics_for_test().4 > 0` style waits for the same coupling.
