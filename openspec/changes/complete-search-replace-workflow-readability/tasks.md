## 0. Gates and orientation

- [ ] 0.1 **Two-proof gate — blocking.** Slot 1 is already archived at
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
- [ ] 0.2 Read `docs/next/workflow-readability.md` (status line, "Slot 1 residue",
      slot ledger, deferred work) and `docs/workflow-readability-matrix.md` rows
      `WFR-SEARCH-REPLACE`, `WFR-AUTOMATION-SPINE`, `WFR-BUFFER-REPLACEMENT`,
      plus the `Settled Conventions`, `Facade size budget`,
      `Migrated Workflow Roles`, `Policy Module Census`, `Seam Value Objects`, and
      `Completion Rule` sections. Read all five capability specs. Read slot 2a's
      task 12.2 handoff notes: they record convention friction this change
      inherits.
- [ ] 0.3 Read the code before changing it and write the current ordered stages
      into this file: `ui/search_panel/replace.rs`, `ui/search_panel/mod.rs`'s
      Replace All narration, `ui/window/search.rs`'s replace and undo path,
      `services/search_backup.rs`, and the write path in
      `services/content_search/replace.rs`. Name every inversion and its
      resumption point.
- [ ] 0.4 Invoke the `data-safety` skill in explicit mode over the intended diff
      surface before writing code, and again in task 8 over the actual diff.
      Record both.
- [ ] 0.5 Re-verify this change's premises against the tree. The authoring
      inventory found that two residue items are not what the residue text says
      (tasks 1 and 2). Confirm that is still true, and record any drift here
      instead of working from stale findings.

## 1. Retire the `activate_undo_replacements` residue item with evidence

- [ ] 1.1 Confirm and record that `LushtextSearchPanel::activate_undo_replacements`
      in `ui/search_panel/mod.rs` is already a documented one-line delegation to
      `replace::hand_back_undo_backup`, reading no transaction state and mutating
      no widget — i.e. slot 1's result-cap fix already discharged the residue item
      as written. Quote the function in this file as the evidence.
- [ ] 1.2 Confirm the delegate itself is correctly placed: `hand_back_undo_backup`
      refuses while the apply transaction is claimed, takes the published backup,
      retracts the affordance so the same journal cannot be handed back twice, and
      invokes the window callback. Record the transaction state it reads and the
      widgets it mutates, and confirm all of it belongs to the coordination role
      rather than the facade.
- [ ] 1.3 Fix the asymmetry that **does** remain, one layer out. In
      `ui/window/search.rs`, the undo path re-reads and re-mutates panel state
      inline before spawning the undo worker — claiming the replace transaction,
      showing the undo button on both early-return paths, reserving undo capacity
      — and installs the remainder backup inline afterwards. Give the panel one
      named operation per step so the window delegates the way the panel's own
      stage 4 does, and keep the transaction and generation guards on the panel
      side where they already live.
- [ ] 1.4 Update the residue section of `docs/next/workflow-readability.md`: mark
      the `activate_undo_replacements` item discharged, and state that the real
      residual asymmetry was on the window side and was fixed here. A future
      session must not re-open an item that is done, nor conclude the window-side
      work was skipped.

## 2. Settle `model/workspace_search.rs` — the decision is that it stays

- [ ] 2.1 Re-derive the full reference set rather than trusting the matrix cell,
      which records "2 consumers, both search". Expect at least:
      `services/content_search/search.rs` (five imported types),
      `ui/search_panel/execution.rs` (the traversal plan),
      `model/content_search.rs` (two types embedded in public enum variants), and
      `crates/lushtext-core/benches/benchmarks.rs` (two types addressed
      directly). Also check `crates/lushtext-core/tests/workspace_terminology.rs`,
      which names the file path literally.
- [ ] 2.2 Record the decision: **it stays in `model/`.** A service and a `model/`
      sibling both depend on it, so relocating it under `ui/` would invert
      dependency direction (`services -> ui`), which the convention forbids
      outright. It is already pure (no GTK-family import), already mutation-scoped
      through `model/**`, and already carries seven co-located unit tests, so the
      relocation would trade a dependency-direction violation for nothing.
- [ ] 2.3 Correct the matrix. Move the row out of "Additional single-workflow
      modules the census found" and into "Modules confirmed as domain and staying
      in `model/`", with the corrected consumer list and the dependency-direction
      reason. Keep a short note at the old location pointing at the resolution, so
      a reader following the census snapshot does not conclude the decision is
      still open.
- [ ] 2.4 Confirm no `.agents/rules/*.md`, skill, or doc asserts that this module
      is pending relocation, and fix any that does.

## 3. Amend the bounded coordination role set

- [ ] 3.1 Record, from the code, the two cohesive halves of `replace.rs` **and the
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
- [ ] 3.2 Confirm the naming situation before amending: `ui/search_panel/` already
      spends `execution.rs` on the streaming search stage order and
      `retirement.rs` on bounded disposal, for the same workflow, so the Replace
      All stage order's two modules cannot take either name. The *collision* is
      already resolved by the stage-order qualification rule slot 2a added. What is
      still missing is a name for the journal half's job — maintaining a durable,
      generation-guarded record a later stage reads back, with startup recovery —
      which none of `admission`, `execution`, `retirement`, or `watch` describes and
      which `retirement` actively contradicts.
- [ ] 3.3 Apply this change's `gtk-adapter-module-boundaries` delta: add `journal`
      to the bounded set with the scenario distinguishing it from `retirement` and
      `execution`. That is the delta's only content — do not re-add the
      qualification rule (slot 2a owns it) and do not widen the set further.
- [ ] 3.4 Retroactive-amendment obligation: re-check **every** row the matrix
      marks `migrated` against the amended role set and record the result per row.
      At this point that is `WFR-SEARCH-REPLACE` and `WFR-COMMAND-PALETTE`. Adding
      a role name does not invalidate an existing correct name, so this is expected
      to be a confirmation rather than a rename; if a row's declared name is
      genuinely wrong under the amended set, fix it here, because two generations
      of the convention must not coexist in the tree.
- [ ] 3.5 Update `.agents/rules/rust.md`'s bounded role-name list, the matrix's
      "Role file names" table, and any skill that enumerates the role names, so no
      standing guidance still lists the pre-amendment set. Run
      `make check-agent-docs` and `make check-agent-skills`.

## 4. Split `replace.rs` into role-named coordination modules

- [ ] 4.1 Split along the preview / journal seam established in task 3.1. The
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
- [ ] 4.2 Name both modules from the amended bounded set, using the stage-order
      qualification rule where a bounded name is already spent. Record the chosen
      names and the reasoning in the matrix row's notes.
- [ ] 4.3 Keep the crate-visible seam stable: the operation the window uses to
      convert a disposal reservation plus a raw backup into a guarded undo backup
      is re-exported from the facade today and must keep working from its new
      home, with an intent-first name.
- [ ] 4.4 Rename cross-module operations for intent as part of the split, and give
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
- [ ] 4.5 Keep mechanism names only on private helpers inside the module that owns
      the mechanism.
- [ ] 4.6 Confirm the split changed no behavior: no reordering of the transaction
      claim relative to the capacity reservation, no change to when the undo
      affordance appears or disappears, no change to which early-return path
      restores which state, and no change to the order in which the three shared
      fields from task 3.1 are read and written.

## 5. Extract the durable half's pure policy

- [ ] 5.1 Capture the mutation baseline **first**. `ui/search_panel/replace.rs` is
      **not** in the mutation scope today, so the pure logic about to move
      generates zero mutants. Record that explicitly as the baseline, and note the
      asymmetry: for logic moving from an unscoped file into a scoped
      `policy.rs`, parity means **gaining** mutants that are all killed, which is
      strictly stronger than the requirement's equal-counts phrasing. The
      equal-counts clause governs relocations between two scoped locations.
- [ ] 5.2 Move into `ui/search_panel/policy.rs`: the preview reservation weight
      and the completed-outcome shrink-to weight, the retained-byte saturating
      cast, the undo-capacity admission arithmetic currently inline in the undo
      reservation (which must account for the installed backup plus the transient
      input weight against the retained-bytes ceiling), and the generation-match
      predicates inline in the generation-guarded install and clear.
- [ ] 5.3 Every moved function must take primitives or plain value objects, never
      `&self` on a GObject, so the module keeps zero `gtk4`, `glib`, `gio`,
      `libadwaita`, or `sourceview5` imports. `make check-workflow-boundaries`
      enforces this and names the file and import on failure.
- [ ] 5.4 Add co-located `#[cfg(test)]` unit tests for each moved decision,
      including each cap boundary (at, one under, one over), a saturating case, and
      each generation-mismatch rejection independently.
- [ ] 5.5 Name any policy literal this module now owns as a typed constant beside
      its decision, per the literal-ownership rule in `.agents/rules/rust.md`.
- [ ] 5.6 Prove the outcome: run the focused scoped mutation run against
      `ui/search_panel/policy.rs` using the two-part scoping documented in
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`
      (`MUTANTS_RE` plus a `MUTANTS_EXCLUDE` glob, because `MUTANTS_RE` alone does
      not filter the `delete field` mutant kind), and confirm every newly generated
      mutant is caught or unviable with a stated reason. Note that this module
      already carries the slot-1 population, so report the pre-existing and newly
      added mutants separately.

## 6. Decide `services/search_backup.rs`'s buried policy

- [ ] 6.1 Enumerate the rules buried in its three large loaders and confirm the
      duplication before deciding: the journal-activation decision (twice, once
      per loader shape), the per-entry payload budget arm (twice), the manifest
      entry-count cap check (three times), the manifest/marker/`.json`
      payload-file filter (twice), manifest entry dedup by entry file and by
      target path, the retained-weight admission, and the cleanup-replacement
      eligibility rule.
- [ ] 6.2 Record the placement decision with its reason. The constraint is that
      these rules **cannot** move to `ui/search_panel/policy.rs`: a service must
      not depend on the adapter. The real choice is between a
      `services/search_backup/policy.rs` and private pure functions with direct
      unit tests in place. Both are already inside the `services/**` mutation
      scope, so this is a testability and de-duplication decision, not a coverage
      one — say so, and do not claim a mutation-coverage benefit that does not
      exist.
- [ ] 6.3 Whichever placement is chosen, de-duplicate each rule to one
      implementation and give it direct unit tests that do not need a tempdir:
      activation with and without diagnostics and with a manifest/entry count
      mismatch; the payload budget at and over the cap; the cap check at and over
      the entry limit; the payload-file filter accepting an entry and rejecting the
      manifest, the cleanup marker, and a non-`.json` file; dedup rejecting a
      duplicate entry file and a duplicate target path; and cleanup eligibility
      refusing when any diagnostic disallows replacement.
- [ ] 6.4 Do not change any on-disk shape, file name, manifest field, cap value,
      diagnostic reason, or activation outcome. This is a restructuring of where
      the decisions live, and every existing `search_backup` test must pass
      unchanged.
- [ ] 6.5 Classify the seam surface on the write side for the record:
      `services/content_search/replace.rs` carries configuration (a byte-cap
      thread-local plus its setter), probe (the active-journal assertion), and
      actuation (the after-metadata hook registry and the before-rename
      fault-injection guard) seams, plus three inspection functions over those
      fault-injection registries. Record the classification in the matrix row and
      leave the actuation seams alone: they are the fault-injection mechanism this
      change's failure-path verification depends on.

## 7. Extend the replace/undo evidence surface

- [ ] 7.1 Add the fields the durable half needs, at minimum: the apply
      transaction's pending flag as its **own** field rather than folded into
      preview-pending; the transaction generation; the undo-backup generation; the
      preview generation; the preview capacity-retry armed state (the undo twin is
      already exposed); the installed backup's entry count and retained weight; an
      in-flight journal disk-job counter analogous to the existing preview
      selection-job counter; and the last apply result's replaced, skipped, and
      error counts.
- [ ] 7.2 Respect the reentrancy constraint the surface's own module doc records:
      the accessor takes shared borrows of the queued preview request and the undo
      backup, so no new field may be read from inside a `borrow_mut()`. Prove it
      with a test that reads evidence at each point where the workflow holds a
      mutable borrow today.
- [ ] 7.3 Confirm reading the extended surface still mutates nothing — no timer,
      queue, generation counter, coordinator, or disposal reservation — and still
      does not require the workflow to be in a particular stage.
- [ ] 7.4 Migrate the widget tests that reach around the widget into the
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
- [ ] 7.5 Expect **no** inspection functions to retire in `replace.rs`: slot 1
      already retired all eight into `evidence.rs`, and the **five** remaining
      `_for_test` functions there (`clear_undo_backup_for_test`,
      `reserve_undo_backup_generation_for_test`,
      `set_persisted_undo_backup_for_generation_for_test`,
      `begin_replace_transaction_for_test`, `finish_replace_transaction_for_test`)
      are actuation seams that stay per the programme-level deferral. Record that
      so this task is not read as incomplete. Confirm the project test count does
      not decrease.
- [ ] 7.6 Confirm the workflow's test-only timing and limit overrides all still
      live in the single `test_policy.rs` value, and that any override this change
      needs is added there rather than as a new module-level static.

## 8. Automation: prove no widening

- [ ] 8.1 Confirm every documented `window.content_search` field except `visible`
      still projects from the evidence surface, and that `visible` is still window
      shell state. Slot 1 established this; this change must not regress it while
      the surface grows.
- [ ] 8.2 Confirm the new evidence fields from task 7 — generations, disk-job
      counts, backup entry counts and weights, last-apply detail — are **not**
      serialized into the snapshot. The evidence surface is internal; only
      documented contract fields cross the D-Bus boundary, and existing redaction
      and omission behavior for private state is preserved.
- [ ] 8.3 Confirm the `workspace-search` and `replace-preview` readiness blockers
      keep their documented semantics.
- [ ] 8.4 Keep the evidence-field drift check slot 2a implemented in
      `scripts/check-automation-docs.py` correct for the extended surface: update
      the projection paragraph and the evidence-field-to-snapshot-field mapping in
      `docs/automation-reference.md` that the check reads, and confirm the check
      classifies each field added in task 7 correctly — projected fields must be
      documented, and internal-only fields (generations, disk-job counts, backup
      weights) must be ignored rather than demanded. If the check does not exercise
      the new fields at all, its mapping is stale and this task is not done. Run
      `make check-automation-docs` and `make automation-client-self-test`.
- [ ] 8.5 Prove it rather than assert it: capture an Automation1 snapshot for the
      same app state before and after, and diff the `content_search` object and
      the readiness fields to zero differences.

## 9. Facade and matrix completion

- [ ] 9.1 Update `ui/search_panel/mod.rs`'s Replace All stage narration and role
      table for the new module names, keeping both stage orders, all inversions,
      and their named resumption points. The facade must still own no timer,
      generation counter, admission bookkeeping, or widget mutation beyond the
      entry-point surface its module doc carves out.
- [ ] 9.2 Measure the facade and confirm it is within the 370-line budget slot 2a
      declared. The starting point is 350, so the working headroom is 20 lines and
      the Replace All narration is about to grow by at least two module names.
      **At, say, 372 lines the answer is to delegate more, not to amend the
      budget**: fold narration detail into the coordination modules' own module
      docs and keep the facade's stage list to intent plus delegate plus resumption
      point. Raising the number is a convention amendment that would require
      re-migrating every migrated row in this change, and by now that is two
      workflows. If delegation genuinely cannot fit the narration, stop and escalate
      rather than quietly editing the budget line.
- [ ] 9.3 Update the matrix's `### WFR-SEARCH-REPLACE` subsection under
      `Migrated Workflow Roles`: the new coordination module names, the policy and
      evidence paths, and a `mutation parity` pointer to this change's evidence
      file. Remove the note saying `replace.rs` keeps a workflow-descriptive name
      pending slot 2, and replace it with what was decided.
- [ ] 9.4 Update the row's cells: `Owned pure policy` for the durable half's
      relocated decisions, `Seams (i/c/a/p)` for the post-migration reality,
      `Evidence surface` for the extended surface, `Risk` to record that the
      tier-3 half is now covered, and `Slot` to `1 (search/preview half) + 2b
      (replace/undo half)`.
- [ ] 9.5 Update the row's `Post-migration note` in "Workflow Stage Traces" so the
      Replace All trace names the current operations and modules, including the
      post-split owner of each of the three shared fields from task 3.1.
- [ ] 9.6 Write the mutation parity evidence to
      `openspec/changes/complete-search-replace-workflow-readability/evidence/mutation-parity-replace-policy.md`,
      following the structure of
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`:
      scope re-verification with the exact commands, the before/after table,
      per-survivor disposition, the pre-existing slot-1 population reported
      separately from this change's additions, and the
      out-of-scope-to-in-scope asymmetry from task 5.1 stated plainly.
- [ ] 9.7 Advance `docs/next/workflow-readability.md`: flip slot 2b's ledger line
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
- [ ] 9.8 Update the baseline section of the record with this change's
      contribution, reporting **seams reified** as the primary unit and stating
      which long-signature definition (receiver-counted 88 or strict 43) any
      secondary figure uses.
- [ ] 9.9 Update `AGENTS.md`, `README.md`, and any `.agents/rules/*.md` or
      `.agents/skills/**` reference naming a path this change moved.

## 10. Verification

- [ ] 10.1 `make check` and `make check-policy` clean, including
      `check-workflow-boundaries` (facade budget, role completeness, ledger
      agreement), `check-filesystem-boundary`, `check-automation-docs`,
      `check-accessibility-policy`, and `check-visual-proof-policy`.
- [ ] 10.2 `make test` and `make test-widget-headless` clean with **zero `FLAKY:`
      lines**. A recovered flake is a blocker per
      `.agents/rules/preexisting-blockers.md`. Record test counts before and after
      and confirm the total did not decrease.
- [ ] 10.3 `make mutants-diff` clean, with the task 9.6 evidence attached and
      survivors closed by added tests rather than scope changes. If the wrapper's
      `git diff origin/main...` cannot see working-tree edits, use the explicit
      merge-base diff workaround recorded in
      `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`
      and record both runs.
- [ ] 10.4 The mandatory proof lanes for `ui/` and widget-test changes, each from a
      clean artifact root: `make visual-geometry-smoke`,
      `make accessibility-smoke`, `make visual-smoke`.
- [ ] 10.5 **Replace All and undo behavior equivalence**, each case with a widget
      test asserting user-visible and on-disk outcomes: no matches; one match in
      one file; many matches across many files; a partial check where only some
      rows are selected and only those are written; a preview superseded by a
      newer query, whose stale completion must publish nothing and retire its
      payload; a Replace All followed by undo restoring byte-exact original
      content; and a second undo attempt that must be refused rather than
      double-applied.
- [ ] 10.6 **Journal failure-path equivalence**, using the existing
      fault-injection seams in `services/content_search/replace.rs`: a
      before-rename failure must classify as "previous bytes intact" and leave the
      document reported as unwritten; an after-rename durability failure must
      surface as durability-unconfirmed rather than a generic lost save; and an
      after-metadata hook must fire at the same point as before this change.
- [ ] 10.7 **Startup recovery equivalence** across the journal's real states: a
      healthy journal that activates; a journal with diagnostics that must not
      activate; a journal with a duplicate target path; one over the entry-count
      cap; one over the payload budget; one over the retained-memory cap; an empty
      manifest; a cleanup-in-progress marker taking precedence; and a missing
      manifest. Each must produce the same activation outcome, the same
      diagnostics, and the same cleanup behavior as before.
- [ ] 10.8 `make crash-recovery-smoke` clean, since the undo journal is
      recovery-relevant durable state.
- [ ] 10.9 Re-run the `data-safety` skill in explicit mode over the actual diff and
      resolve every confirmed finding. A tier-3 change does not close with an open
      data-safety finding.
- [ ] 10.10 **Live run:** `make run` against throwaway fixture workspace folders,
      performing a real workspace search, a real Replace All, and a real undo.
      Confirm the fixture files are rewritten and then restored byte-exactly, and
      that stderr has no new `Gtk-WARNING`, `Gtk-CRITICAL`,
      `GLib-GObject-WARNING`, pixman `*** BUG ***`, or `Trying to measure`
      output. Replace All mutates files: it must never be pointed at the
      maintainer's real workspace folders. Per the slot-1 precedent, if the
      maintainer's environment makes the literal target unsafe, record exactly what
      was run instead and what remains uncovered rather than silently substituting
      a headless run.
- [ ] 10.11 Cold-read verification: with this change's conversation set aside, read
      only the facade and confirm both stage orders and every inversion are
      followable without opening the coordination or policy modules, and that a
      reader can tell where the durable write and the undo journal live. If not,
      the split in task 4 is wrong and must be revisited before archiving.
- [ ] 10.12 `openspec validate complete-search-replace-workflow-readability
      --strict` clean.

## 11. Handoff

- [ ] 11.1 Confirm the programme record and the matrix agree that
      `WFR-SEARCH-REPLACE` is complete, that slots 1, 2a, and 2b are complete, and
      that slot 3 is the next outstanding slot with `WFR-AUTOMATION-SPINE` carried
      onto its line.
- [ ] 11.2 Record convention friction for slots 3 through 7: whether the amended
      role set was sufficient, whether the stage-order qualification rule read
      well in practice, whether the 370-line budget held on a facade narrating two
      stage orders including a durable write, and whether the evidence surface's
      reentrancy constraint needs to become a stated convention rather than a
      per-workflow module note. Three workflows are migrated after this change, so
      the retroactive-amendment rule is now materially more expensive than it was
      at slot 2.
