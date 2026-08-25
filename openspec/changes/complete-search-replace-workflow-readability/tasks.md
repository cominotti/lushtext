## 0. Gates and orientation

- [x] 0.1 **Two-proof gate — blocking.** Slot 1 is already archived at
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/`
      with its five deltas in `openspec/specs/`. Confirm the second proof: slot 2a
      (`migrate-command-palette-workflow-readability`) is archived;
      `docs/workflow-readability-matrix.md` marks `WFR-COMMAND-PALETTE` `migrated`
      with a complete `Migrated Workflow Roles` subsection;
      `docs/next/workflow-readability.md`'s ledger marks slot 2a `complete`; and
      `make check-workflow-boundaries` passes on a clean tree. Also confirm the two
      slot-2a deliverables this change depends on are present: the stage-order
      qualification rule in
      `openspec/specs/gtk-adapter-module-boundaries/spec.md`, and the working
      evidence-to-snapshot drift check in `scripts/check-automation-docs.py`. This
      change touches a tier-3 path that rewrites the user's files. Do **not**
      proceed on one proof.
- [x] 0.2 Read `docs/next/workflow-readability.md` (status line, "Slot 1 residue",
      slot ledger, deferred work) and `docs/workflow-readability-matrix.md` rows
      `WFR-SEARCH-REPLACE`, `WFR-AUTOMATION-SPINE`, `WFR-BUFFER-REPLACEMENT`,
      plus the `Settled Conventions`, `Facade size budget`,
      `Migrated Workflow Roles`, `Policy Module Census`, `Seam Value Objects`, and
      `Completion Rule` sections. Read all five capability specs. Read slot 2a's
      task 12.2 handoff notes: they record convention friction this change
      inherits.
- [x] 0.3 Read the code before changing it and write the current ordered stages
      into this file: `ui/search_panel/replace.rs`, `ui/search_panel/mod.rs`'s
      Replace All narration, `ui/window/search.rs`'s replace and undo path,
      `services/search_backup.rs`, and the write path in
      `services/content_search/replace.rs`. Name every inversion and its
      resumption point.
- [x] 0.4 Invoke the `data-safety` skill in explicit mode over the intended diff
      surface before writing code, and again in task 8 over the actual diff.
      Record both.
- [x] 0.5 Re-verify this change's premises against the tree. The authoring
      inventory found that two residue items are not what the residue text says
      (tasks 1 and 2). Confirm that is still true, and record any drift here
      instead of working from stale findings.

## 1. Retire the `activate_undo_replacements` residue item with evidence

- [x] 1.1 Confirm and record that `LushtextSearchPanel::activate_undo_replacements`
      in `ui/search_panel/mod.rs` is already a documented one-line delegation to
      `replace::hand_back_undo_backup`, reading no transaction state and mutating
      no widget — i.e. slot 1's result-cap fix already discharged the residue item
      as written. Quote the function in this file as the evidence.
- [x] 1.2 Confirm the delegate itself is correctly placed: `hand_back_undo_backup`
      refuses while the apply transaction is claimed, takes the published backup,
      retracts the affordance so the same journal cannot be handed back twice, and
      invokes the window callback. Record the transaction state it reads and the
      widgets it mutates, and confirm all of it belongs to the coordination role
      rather than the facade.
- [x] 1.3 Fix the asymmetry that **does** remain, one layer out. In
      `ui/window/search.rs`, the undo path re-reads and re-mutates panel state
      inline before spawning the undo worker — claiming the replace transaction,
      showing the undo button on both early-return paths, reserving undo capacity
      — and installs the remainder backup inline afterwards. Give the panel one
      named operation per step so the window delegates the way the panel's own
      stage 4 does, and keep the transaction and generation guards on the panel
      side where they already live.
- [x] 1.4 Update the residue section of `docs/next/workflow-readability.md`: mark
      the `activate_undo_replacements` item discharged, and state that the real
      residual asymmetry was on the window side and was fixed here. A future
      session must not re-open an item that is done, nor conclude the window-side
      work was skipped.

## 2. Settle `model/workspace_search.rs` — the decision is that it stays

- [x] 2.1 Re-derive the full reference set rather than trusting the matrix cell,
      which records "2 consumers, both search". Expect at least:
      `services/content_search/search.rs` (five imported types),
      `ui/search_panel/execution.rs` (the traversal plan),
      `model/content_search.rs` (two types embedded in public enum variants), and
      `crates/lushtext-core/benches/benchmarks.rs` (two types addressed
      directly). Also check `crates/lushtext-core/tests/workspace_terminology.rs`,
      which names the file path literally.
- [x] 2.2 Record the decision: **it stays in `model/`.** A service and a `model/`
      sibling both depend on it, so relocating it under `ui/` would invert
      dependency direction (`services -> ui`), which the convention forbids
      outright. It is already pure (no GTK-family import), already mutation-scoped
      through `model/**`, and already carries seven co-located unit tests, so the
      relocation would trade a dependency-direction violation for nothing.
- [x] 2.3 Correct the matrix. Move the row out of "Additional single-workflow
      modules the census found" and into "Modules confirmed as domain and staying
      in `model/`", with the corrected consumer list and the dependency-direction
      reason. Keep a short note at the old location pointing at the resolution, so
      a reader following the census snapshot does not conclude the decision is
      still open.
- [x] 2.4 Confirm no `.agents/rules/*.md`, skill, or doc asserts that this module
      is pending relocation, and fix any that does.

## 3. Amend the bounded coordination role set

- [x] 3.1 Record, from the code, the two cohesive halves of `replace.rs` **and the
      state they share**, with the post-split owner of each shared field. Three
      fields on `SearchPreviewState` (`ui/search_panel/imp.rs:153-189`) are touched
      by both halves and each needs a named owner plus a named operation the other
      half calls: `replace_transaction_pending` (`:164`), written by the journal
      half's transaction gate but read by the preview half in
      `begin_confirmed_replacement` (`replace.rs:713`) and
      `update_replace_button_sensitivity` (`:827`);
      `replace_transaction_generation` (`:165`), reserved by preview selection and
      consumed by the durable apply (`:199-235`); and `undo_backup_generation`
      (`:159`), threaded through `begin_replace_transaction` into the journal
      installs. The halves are **not** state-disjoint; do not attempt to prove that
      they are. The design question this task answers is who owns each of the three
      and what the crossing operation is called.
- [x] 3.2 Confirm the naming situation before amending: `ui/search_panel/` already
      spends `execution.rs` on the streaming search stage order and
      `retirement.rs` on bounded disposal, for the same workflow, so the Replace
      All stage order's two modules cannot take either name. The *collision* is
      already resolved by the stage-order qualification rule slot 2a added. What is
      still missing is a name for the journal half's job — maintaining a durable,
      generation-guarded record a later stage reads back, with startup recovery —
      which none of `admission`, `execution`, `retirement`, or `watch` describes and
      which `retirement` actively contradicts.
- [x] 3.3 Apply this change's `gtk-adapter-module-boundaries` delta: add `journal`
      to the bounded set with the scenario distinguishing it from `retirement` and
      `execution`. That is the delta's only content — do not re-add the
      qualification rule (slot 2a owns it) and do not widen the set further.
- [x] 3.4 Retroactive-amendment obligation: re-check **every** row the matrix
      marks `migrated` against the amended role set and record the result per row.
      At this point that is `WFR-SEARCH-REPLACE` and `WFR-COMMAND-PALETTE`. Adding
      a role name does not invalidate an existing correct name, so this is expected
      to be a confirmation rather than a rename; if a row's declared name is
      genuinely wrong under the amended set, fix it here, because two generations
      of the convention must not coexist in the tree.
- [x] 3.5 Update `.agents/rules/rust.md`'s bounded role-name list, the matrix's
      "Role file names" table, and any skill that enumerates the role names, so no
      standing guidance still lists the pre-amendment set. Run
      `make check-agent-docs` and `make check-agent-skills`.

## 4. Split `replace.rs` into role-named coordination modules

- [x] 4.1 Split along the preview / journal seam established in task 3.1. The
      preview module takes the preview-attempt lifecycle: ticket issue, generation
      open, capacity reservation and its retry parking, single-flight coalescing,
      worker dispatch, publish-or-retire on completion, superseded-preview
      release, the queued-request drain, preview-mode enter and exit,
      search-state-change invalidation, the checked-selection claim and apply, and
      the three preview widget-mutation helpers (button sensitivity, preview
      summary, summary restore). The journal module takes the transaction gate,
      undo-capacity reservation, generation reservation, generation-guarded install
      and clear, the worker-side disk save and delete, startup recovery with stale
      cleanup and diagnostics, the capacity retry, the undo affordance toggles, and
      the hand-back.
- [x] 4.2 Name both modules from the amended bounded set, using the stage-order
      qualification rule where a bounded name is already spent. Record the chosen
      names and the reasoning in the matrix row's notes.
- [x] 4.3 Keep the crate-visible seam stable: the operation the window uses to
      convert a disposal reservation plus a raw backup into a guarded undo backup
      is re-exported from the facade today and must keep working from its new
      home, with an intent-first name.
- [x] 4.4 Rename cross-module operations for intent as part of the split, and give
      each of the three shared fields from task 3.1 one named crossing operation
      instead of a direct field read from the other half.
      In particular, fix `retire_undo_backup_off_main` (`replace.rs:488-494`),
      whose name promises off-main retirement while its body drops the value
      synchronously on the calling thread. **Default to renaming it** so the name
      matches the behavior; that is a pure readability fix with no runtime change,
      which is what this programme is for. Changing the *behavior* to match the
      name instead would move a document-sized drop off the GTK thread — a real
      responsiveness change requiring its own retirement-path proof and an explicit
      reversal of this change's "no behavior change" non-goal. Do not make that
      change incidentally; if it is warranted, it is a separate change.
- [x] 4.5 Keep mechanism names only on private helpers inside the module that owns
      the mechanism.
- [x] 4.6 Confirm the split changed no behavior: no reordering of the transaction
      claim relative to the capacity reservation, no change to when the undo
      affordance appears or disappears, no change to which early-return path
      restores which state, and no change to the order in which the three shared
      fields from task 3.1 are read and written.

## 5. Extract the durable half's pure policy

- [x] 5.1 Capture the mutation baseline **first**. `ui/search_panel/replace.rs` is
      **not** in the mutation scope today, so the pure logic about to move
      generates zero mutants. Record that explicitly as the baseline, and note the
      asymmetry: for logic moving from an unscoped file into a scoped
      `policy.rs`, parity means **gaining** mutants that are all killed, which is
      strictly stronger than the requirement's equal-counts phrasing. The
      equal-counts clause governs relocations between two scoped locations.
- [x] 5.2 Move into `ui/search_panel/policy.rs`: the preview reservation weight
      and the completed-outcome shrink-to weight, the retained-byte saturating
      cast, the undo-capacity admission arithmetic currently inline in the undo
      reservation (which must account for the installed backup plus the transient
      input weight against the retained-bytes ceiling), and the generation-match
      predicates inline in the generation-guarded install and clear.
- [x] 5.3 Every moved function must take primitives or plain value objects, never
      `&self` on a GObject, so the module keeps zero `gtk4`, `glib`, `gio`,
      `libadwaita`, or `sourceview5` imports. `make check-workflow-boundaries`
      enforces this and names the file and import on failure.
- [x] 5.4 Add co-located `#[cfg(test)]` unit tests for each moved decision,
      including each cap boundary (at, one under, one over), a saturating case, and
      each generation-mismatch rejection independently.
- [x] 5.5 Name any policy literal this module now owns as a typed constant beside
      its decision, per the literal-ownership rule in `.agents/rules/rust.md`.
- [x] 5.6 Prove the outcome: run the focused scoped mutation run against
      `ui/search_panel/policy.rs` using the two-part scoping documented in
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`
      (`MUTANTS_RE` plus a `MUTANTS_EXCLUDE` glob, because `MUTANTS_RE` alone does
      not filter the `delete field` mutant kind), and confirm every newly generated
      mutant is caught or unviable with a stated reason. Note that this module
      already carries the slot-1 population, so report the pre-existing and newly
      added mutants separately.

## 6. Decide `services/search_backup.rs`'s buried policy

- [x] 6.1 Enumerate the rules buried in its three large loaders and confirm the
      duplication before deciding: the journal-activation decision (twice, once
      per loader shape), the per-entry payload budget arm (twice), the manifest
      entry-count cap check (three times), the manifest/marker/`.json`
      payload-file filter (twice), manifest entry dedup by entry file and by
      target path, the retained-weight admission, and the cleanup-replacement
      eligibility rule.
- [x] 6.2 Record the placement decision with its reason. The constraint is that
      these rules **cannot** move to `ui/search_panel/policy.rs`: a service must
      not depend on the adapter. The real choice is between a
      `services/search_backup/policy.rs` and private pure functions with direct
      unit tests in place. Both are already inside the `services/**` mutation
      scope, so this is a testability and de-duplication decision, not a coverage
      one — say so, and do not claim a mutation-coverage benefit that does not
      exist.
- [x] 6.3 Whichever placement is chosen, de-duplicate each rule to one
      implementation and give it direct unit tests that do not need a tempdir:
      activation with and without diagnostics and with a manifest/entry count
      mismatch; the payload budget at and over the cap; the cap check at and over
      the entry limit; the payload-file filter accepting an entry and rejecting the
      manifest, the cleanup marker, and a non-`.json` file; dedup rejecting a
      duplicate entry file and a duplicate target path; and cleanup eligibility
      refusing when any diagnostic disallows replacement.
- [x] 6.4 Do not change any on-disk shape, file name, manifest field, cap value,
      diagnostic reason, or activation outcome. This is a restructuring of where
      the decisions live, and every existing `search_backup` test must pass
      unchanged.
- [x] 6.5 Classify the seam surface on the write side for the record:
      `services/content_search/replace.rs` carries configuration (a byte-cap
      thread-local plus its setter), probe (the active-journal assertion), and
      actuation (the after-metadata hook registry and the before-rename
      fault-injection guard) seams, plus three inspection functions over those
      fault-injection registries. Record the classification in the matrix row and
      leave the actuation seams alone: they are the fault-injection mechanism this
      change's failure-path verification depends on.

## 7. Extend the replace/undo evidence surface

- [x] 7.1 Add the fields the durable half needs, at minimum: the apply
      transaction's pending flag as its **own** field rather than folded into
      preview-pending; the transaction generation; the undo-backup generation; the
      preview generation; the preview capacity-retry armed state (the undo twin is
      already exposed); the installed backup's entry count and retained weight; an
      in-flight journal disk-job counter analogous to the existing preview
      selection-job counter; and the last apply result's replaced, skipped, and
      error counts.
- [x] 7.2 Respect the reentrancy constraint the surface's own module doc records:
      the accessor takes shared borrows of the queued preview request and the undo
      backup, so no new field may be read from inside a `borrow_mut()`. Prove it
      with a test that reads evidence at each point where the workflow holds a
      mutable borrow today.
- [x] 7.3 Confirm reading the extended surface still mutates nothing — no timer,
      queue, generation counter, coordinator, or disposal reservation — and still
      does not require the workflow to be in a particular stage.
- [x] 7.4 Migrate the widget tests that reach around the widget into the
      `search_backup` service. `crates/lushtext/tests/widget/search_panel.rs` has
      **35 direct call sites: 16 `load`, 4 `save`, 15 `delete`.** The migration
      target population is the subset whose question is *"did the workflow record
      or clear it"* — predominantly the `load` and `delete` assertions — which
      should read evidence instead. Keep the direct service call where the question
      is genuinely *"what bytes are on disk"* (round-trip fidelity, byte-exact undo
      restoration) and where the test is *arranging* state rather than asserting it
      (the `save` sites). State per site which category it is, and report how many
      of the 35 moved. Do not add a per-field `*_for_test` accessor for anything: a
      test needing a fact the surface lacks extends the surface.
- [x] 7.5 Expect **no** inspection functions to retire in `replace.rs`: slot 1
      already retired all eight into `evidence.rs`, and the **five** remaining
      `_for_test` functions there (`clear_undo_backup_for_test`,
      `reserve_undo_backup_generation_for_test`,
      `set_persisted_undo_backup_for_generation_for_test`,
      `begin_replace_transaction_for_test`, `finish_replace_transaction_for_test`)
      are actuation seams that stay per the programme-level deferral. Record that
      so this task is not read as incomplete. Confirm the project test count does
      not decrease.
- [x] 7.6 Confirm the workflow's test-only timing and limit overrides all still
      live in the single `test_policy.rs` value, and that any override this change
      needs is added there rather than as a new module-level static.

## 8. Automation: prove no widening

- [x] 8.1 Confirm every documented `window.content_search` field except `visible`
      still projects from the evidence surface, and that `visible` is still window
      shell state. Slot 1 established this; this change must not regress it while
      the surface grows.
- [x] 8.2 Confirm the new evidence fields from task 7 — generations, disk-job
      counts, backup entry counts and weights, last-apply detail — are **not**
      serialized into the snapshot. The evidence surface is internal; only
      documented contract fields cross the D-Bus boundary, and existing redaction
      and omission behavior for private state is preserved.
- [x] 8.3 Confirm the `workspace-search` and `replace-preview` readiness blockers
      keep their documented semantics.
- [x] 8.4 Keep the evidence-field drift check slot 2a implemented in
      `scripts/check-automation-docs.py` correct for the extended surface: update
      the projection paragraph and the evidence-field-to-snapshot-field mapping in
      `docs/automation-reference.md` that the check reads, and confirm the check
      classifies each field added in task 7 correctly — projected fields must be
      documented, and internal-only fields (generations, disk-job counts, backup
      weights) must be ignored rather than demanded. If the check does not exercise
      the new fields at all, its mapping is stale and this task is not done. Run
      `make check-automation-docs` and `make automation-client-self-test`.
- [x] 8.5 Prove it rather than assert it: capture an Automation1 snapshot for the
      same app state before and after, and diff the `content_search` object and
      the readiness fields to zero differences.

## 9. Facade and matrix completion

- [x] 9.1 Update `ui/search_panel/mod.rs`'s Replace All stage narration and role
      table for the new module names, keeping both stage orders, all inversions,
      and their named resumption points. The facade must still own no timer,
      generation counter, admission bookkeeping, or widget mutation beyond the
      entry-point surface its module doc carves out.
- [x] 9.2 Measure the facade and confirm it is within the 370-line budget slot 2a
      declared. The starting point is 350, so the working headroom is 20 lines and
      the Replace All narration is about to grow by at least two module names.
      **At, say, 372 lines the answer is to delegate more, not to amend the
      budget**: fold narration detail into the coordination modules' own module
      docs and keep the facade's stage list to intent plus delegate plus resumption
      point. Raising the number is a convention amendment that would require
      re-migrating every migrated row in this change, and by now that is two
      workflows. If delegation genuinely cannot fit the narration, stop and escalate
      rather than quietly editing the budget line.
- [x] 9.3 Update the matrix's `### WFR-SEARCH-REPLACE` subsection under
      `Migrated Workflow Roles`: the new coordination module names, the policy and
      evidence paths, and a `mutation parity` pointer to this change's evidence
      file. Remove the note saying `replace.rs` keeps a workflow-descriptive name
      pending slot 2, and replace it with what was decided.
- [x] 9.4 Update the row's cells: `Owned pure policy` for the durable half's
      relocated decisions, `Seams (i/c/a/p)` for the post-migration reality,
      `Evidence surface` for the extended surface, `Risk` to record that the
      tier-3 half is now covered, and `Slot` to `1 (search/preview half) + 2b
      (replace/undo half)`.
- [x] 9.5 Update the row's `Post-migration note` in "Workflow Stage Traces" so the
      Replace All trace names the current operations and modules, including the
      post-split owner of each of the three shared fields from task 3.1.
- [x] 9.6 Write the mutation parity evidence to
      `openspec/changes/complete-search-replace-workflow-readability/evidence/mutation-parity-replace-policy.md`,
      following the structure of
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`:
      scope re-verification with the exact commands, the before/after table,
      per-survivor disposition, the pre-existing slot-1 population reported
      separately from this change's additions, and the
      out-of-scope-to-in-scope asymmetry from task 5.1 stated plainly.
- [x] 9.7 Advance `docs/next/workflow-readability.md`: flip slot 2b's ledger line
      to `complete`, drop the `(partial)` marker from `WFR-SEARCH-REPLACE` since
      the row is now migrated end to end, carry `WFR-AUTOMATION-SPINE` forward as
      `(partial)` on the complete line **and** onto slot 3's outstanding line
      (omitting it from every outstanding line fails the gate; marking it
      `migrated` would be a false claim). Add `WFR-AUTOMATION-SPINE` to slot 3's
      row in the **remaining-scope table** as well, not only to its ledger line, so
      the prose and the machine-readable list agree about what slot 3 owes. Update
      the status line and the remaining-scope table, and empty the "Slot 1 residue"
      section — in both `docs/next/workflow-readability.md` and the matrix's
      parallel "Slot 1 residue that slot 2 inherits" paragraph — with a note that
      all six obligations are discharged and by which change.
- [x] 9.8 Update the baseline section of the record with this change's
      contribution, reporting **seams reified** as the primary unit and stating
      which long-signature definition (receiver-counted 88 or strict 43) any
      secondary figure uses.
- [x] 9.9 Update `AGENTS.md`, `README.md`, and any `.agents/rules/*.md` or
      `.agents/skills/**` reference naming a path this change moved.

## 10. Verification

- [x] 10.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries` (facade budget, role completeness, ledger
      agreement), `check-filesystem-boundary`, `check-automation-docs`,
      `check-accessibility-policy`, and `check-visual-proof-policy`.
- [x] 10.2 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines**. A recovered flake is a blocker per
      `.agents/rules/preexisting-blockers.md`. Record test counts before and after
      and confirm the total did not decrease.
- [x] 10.3 `make mutants-diff` clean, with the task 9.6 evidence attached and
      survivors closed by added tests rather than scope changes. If the wrapper's
      `git diff origin/main...` cannot see working-tree edits, use the explicit
      merge-base diff workaround recorded in
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`
      and record both runs.
- [x] 10.4 The mandatory proof lanes for `ui/` and widget-test changes, each from a
      clean artifact root: `make visual-geometry-smoke`,
      `make accessibility-smoke`, `make visual-smoke`.
- [x] 10.5 **Replace All and undo behavior equivalence**, each case with a widget
      test asserting user-visible and on-disk outcomes: no matches; one match in
      one file; many matches across many files; a partial check where only some
      rows are selected and only those are written; a preview superseded by a
      newer query, whose stale completion must publish nothing and retire its
      payload; a Replace All followed by undo restoring byte-exact original
      content; and a second undo attempt that must be refused rather than
      double-applied.
- [x] 10.6 **Journal failure-path equivalence**, using the existing
      fault-injection seams in `services/content_search/replace.rs`: a
      before-rename failure must classify as "previous bytes intact" and leave the
      document reported as unwritten; an after-rename durability failure must
      surface as durability-unconfirmed rather than a generic lost save; and an
      after-metadata hook must fire at the same point as before this change.
- [x] 10.7 **Startup recovery equivalence** across the journal's real states: a
      healthy journal that activates; a journal with diagnostics that must not
      activate; a journal with a duplicate target path; one over the entry-count
      cap; one over the payload budget; one over the retained-memory cap; an empty
      manifest; a cleanup-in-progress marker taking precedence; and a missing
      manifest. Each must produce the same activation outcome, the same
      diagnostics, and the same cleanup behavior as before.
- [x] 10.8 `make crash-recovery-smoke` clean, since the undo journal is
      recovery-relevant durable state.
- [x] 10.9 Re-run the `data-safety` skill in explicit mode over the actual diff and
      resolve every confirmed finding. A tier-3 change does not close with an open
      data-safety finding.
- [x] 10.10 **Live run:** `make run` against throwaway fixture workspace folders,
      performing a real workspace search, a real Replace All, and a real undo.
      Confirm the fixture files are rewritten and then restored byte-exactly, and
      that stderr has no new `Gtk-WARNING`, `Gtk-CRITICAL`,
      `GLib-GObject-WARNING`, pixman `*** BUG ***`, or `Trying to measure`
      output. Replace All mutates files: it must never be pointed at the
      maintainer's real workspace folders. Per the slot-1 precedent, if the
      maintainer's environment makes the literal target unsafe, record exactly what
      was run instead and what remains uncovered rather than silently substituting
      a headless run.
- [x] 10.11 Cold-read verification: with this change's conversation set aside, read
      only the facade and confirm both stage orders and every inversion are
      followable without opening the coordination or policy modules, and that a
      reader can tell where the durable write and the undo journal live. If not,
      the split in task 4 is wrong and must be revisited before archiving.
- [x] 10.12 `openspec validate complete-search-replace-workflow-readability
      --strict` clean.

## 11. Handoff

- [x] 11.1 Confirm the programme record and the matrix agree that
      `WFR-SEARCH-REPLACE` is complete, that slots 1, 2a, and 2b are complete, and
      that slot 3 is the next outstanding slot with `WFR-AUTOMATION-SPINE` carried
      onto its line.
- [x] 11.2 Record convention friction for slots 3 through 7: whether the amended
      role set was sufficient, whether the stage-order qualification rule read
      well in practice, whether the 370-line budget held on a facade narrating two
      stage orders including a durable write, and whether the evidence surface's
      reentrancy constraint needs to become a stated convention rather than a
      per-workflow module note. Three workflows are migrated after this change, so
      the retroactive-amendment rule is now materially more expensive than it was
      at slot 2.

---

## Appendix A — orientation record (tasks 0.2, 0.3, 0.5, 1.1, 1.2, 2.1, 2.2, 3.1, 3.2, 4.2, 6.1, 6.2, 6.5, 7.5)

### A.1 Gate evidence (task 0.1)

- Slot 2a archived at
  `openspec/changes/archive/2026-08-25-migrate-command-palette-workflow-readability/`.
- `docs/workflow-readability-matrix.md` line 87 marks `WFR-COMMAND-PALETTE`
  `migrated`, with a `### WFR-COMMAND-PALETTE` subsection under
  `Migrated Workflow Roles` (line 980).
- `docs/next/workflow-readability.md` ledger: `- slot 2a (complete):
  WFR-COMMAND-PALETTE, WFR-AUTOMATION-SPINE (partial)`.
- `make check-workflow-boundaries` passed on a clean tree ("2 workflow policy
  module(s) are pure and mutation-scoped, every migrated matrix row names
  complete, existing roles, and the programme record's slot ledger agrees with
  the matrix").
- Slot 2a's two dependencies are present: the stage-order qualification rule at
  `openspec/specs/gtk-adapter-module-boundaries/spec.md:162-169` (+ its scenario
  at `:194`), and the evidence-to-snapshot drift check in
  `scripts/check-automation-docs.py` (`EVIDENCE_PROJECTIONS`,
  `evidence_projection_findings`).

### A.2 Premise re-verification (task 0.5)

Every authoring premise held against the tree:

| Premise | Verified |
| --- | --- |
| `activate_undo_replacements` is already a one-line delegation | yes, `mod.rs:258-260` |
| `model/workspace_search.rs` reference set is larger than the matrix's "2 consumers" | yes, see A.4 |
| `replace.rs` is 994 lines with two cohesive halves | yes |
| three shared `SearchPreviewState` fields | yes, `imp.rs:159/163/165` (doc comments shifted the authoring line numbers by ~4); verified against the current file rather than restated |
| `retire_undo_backup_off_main` drops synchronously | **NO — premise corrected, see A.10.** The body drops an `Arc<DisposalOwned<..>>`, whose last-reference drop submits the payload to the disposal lane |
| five remaining `_for_test` functions in `replace.rs` | yes |
| widget tests: 35 `search_backup` sites = 16 load / 4 save / 15 delete | yes, exact |
| `ui/window/search.rs` undo path re-reads/re-mutates panel state inline | yes, `search.rs:373-474` |
| facade is 350 lines, budget 370 | yes |

No drift to report.

### A.3 Residue items (tasks 1.1, 1.2)

**Task 1.1 — `activate_undo_replacements` is already a delegation.** Verbatim
from `ui/search_panel/mod.rs`:

```rust
/// Trigger the visible Undo Replacements affordance through the normal callback.
///
/// Replace stage 4. `replace::hand_back_undo_backup` checks the apply
/// transaction, takes the current backup, and retracts the affordance before
/// handing the backup to the window callback. The durable undo journal and
/// its generation guards stay in `replace` and `services/content_search`.
pub fn activate_undo_replacements(&self) {
    self.hand_back_undo_backup();
}
```

It reads no transaction state and mutates no widget. Slot 1's result-cap fix
already discharged the residue item as written.

**Task 1.2 — the delegate is correctly placed.** `hand_back_undo_backup`
(`replace.rs:744-756`) reads `preview.replace_transaction_pending` (refuse while
claimed) and `preview.undo_backup` (the published journal), mutates the
`undo_button` (via `hide_undo_button`, which also refreshes accessibility
state), and invokes `callbacks.undo_callback`. Transaction-gate reads and undo
affordance retraction are journal coordination, not narration, so all of it
belongs to the coordination role.

### A.4 `model/workspace_search.rs` reference set (task 2.1) and decision (2.2)

Re-derived, not taken from the matrix cell:

| Consumer | What it uses |
| --- | --- |
| `services/content_search/search.rs:27-30` | imports `WorkspaceSearchFallbackClaim`, `WorkspaceSearchFallbackLedger`, `WorkspaceSearchFallbackLimits`, `WorkspaceSearchFallbackMetrics`, `WorkspaceSearchTraversalPlan` (five types); also names `WorkspaceSearchIncompleteReason` at `:1120` |
| `model/content_search.rs:14` | embeds `WorkspaceSearchFallbackMetrics` and `WorkspaceSearchIncompleteReason` in public enum variants |
| `ui/search_panel/execution.rs:27` | imports `WorkspaceSearchTraversalPlan` |
| `crates/lushtext-core/benches/benchmarks.rs:62-64` | addresses `WorkspaceSearchFallbackMetrics` and `WorkspaceSearchTraversalPlan` directly |
| `crates/lushtext-core/tests/workspace_terminology.rs:270` | names the file path literally |
| `model/mod.rs:34` | `pub mod workspace_search;` |

`model/automation.rs` and `ui/automation.rs` matches are the unrelated snapshot
field names `workspace_search_visible` / `workspace_searching`, not this module.

**Decision: it stays in `model/`.** A service (`services/content_search/search.rs`)
and a `model/` sibling (`model/content_search.rs`) both depend on it, so
relocating it under `ui/search_panel/` would invert dependency direction
(`services -> ui`), which the convention forbids outright. It is already pure
(no GTK-family import), already mutation-scoped through `model/**`, and already
carries co-located unit tests, so the relocation would trade a
dependency-direction violation for nothing.

### A.5 Current ordered stages before the change (task 0.3)

**`ui/search_panel/replace.rs` — preview stage order.**

1. `enter_preview_mode` (`:503`) — read accepted matches; `issue_preview_ticket`
   opens a generation; take the retired outcome and checked ids; set
   `preview_pending`, clear `preview_mode`; button label/sensitivity;
   `release_superseded_preview`. If a preview worker is already running, coalesce
   into `queued_preview_request` (superseding its ticket) and return; otherwise
   `spawn_preview_request`.
2. `spawn_preview_request` (`:543`) — budget from `replace_preview_budget()`;
   reserve `preview_reservation_weight(budget)` from disposal admission. On
   refusal, `arm_preview_capacity_retry` parks the request and arms the capacity
   wakeup (**inversion A**, resumes in the wakeup closure at `:611`, which
   re-checks `may_dispatch` before re-dispatching). Otherwise mark the worker
   running, install a cancel token, and dispatch `spawn_blocking_then`.
   **Inversion B**: control resumes in the completion closure at `:575`, which
   validates `ticket.is_current(&panel.preview_facts())`. Current publishes
   (checked ids, outcome, `preview_mode`, summary, results, accessibility, then
   `finish_preview_worker`); stale routes the payload to
   `spawn_guarded_preview_retirement`.
3. `finish_preview_worker` (`:688`) — drains `queued_preview_request`;
   re-dispatches when `may_dispatch`, otherwise drops the request and recurses
   (**inversion C**, a tail-recursive drain rather than a loop).
4. `begin_confirmed_replacement` (`:713`) — refuse while
   `replace_transaction_pending` or not in preview mode; take the outcome;
   `begin_replace_transaction()` or restore the outcome and return; take the
   checked ids; `issue_preview_ticket`; clear `preview_mode`, set
   `preview_pending`; "Preparing Selection…"; restore the search summary and
   results; `apply_checked_replacements`.
5. `apply_checked_replacements` (`:760`) — charge `preview_selection_jobs`;
   partition checked rows on a worker. **Inversion D**: the completion closure at
   `:781` validates the ticket. Current: clear pending, restore the label,
   refresh sensitivity/accessibility, then either `finish_replace_transaction`
   (empty selection) or invoke `replace_callback` (**inversion E** — control
   leaves the panel for `ui/window/search.rs` and returns only through
   `set_persisted_undo_backup_for_generation`,
   `clear_undo_backup_for_generation`, and `finish_replace_transaction`), or
   retire the selection and finish the transaction when no callback is
   registered. Stale: retire and finish the transaction.
6. `exit_preview_mode` (`:810`) and `invalidate_replace_preview_request`
   (`:846`) — invalidate the active attempt, release the superseded payload,
   restore the search summary.
7. Preview widget-mutation helpers: `update_replace_button_sensitivity`
   (`:827`), `refresh_preview_summary` (`:909`), `restore_search_summary`
   (`:947`).

**`ui/search_panel/replace.rs` — journal stage order.**

1. `try_reserve_undo_replacement` (`:104`) — read the installed backup's
   reservation weight, then reserve either a replacement or a fresh reservation
   against `MAX_REPLACE_UNDO_RETAINED_BYTES`.
2. `begin_replace_transaction` (`:197`) — claim `replace_transaction_pending`,
   reserve a journal generation, store `replace_transaction_generation`, disable
   both buttons, return a `ReplaceJournalFreshness`.
3. `take_replace_transaction` (`:226`) — the window-side handoff: read pending,
   take the reserved generation.
4. `supersede_prior_undo_for_replace` (`:239`) — drop the installed backup, hide
   the undo button, retire the payload.
5. `set_persisted_undo_backup_for_generation` (`:246`) — generation guard,
   install, retire the previous payload, refresh accessibility.
6. `clear_undo_backup_for_generation` (`:272`) — generation guard, then
   `clear_undo_backup`.
7. `set_undo_backup_in_memory` (`:286`) plus `save_undo_backup_on_disk`
   (`:433`) — cancel the capacity wakeup, bump the generation, install, then a
   worker takes the journal guard, re-checks the generation, and calls
   `search_backup::save` (**inversion F**, completion only logs).
8. `clear_undo_backup` (`:373`) plus `delete_undo_backup_on_disk` (`:461`) — the
   same shape for deletion (**inversion G**).
9. `load_persisted_undo_backup` (`:302`) — startup recovery. Reserve capacity or
   `schedule_persisted_undo_backup_retry` (**inversion H**, a disposal-capacity
   wakeup that re-enters this stage). The worker takes the journal guard,
   re-checks the generation, calls `search_backup::load_recovering`; active
   recovery owns the backup under the reservation, otherwise it runs
   `cleanup_stale` and reports diagnostics. **Inversion I**: the completion
   closure at `:343` re-checks the generation, installs the persisted backup and
   shows the undo button.
10. `finish_replace_transaction` (`:216`) — clear the generation and the pending
    flag, re-enable buttons, refresh sensitivity and accessibility.
11. `hand_back_undo_backup` (`:744`) — refuse while the transaction is claimed,
    clone the published backup, hide the button, invoke the undo callback
    (**inversion J**, control leaves for the window).
12. `show_undo_button` / `hide_undo_button` (`:129`, `:139`) — affordance
    toggles.
13. `retire_undo_backup_off_main` (`:488`) — releases the panel Arc; the
    guarded owner's drop is what retires the payload off-main (see A.10).

**`ui/search_panel/mod.rs` Replace All narration** names stages 1–4 and both
worker inversions, and states that "the durable write, journal, and undo restore
live in `services/content_search` and `ui/window/search.rs`".

**`ui/window/search.rs` Replace All path** (`:192-369`) —
`take_replace_transaction` or warn and return; build `skip_paths` and
`open_canonical_identities` from open editor tabs; nothing to apply → warn plus
`finish_replace_transaction`; `try_reserve_undo_replacement(reservation_weight)`
or warn plus `finish_replace_transaction`; `supersede_prior_undo_for_replace`;
worker `apply_replacements_if_current`. Completion publishes the status message,
then either `set_persisted_undo_backup_for_generation` + `show_undo_button` or
`clear_undo_backup_for_generation`, reloads affected tabs, and always calls
`finish_replace_transaction`.

**`ui/window/search.rs` undo path** (`:373-474`) — the residual asymmetry:
`begin_replace_transaction()` inline; on refusal `show_undo_button()` inline;
`try_reserve_undo_replacement(None)` inline; on refusal warn plus
`finish_replace_transaction` plus `show_undo_button` inline; collect open
identities; worker `undo_replacements_for_open_identities`. Completion reloads
tabs, then either `set_guarded_undo_backup` + `show_undo_button` inline, or
`clear_undo_backup`, then `finish_replace_transaction`.

**`services/search_backup.rs`** — `load_recovering` (`:148`) dispatches on the
journal directory's path status, falling back to `load_retired_backup`;
`load_journal` (`:392`) checks the cleanup marker, loads the manifest,
short-circuits to `load_incremental_journal` (`:561`) for incremental manifests,
enforces the entry-count cap, then walks manifest entries applying entry-file
dedup, target-path dedup, entry load, path agreement, and the payload budget,
and finally computes activation and orphan diagnostics. `save` (`:219`) deletes,
writes each entry, and calls `mark_journal_active`. `delete` (`:744`) writes the
cleanup marker first, removes the legacy file and the journal directory, and
syncs the parent. No control inversion: this module is synchronous worker code.

**`services/content_search/replace.rs` write path** — per-file: write the undo
journal entry, assert the active journal (probe seam), take the target write
guard, atomic replace with `BeforeRename` / `AfterRename` classification, and
run the after-metadata hook (actuation seam). The before-rename fault-injection
guard is consumed at `:1426`; the after-metadata hook runs at `:1322`.

### A.6 Split design (tasks 3.1, 3.2, 4.2)

**The two halves and the state they share.** All three shared fields get
`journal.rs` as their single owner, because every write already lives on the
journal side; the preview half loses its direct field reads in favour of two
named crossing predicates.

| Shared field | Post-split owner | Crossing operation the other half calls |
| --- | --- | --- |
| `replace_transaction_pending` (`imp.rs:163`) | `journal.rs` (the transaction gate writes it) | `journal::replace_transaction_claimed(&panel) -> bool`, called by `replace_execution::begin_confirmed_replacement`, `update_replace_button_sensitivity`, and `evidence` |
| `replace_transaction_generation` (`imp.rs:165`) | `journal.rs` (reserved by `begin_replace_transaction`, consumed by `take_replace_transaction`) | `journal::replace_transaction_generation_reserved(&panel) -> bool`, called by the durable-apply completion in `replace_execution::apply_checked_replacements` |
| `undo_backup_generation` (`imp.rs:159`) | `journal.rs` (every install, clear, disk save, and disk delete compares against it) | no preview-half crossing; it crosses to the window only inside `ReplaceJournalFreshness` |

**Naming.** `ui/search_panel/` already spends `execution.rs` on the streaming
search stage order and `retirement.rs` on bounded result disposal, both for this
same workflow. The Replace All stage order's preview half has the same
submit/dispatch/arbitrate shape as `execution`, so it takes the stage-order
qualifier the spec sanctions: **`replace_execution.rs`**. The journal half's job
— maintaining a durable generation-guarded record a later stage reads back, with
startup recovery — is described by none of `admission`, `execution`,
`retirement`, or `watch`, and `retirement` means the opposite, so this change
amends the bounded set with `journal` and the module is **`journal.rs`**,
unqualified because nothing else in the directory claims that role.

`execution.rs` is deliberately **not** renamed to `search_execution.rs`. The
spec's qualification rule puts the qualifier on the module whose fitting name is
already spent, and renaming a stable already-migrated coordination module (plus,
for symmetry, `retirement.rs`) would add churn to a tier-3 change that rewrites
the user's files. The palette qualified both of its execution modules because
both were created in one change; here only one is new.

### A.7 `services/search_backup.rs` buried policy (tasks 6.1, 6.2)

Duplication confirmed before deciding:

| Rule | Sites before |
| --- | --- |
| journal-activation decision | 2 (`:533` manifest, `:690` incremental) |
| per-entry payload budget arm | 2 (`:505`, `:645`) |
| manifest entry-count cap check | 3 (`:324`, `:435`, `:609`) |
| manifest / cleanup-marker / non-`.json` payload-file filter | 2 (`:600-606`, `:862-867`) |
| manifest entry dedup by entry file and by target path | 1 combined site (`:457-472`), plus the incremental duplicate-target detection at `:657-672` |
| retained-weight admission | 1 (`:673`) |
| cleanup-replacement eligibility | 1 (`:789-792`) |

**Placement decision: private pure functions with direct unit tests, in place.**
Not a `services/search_backup/policy.rs`. The convention's `policy.rs` role is
one per **workflow**, inside that workflow's own directory, and its purity rule
exists so `ui/**/policy.rs` reaches the default mutation scope. `search_backup`
is a service, not a workflow, and `services/**` is already examined, so a
`policy.rs` here would add a directory indirection and a second meaning for the
role name while buying nothing. These rules also cannot move to
`ui/search_panel/policy.rs`, because a service must not depend on the adapter.

**This is a testability and de-duplication decision, not a coverage one.** Both
placements sit inside the existing `services/**` mutation scope, so no mutation
coverage is gained or lost; the win is that staleness, budget, cap, filter,
dedup, and eligibility rules become unit-testable without a tempdir, and each is
written once.

### A.8 Write-side seam classification (task 6.5)

`services/content_search/replace.rs` carries:

- **configuration**: `MAX_REPLACE_UNDO_BYTES` test thread-local plus
  `set_max_replace_undo_bytes_for_test` (`:45-65`).
- **probe**: `assert_active_journal_before_write_for_test` (`:804`), called at
  `:703`.
- **actuation**: `fail_next_replace_before_rename_for_path_for_test` (`:98`) and
  `register_undo_after_metadata_hook_for_test` (`:172`) with its
  `UndoAfterMetadataHook` cleanup guard.
- **inspection over those registries** (3):
  `replace_before_rename_failure_is_armed_for_test` (`:114`),
  `undo_after_metadata_hook_registry_is_empty_for_test` (`:194`),
  `undo_after_metadata_hook_is_registered_for_test` (`:206`).

The actuation seams stay: they are the fault-injection mechanism this change's
failure-path verification depends on.

### A.9 Test seams that stay (task 7.5)

No inspection function retires in the replace/undo half: slot 1 already retired
all eight into `evidence.rs`. The **five** remaining `_for_test` functions —
`clear_undo_backup_for_test`, `reserve_undo_backup_generation_for_test`,
`set_persisted_undo_backup_for_generation_for_test`,
`begin_replace_transaction_for_test`, `finish_replace_transaction_for_test` —
are actuation seams driving steps otherwise reachable only through a worker
completion or the transaction gate, and they stay per the programme-level
deferral in `docs/next/workflow-readability.md` section 7.

### A.10 Premise correction — task 4.4's `retire_undo_backup_off_main` claim

**FLAGGED LOUDLY: the authoring premise is wrong.** Task 4.4 states that
`retire_undo_backup_off_main` has a "name [that] promises off-main retirement
while its body drops the value synchronously on the calling thread". The body is:

```rust
fn retire_undo_backup_off_main(retired: Option<Arc<super::GuardedReplaceUndoBackup>>) {
    let Some(retired) = retired else { return };
    drop(retired);
}
```

That `drop` releases an `Arc<DisposalOwned<ReplaceUndoBackup>>`. When it is the
last reference, `DisposalOwned::drop`
(`crates/lushtext-core/src/ui/plain_disposal.rs:609-660`) does **not** destroy
the payload inline: it moves the value into a `DisposalJob` and submits it to the
disposal lane, so the document-sized destruction runs on a disposal worker. The
inline-`drop(value)` branch is taken only when the owner carries neither a permit
nor a retained lane, which no undo journal does — every one is created through
`reservation.own(backup)` or `try_own_for_gtk`. **The name's "off_main" claim is
therefore accurate, and changing the behavior to match the name (which task 4.4
correctly rules out of scope) was never needed.**

Implemented as the closest faithful reading of the task: the function is still
**renamed**, because two other things about the name are genuinely wrong, and the
rename is still a pure readability fix with no runtime change.

- `retire_` now collides with the `retirement` bounded coordination role, which
  in this directory means `ui/search_panel/retirement.rs`'s bounded result
  disposal. This function is not that.
- `_off_main` attributes to this function a guarantee that belongs to the
  guarded owner it releases. The function's own job is "release this panel
  reference"; off-main destruction is `DisposalOwned`'s contract.

New name: `release_superseded_undo_journal`, with a doc comment recording exactly
where the off-main destruction comes from so the next reader does not have to
re-derive it. No behavior change.

### A.13 Task 5.4's cap-boundary clause — where it applies (finding F12)

Task 5.4 requires co-located unit tests for each moved decision "including each
cap boundary (at, one under, one over), a saturating case, and each
generation-mismatch rejection independently". Recorded honestly, because the
clause applies unevenly across the two halves of this change's policy work:

- **The `services/search_backup.rs` half has real cap boundaries, and they are
  tested at, under, and over.** `entry_count_exceeds_cap` is asserted at 0, at
  `JOURNAL_SCAN_MAX_ENTRIES - 1`, at the cap, and at one over;
  `retained_weight_exceeds_cap` at 0, at `MAX_REPLACE_UNDO_RETAINED_BYTES`, and at
  one over; `payload_budget_exceeded` at exactly the cap, under it, one over, and
  with a zero cap.
- **The `ui/search_panel/policy.rs` half has no cap comparison to test, so the
  clause is vacuous there rather than skipped.** `plan_undo_reservation` does not
  compare anything against a ceiling: it decides *whether* a reservation replaces
  guarded owners and *what weight* it credits back, and the ceiling
  (`MAX_REPLACE_UNDO_RETAINED_BYTES`) is applied by the caller in `journal.rs`
  when it picks between `try_reserve_replacement_for_gtk` and
  `try_reserve_for_gtk`. Keeping the ceiling out of the pure function was
  deliberate — it is what lets the policy stay free of a `services` dependency —
  and the consequence is that there is no boundary in it to sit at, under, or
  over. What the boundary clause's *intent* maps onto here is the
  distinguishability of the admission decisions, which is tested: `Some(0)` (a
  guarded owner that measures nothing) is asserted to be a `Replacement`, not
  `Fresh`, which is the one boundary the function actually has.
- **Saturating cases are tested on both halves**: `retained_byte_weight` at
  `usize::MAX`, `preview_reservation_weight` on a `usize::MAX` budget,
  `plan_undo_reservation` with combined weight saturating at `u64::MAX`, and
  `payload_budget_exceeded` with a saturating accumulated total.
- **Generation-mismatch rejections are tested independently**:
  `journal_generation_is_current` is asserted for equal, one-above, one-below,
  zero-equal, and both sides of a `u32` wrap.

### A.11 Live run record (task 10.10)

**Deviation from the literal target, and why.** Task 10.10 names `make run`. On
this maintainer's machine that launches against their real
`$XDG_DATA_HOME/lushtext/workspaces.json`, so the app would restore **their own
workspace folders** — and Replace All rewrites files. The task's own guard
("Replace All mutates files: it must never be pointed at the maintainer's real
workspace folders") therefore rules out the literal command, and there is no
automation action to add a workspace folder, so the fixture cannot be introduced
after launch. Per the slot-1 precedent, exactly what was run is recorded here.

**What was run.** The freshly built debug binary
(`cargo build && ./target/debug/lushtext`) on the maintainer's **live GNOME
Wayland session** — a real GUI window on the real compositor, not headless — with
`LUSHTEXT_DATA_DIR`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, and `XDG_STATE_HOME`
pointed at a throwaway `/tmp` tree. That tree was seeded with a
`workspaces.json` naming one throwaway fixture folder holding
`first.txt` (`alpha needle beta\nplain line\n`), `second.txt`
(`needle only\n`), and `third.txt` (`no match here\n`, a non-matching control).
The maintainer's own app data and workspace folders were never read or written,
and no dev desktop entry was staged.

**What was exercised, through the real D-Bus automation actions.**

| Step | Observed |
| --- | --- |
| workspace restore | `workspace.scope_workspace_name = "Replace Fixture"`, `folder_count = 1` |
| `set-search-panel-visible true` + `set-search-panel-query "needle"` | real streaming search: `match_count = 2`, `file_count = 2` |
| `set-search-panel-replace-query "thread"` + `preview-search-panel-replacements` | `replace_preview_mode = true`, `replace_preview_count = 2`, `checked_replacement_count = 2` |
| `confirm-search-panel-replacements` | **fixture files really rewritten**: `alpha thread beta`, `thread only`; control file untouched. Status bar: `Replaced 2 of 2 matches in 2 files`. Durable journal on disk: two entry files plus `manifest.json`. `has_undo_backup = true` |
| `undo-search-panel-replacements` | **byte-exact restoration confirmed by `sha256sum -c`** against pre-replacement digests: all three files `OK`. Status bar: `Reverted 2 files`. Journal directory removed. `has_undo_backup = false` |
| `undo-search-panel-replacements` again | refused: digests still `OK`, no second write |

**stderr.** One line for the whole session:
`WARNING: radv is not a conformant Vulkan implementation, testing use only.`
That is the host Mesa/Vulkan driver banner, not an app message. Zero
`Gtk-WARNING`, `Gtk-CRITICAL`, `GLib-GObject-WARNING`, pixman `*** BUG ***`, and
`Trying to measure` output. Log preserved in the session scratchpad as
`live-run-stderr.log`.

**What remains uncovered by this substitution.** Only the app-data profile: the
run did not exercise restoring the maintainer's real `workspaces.json`, session,
or drafts. Everything the task asks about — a real GUI process on a real
compositor performing a real workspace search, a real Replace All that mutates
files, a real undo that restores them byte-exactly, and a clean stderr — was
covered.

### A.12 Data-safety passes (tasks 0.4, 10.9)

Both passes were run with the `data-safety` skill in explicit mode, dispatching
the leaf domain reviewers in the contract's batch order. Only two of the five
domains trigger on this diff surface; the other three (`draft-integrity`,
`close-flow`, `restore-lifecycle`) match no suffix and no content hint here.

**Pass 1 — before writing code (task 0.4).** Over the intended diff surface:
`services/search_backup.rs`, `services/content_search/replace.rs`,
`ui/search_panel/replace.rs`, `ui/search_panel/mod.rs`,
`ui/search_panel/imp.rs`, `ui/window/search.rs`.

- `atomic-write`: **CLEAN.** All six patterns resolved SAFE with no unresolved
  branches. Persistent writes all route through
  `filesystem::write::atomic_replace` / `atomic_replace_stream` via
  `recovery_metadata::save_enveloped_json_path`; no production
  `std::thread::spawn`; `SlotGuard` survives worker panic and is released only
  after the GTK callback consumes the result; the full probe-write-flush-metadata-sync-rename-syncparent
  order is present; temp paths carry pid plus a process-local counter in the
  destination directory; coordination keys the stable resolved target, not the
  destination inode.
- `replace-safety`: **two confirmed findings**, both pre-existing.
  - **RS-3/RS-1, HIGH.** `search_backup::save` is delete-then-rebuild, and its
    only production caller was the partial-undo remainder path — whose
    unrestored files still hold Replace All output. A crash or a save error
    inside the rebuild window left them with no durable rollback copy, which is
    the exact in-memory-only failure the journal exists to prevent. The error
    was only `tracing::error!`-logged.
  - **RS-4, MEDIUM.** `rollback_applied_files` rewrote `original_bytes`
    unconditionally, and its per-file `TargetWriteGuard` is released as the loop
    advances, so an editor save landing mid-rollback could be discarded — after
    which a zero-error rollback deletes the journal.

**Both were fixed in this change**, per `.agents/rules/preexisting-blockers.md`
("fix it in the same work stream instead of ... treating it as out of scope"),
which overrides this change's own "no behavior change" non-goal. The precedent is
slot 1, which fixed the pre-existing result-cap defect while proving behavior
equivalence.

Independently verified before fixing: **the RS-4 rollback path is unreachable in
production today.** The only production caller of
`apply_replacements_if_current` (`ui/window/search.rs`) constructs a fresh
`AtomicBool::new(false)` that nothing ever sets, so `cancelled` cannot become
true and `rollback_applied_files` runs only from tests. That is why hardening it
carries no behavior-equivalence risk — and why it was still worth doing, since
the hazard becomes live the moment a cancelling caller is added.

**Pass 2 — over the actual diff (task 10.9).** Same reviewer, re-run against the
landed change including the split into `journal.rs` and `replace_execution.rs`.

- **FIX 1: closed.** `shrink_journal_to` never leaves the journal inactive.
  Interrupted before the new manifest lands, the old superset manifest is still
  active and undo re-validates each file's bytes, so an already-restored entry is
  recognised and never rewritten; interrupted after, the smaller manifest is
  active and the leftovers are orphans, which recovery reports without
  deactivating (`active` is computed before orphan diagnostics are appended). All
  three fallback triggers are states in which the on-disk journal was already
  unusable before the rewrite began.
- **FIX 2: closed.** `rollback_file_disposition` runs while the current path's
  `TargetWriteGuard` is still held, so classify-then-write is not a TOCTOU gap.
  Every unverifiable or changed target records an error, and
  `BoundedDiagnosticSample::record` increments `total_count` before any sampling
  cap, so the journal cannot be deleted on a silently-zeroed count.
  `AlreadyOriginal` correctly writes nothing and records no error.
- **No new flags. No ordering changes.** All eight safety-ordered sequences the
  first pass identified survive the split unchanged, including the pending claim
  before the generation reservation, the outcome restore on the already-claimed
  early return, capacity-before-supersede in the window apply path, exactly one
  `show_undo_button` per undo failure exit, the Acquire-ordered generation
  compare before any install or clear, and the service's
  `save_entry` -> `mark_incremental_journal_active` -> `atomic_write` per-file
  order with no journal deletion on `AfterRename`.
- **No unresolved candidates.** RS-2 remains as calibrated.

One residual the reviewer raised without flagging was fixed anyway: the
durability-failure message reached the status bar as `Info`, because the search
panel's message callback carried no severity. It now carries one, matching the
sidebar and workspace-section siblings, and the journal publishes the failure as
`Warning`.
