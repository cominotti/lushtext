## 1. Census: enumerate every workflow before migrating any

- [x] 1.1 Derive the workflow inventory from `crates/lushtext-core/src/ui/**`, the
      coordination modules (`*_runtime.rs`, `plain_disposal.rs`,
      `buffer_snapshot.rs`, `services/single_flight.rs`, `services/sync.rs`,
      `services/palette/charge_scope.rs`), and the pure policy modules currently in
      `model/`. Produce one candidate row per workflow, not per widget.
- [x] 1.2 For each candidate workflow, record its current file set with line counts,
      its entry points, and the ordered stages a reader must follow to trace it.
- [x] 1.3 For each pure policy module in `model/`, record its exact consumer list and
      classify it single-consumer or cross-cutting. Confirm the eight known modules
      (`save_admission`, `search_flight`, `search_retirement`, `plain_disposal`,
      `migration_ledger`, `editor_memory`, `buffer_replacement`,
      `minimap_analysis`) and find any missed.
- [x] 1.4 For each workflow, enumerate its feature-gated test seams and classify each
      as inspection, configuration, actuation, or lifecycle probe. Record counts per
      kind per workflow; the totals must reconcile with the current
      `#[cfg(feature = "test-utils")]` census.
- [x] 1.5 For each workflow, identify the field bundles that cross two or more
      function boundaries and name the value object each one should become. A row
      whose seam value object cannot be named is not complete.
- [x] 1.6 Resolve the three known outliers explicitly: `ui/editor_page/minimap.rs`
      (size and pixel-verified geometry), `model/editor_memory.rs` (five consumers),
      and `ui/markdown_preview/**` (already decomposed). Record each as conforming,
      exempt, or deferred with a stated reason.
- [x] 1.7 Decide and record whether `plain_disposal` is cross-cutting coordination
      that stays in place, or a future GTK Lush candidate that is out of scope for
      this programme (design.md open question).
- [x] 1.8 Assign each workflow a risk tier and a migration order position, honoring
      the rule that user-data workflows follow at least two lower-risk migrations.
- [x] 1.9 Write `docs/workflow-readability-matrix.md` with stable row ids, the fields
      above, and per-row migration status. Follow the anchor and status conventions
      already used by `docs/accessibility-matrix.md`.

## 2. Convention: settle the shape the census revealed

- [x] 2.1 Reconcile the two new capability specs against the census. If the census
      contradicts a requirement, amend the spec before any code moves.
- [x] 2.2 Settle the coordination role file name (fixed name versus bounded set) from
      what the census found across workflows, and record the decision in the
      capability spec (design.md open question).
- [x] 2.3 Settle whether the narrative facade gets a normative line budget, or
      whether the exemplar measures it first and a later change sets the number
      (design.md open question).
- [x] 2.4 Settle whether the residual sweep can assert zero
      `#[expect(clippy::too_many_arguments)]` in workflow code or needs a bounded
      allowlist for builder-style row constructors such as
      `ui/window/encoding.rs::append_choice_row` (design.md open question).

## 3. Global enablers: mutation scope and policy checking

- [x] 3.1 Add `crates/lushtext-core/src/ui/**/policy.rs` to `examine_globs` in
      `.cargo/mutants.toml` and document the convention beside it.
- [x] 3.2 Capture a baseline mutation run for the exemplar's current policy modules
      (`model/search_flight.rs`, `model/search_retirement.rs`) so relocation parity
      can be proved in section 5.
- [x] 3.3 Add a policy check script that fails when: a `policy.rs` module imports
      `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5`; a `policy.rs` exists at
      a path the mutation scope cannot reach; a workflow marked migrated in the
      matrix lacks its required roles; or a matrix row claims evidence that is
      absent.
- [x] 3.4 Wire the policy check into `make check-policy` and add a dedicated
      `make check-workflow-boundaries` target for focused local runs.
- [x] 3.5 Verify the new check fails on a deliberately broken fixture and passes on
      the current tree, so it cannot silently no-op.

## 4. Standing guidance revision

- [x] 4.1 Amend `.agents/rules/build.md:378-381` so the mutation scope rule
      distinguishes pure policy modules by convention (in scope) from GTK adapters
      (out of scope), instead of distinguishing by directory. Add the new
      `make check-workflow-boundaries` target and the matrix to the build-target
      documentation.
- [x] 4.2 Reframe the "Coordination Vocabulary" section of `.agents/rules/rust.md`
      as an implementation tier reached from a workflow, presented after the
      workflow/domain vocabulary rather than as the first thing to learn.
- [x] 4.3 Add to `.agents/rules/rust.md` the seam value-object rule (bundle crossing
      two or more boundaries), the prohibition on renaming a value while crossing a
      seam, and the `policy.rs` purity requirement.
- [x] 4.4 Update `.agents/rules/widget-wiring.md` so widget tests read workflow
      evidence surfaces instead of adding per-field `*_for_test` inspection
      functions, and so new inspection needs extend the evidence surface.
- [x] 4.5 Add `docs/workflow-readability-matrix.md` to the mandatory-update trigger
      list in `.agents/rules/documentation.md`.
- [x] 4.6 Update the `rust-hex-arch` skill to own the workflow module shape, role
      assignment, and policy co-location guidance.
- [x] 4.7 Update the `gtk-testing` skill to own the test-seam taxonomy and
      evidence-surface usage, including the deferred status of actuation seams.
- [x] 4.8 Update the `rust-comments` skill with the narrative-facade documentation
      expectation, including how to narrate inverted control flow.
- [ ] 4.9 Update the `gtk-perf-review` and `data-safety` skills for any policy paths
      that the exemplar relocates, and audit every maintained skill and rule for
      references to relocated paths.
- [x] 4.10 Update `AGENTS.md` module layout and key design decisions, and `README.md`
      architecture overview, for the workflow role convention.
- [x] 4.11 Run `make check-agent-docs` and `make check-agent-skills` and fix all
      findings.

## 5. Exemplar: migrate the search panel

- [ ] 5.1 Read `ui/search_panel/**` (`imp.rs` 841 lines, `replace.rs` 1,024 lines,
      `runtime.rs`, `item.rs`) plus `model/search_flight.rs` and
      `model/search_retirement.rs`, and write down the current ordered stages before
      changing anything.
- [ ] 5.2 Relocate `model/search_flight.rs` and `model/search_retirement.rs` into the
      search-panel workflow as pure policy, preserving purity and behavior.
- [ ] 5.3 Prove mutation parity for the relocated policy against the section 3.2
      baseline via `make mutants-diff`, and record the before/after generated and
      killed counts.
- [ ] 5.4 Introduce the search-panel seam value object identified in task 1.5,
      constructed once at the workflow entry point and validated as a unit. Remove
      the now-redundant loose parameters from the seams it covers.
- [ ] 5.5 Rename cross-module search-panel operations from mechanism names to
      workflow-intent names. Leave mechanism names on private helpers inside the
      coordination module.
- [ ] 5.6 Add the search-panel evidence surface exposing every field the panel's
      inspection seams exposed, and migrate the panel's widget tests to read it.
- [ ] 5.7 Remove the retired per-field `*_for_test` inspection functions for the
      search panel and confirm no callers remain. The project test count must not
      decrease.
- [ ] 5.8 Collapse the search panel's test-only timing and limit overrides into one
      per-workflow test policy value, and confirm no override storage compiles
      without the test feature.
- [ ] 5.9 Project the search-related automation snapshot fields from the new evidence
      surface, keeping the exported D-Bus fields, names, and semantics unchanged.
- [ ] 5.10 Write the narrative facade for the search panel and measure its resulting
      size as the input to task 2.3.
- [ ] 5.11 Update `docs/automation-reference.md` and `docs/automation.md` for the
      evidence projection, then run `make check-automation-docs`.
- [ ] 5.12 Mark the search-panel row migrated in the matrix with its evidence
      pointers.

## 6. Verification

- [ ] 6.1 `make check` and `make check-policy` clean, including the new workflow
      boundary check.
- [ ] 6.2 `make test` and `make test-widget-headless` clean, with no `FLAKY:` line.
      Investigate any flake as a blocker per `.agents/rules/preexisting-blockers.md`
      rather than accepting a retry pass.
- [ ] 6.3 `make mutants-diff` clean for the change, with the section 5.3 parity
      evidence attached.
- [ ] 6.4 `make check-automation-docs` and `make automation-client-self-test` clean.
- [ ] 6.5 Behavior equivalence for search and replace: run the search panel through
      no-results, single-result, many-results, capped-results, and constrained-width
      states plus a Replace All with undo, and confirm identical user-visible
      behavior and identical safety behavior.
- [ ] 6.6 Run the app via `make run`, exercise search, replace, and undo, and confirm
      stderr has no new GTK, GLib-GObject, or pixman warnings.
- [ ] 6.7 Confirm `model/` no longer contains the two relocated modules and that the
      matrix records the remaining `model/` policy modules as cross-cutting or
      pending.
- [ ] 6.8 Re-read the exemplar facade cold and confirm the workflow's stages are
      followable without opening the coordination or policy modules. If they are
      not, the shape is wrong and section 2 must be revisited before the change is
      archived.

## 7. Programme record and handoff

The goal of this section is that a session starting cold, months from now, with no
memory of this discussion, can answer in one read: what problem this solves, how
much is done, what is next, what is deferred and why. Do not rely on this change's
archived artifacts being found by search.

- [ ] 7.1 Write `docs/next/workflow-readability.md` as the programme record,
      following the posture-and-gates shape of `docs/next/gtk-lush.md` and
      `crates/gtk-lush/GOVERNANCE.md`. It MUST contain: the four measured problems
      with their numbers; the baseline quantification from design.md D10 (6% of
      `ui/` + `model/` migrated by this change, 2 of 8 policy modules, 48 of 639 test
      seams, 2 of 90 long signatures); the remaining-scope table; the sequencing
      rationale (why census-first, why vertical slices); the rejected alternatives
      (new policy layer, naming-only pass, horizontal slicing); the deferred work
      with what would justify taking it on; and a link to this change by name for
      full rationale.
- [ ] 7.2 Record in the programme record and the matrix the planned migration order
      and per-change scope: search/replace and palette, then save and load, then
      draft/recovery and session, then workspace tree and notes, then minimap, then
      the residual sweep. Note that migration changes are expected to need only
      proposal and tasks, and that needing a spec delta signals an incomplete
      contract.
- [ ] 7.3 Record the deferred work explicitly and separately from the planned work:
      actuation test seams (the missing workflow/dialog-presentation boundary) and
      state-machine reification of inverted drains, each with its reason and its
      justification bar.
- [ ] 7.4 Record the unblock point: the migration changes become authorable after
      sections 1 and 2 of this change, not after the exemplar. State what each needs
      from the census — value-object names, per-kind seam counts, risk tier.
- [ ] 7.5 Record the retroactive-amendment rule in the programme record and the
      matrix so a future migration cannot fork the convention without re-migrating
      earlier workflows.
- [ ] 7.6 Make the record discoverable from the surfaces a session loads
      automatically: add a pointer in `AGENTS.md` (architecture or key design
      decisions), a pointer from `docs/workflow-readability-matrix.md`, and a
      reference from the relevant `.agents/rules/*.md` entry revised in section 4.
      A future session MUST reach the programme record without knowing to search
      `docs/next/`.
- [ ] 7.7 Add `docs/next/` planned-work records and
      `docs/workflow-readability-matrix.md` to the mandatory-update trigger list in
      `.agents/rules/documentation.md`, so a later migration change is required to
      advance the record rather than leaving it stale. This closes the existing gap
      where `docs/next/` is the repo's planned-work convention but is not a
      documentation trigger.
- [ ] 7.8 Extend the section 3.3 policy check so it fails when the programme record
      claims a migration is complete but the matching matrix rows are not marked
      migrated, or when the record's remaining-scope table and the matrix disagree
      about which workflows are outstanding.
- [ ] 7.9 Cold-read verification: with this change's conversation and artifacts set
      aside, read only `AGENTS.md`, the revised rules, the matrix, and the programme
      record, and confirm the next change's scope and prerequisites are derivable. If
      they are not, the record is insufficient and section 7 is incomplete.
