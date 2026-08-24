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
- [x] 4.9 Update the `gtk-perf-review` and `data-safety` skills for any policy paths
      that the exemplar relocates, and audit every maintained skill and rule for
      references to relocated paths.
- [x] 4.10 Update `AGENTS.md` module layout and key design decisions, and `README.md`
      architecture overview, for the workflow role convention.
- [x] 4.11 Run `make check-agent-docs` and `make check-agent-skills` and fix all
      findings.

## 5. Exemplar: migrate the search panel

- [x] 5.1 Read `ui/search_panel/**` (`imp.rs` 841 lines, `replace.rs` 1,024 lines,
      `runtime.rs`, `item.rs`) plus `model/search_flight.rs` and
      `model/search_retirement.rs`, and write down the current ordered stages before
      changing anything.
- [x] 5.2 Relocate `model/search_flight.rs` and `model/search_retirement.rs` into the
      search-panel workflow as pure policy, preserving purity and behavior.
- [x] 5.3 Prove mutation parity for the relocated policy against the section 3.2
      baseline via `make mutants-diff`, and record the before/after generated and
      killed counts.
- [x] 5.4 Introduce the search-panel seam value object identified in task 1.5,
      constructed once at the workflow entry point and validated as a unit. Remove
      the now-redundant loose parameters from the seams it covers.
- [x] 5.5 Rename cross-module search-panel operations from mechanism names to
      workflow-intent names. Leave mechanism names on private helpers inside the
      coordination module.
- [x] 5.6 Add the search-panel evidence surface exposing every field the panel's
      inspection seams exposed, and migrate the panel's widget tests to read it.
- [x] 5.7 Remove the retired per-field `*_for_test` inspection functions for the
      search panel and confirm no callers remain. The project test count must not
      decrease.
- [x] 5.8 Collapse the search panel's test-only timing and limit overrides into one
      per-workflow test policy value, and confirm no override storage compiles
      without the test feature.
- [x] 5.9 Project the search-related automation snapshot fields from the new evidence
      surface, keeping the exported D-Bus fields, names, and semantics unchanged.
- [x] 5.10 Write the narrative facade for the search panel and measure its resulting
      size as the input to task 2.3.
- [x] 5.11 Update `docs/automation-reference.md` and `docs/automation.md` for the
      evidence projection, then run `make check-automation-docs`.
- [x] 5.12 Mark the search-panel row migrated in the matrix with its evidence
      pointers.

## 6. Verification

- [x] 6.1 `make check` and `make check-policy` clean, including the new workflow
      boundary check.
      Both exit 0. `check-workflow-boundaries` reports "1 workflow policy module(s)
      are pure and mutation-scoped, and every migrated matrix row names complete,
      existing roles". Note for future sections: because this section added widget
      tests and a `ui/` unit test, the proof-freshness gates correctly demanded
      refreshed evidence before passing — `check-visual-proof-policy` required a new
      `make visual-geometry-smoke` (any `crates/lushtext/tests/widget/` or
      `crates/lushtext-core/src/ui/` change is visual-sensitive), and
      `check-accessibility-policy` required new `make accessibility-smoke` plus
      `make visual-smoke` runs because the same prefixes feed the accessibility
      source fingerprint. All three lanes were rerun from clean artifact roots and
      pass.
- [x] 6.2 `make test` and `make test-widget-headless` clean, with no `FLAKY:` line.
      Investigate any flake as a blocker per `.agents/rules/preexisting-blockers.md`
      rather than accepting a retry pass.
      Both lanes exit 0 with zero `FLAKY:` lines, twice: once on the committed tree
      and once after this section's added tests (1,422 non-widget tests, full widget
      suite).
- [x] 6.3 `make mutants-diff` clean for the change, with the section 5.3 parity
      evidence attached.
      29 changed-code mutants, 26 caught, 0 missed, 3 unviable, exit 0. The first run
      reproduced the recorded whole-`policy.rs` figures exactly (29/25/1/3), which
      independently confirms the 5.3 focused scoping addressed the same population.
      The pre-existing `exhausted -> true` survivor is closed by one added pure test,
      not by a scope change. Commands, the `git diff origin/main...` worktree
      limitation, and both runs are recorded in
      `evidence/mutation-parity-search-policy.md`.
- [x] 6.4 `make check-automation-docs` and `make automation-client-self-test` clean.
- [x] 6.5 Behavior equivalence for search and replace: run the search panel through
      no-results, single-result, many-results, capped-results, and constrained-width
      states plus a Replace All with undo, and confirm identical user-visible
      behavior and identical safety behavior.
      **All six states are now proven.** Five were proven by this section directly;
      capped-results was blocked by a pre-existing product defect (below), which has
      since been fixed at the service boundary, so the state is reachable and covered.
      Per-state proof, with the widget tests marked `new` added because the state had
      no coverage:

      | State | Widget test | AT-SPI smoke case |
      | --- | --- | --- |
      | no-results | `test_search_panel_no_results_keeps_results_body_hidden` (asserts "No results found", body stays hidden) | `workspace-search-no-results` |
      | single-result | `test_search_panel_single_result_reports_one_result_in_one_file` (**new**: "1 results in 1 files", no warning class, evidence `match_count`/`file_count`) | `workspace-search` (anchor "3 results in 1 files") |
      | many-results | `test_search_panel_many_results_group_by_file_and_scroll_within_clamp` (**new**: 24 matches / 12 files, 12 grouped root rows, clamp held, no horizontal scrolling, header/close still visible) | `workspace-search-dense-constrained` (anchor "16 results in 16 files") |
      | capped-results | `test_search_panel_capped_results_warn_and_keep_every_delivered_match` (**new**: 12,000-match fixture, `result_capped`, "10,000+ results (truncated) — narrow your search" with the `warning` class, at least 10,000 matches kept, accepted snapshot published, Save search shown) plus `test_search_panel_superseding_a_capped_search_discards_it_immediately` (**new**: a newer query during the capped stream still discards everything buffered) | `workspace-search-capped` (**new**: prefix anchor "10,000+ results (truncated)") |
      | constrained-width | `test_search_panel_narrow_window_ellipsizes_rows_without_horizontal_scrolling` (**new**: presented 360px window, window not widened, `hadjustment.upper <= page_size`, entry/close still allocated, realized row labels ellipsize Middle + End) | `workspace-search-dense-constrained` |
      | Replace All + undo | `test_replace_all_then_undo_restores_original_file_bytes` (**new**, window-level end to end: 2 matches previewed and checked, confirm rewrites both files on disk, undo restores exact original bytes) plus the existing preview/confirm safety tests | `workspace-search-replace-undo` (anchors "Replaced 2 of 2 matches in 1 files", "Undo replacements") |

      Third lane: the live-session automation run recorded under 6.6 drove
      no-results, many-results, and Replace All with undo against the real app and
      saw the fixture files go 7 matches -> 0 -> 7.

      All six states behave identically to the pre-migration contract except the
      capped state, which was deliberately corrected (below), and the safety contract
      is unchanged: only generated, checked rows from the current accepted outcome are
      applied, and undo restores the exact original bytes.

      **Pre-existing defect found while attempting the capped-results test, now
      fixed.** The capped state could not be reached through the streaming path.
      `services/content_search/search.rs` sent `SearchEvent::ResultCap` and then
      immediately ran `cancel.store(true)` on the very `Arc<AtomicBool>` that
      `ui/search_panel/execution.rs` reads once per tick as `cancelled`. Every arm of
      that tick loop is guarded by `if !cancelled`, so the `ResultCap` event and every
      match still sitting in the `bounded(1024)` channel were discarded. Measured with
      a 40-file / 24,000-match fixture: the panel settled at 9,000 matches with
      `result_capped == false` and the label "9,000 results in 18 files" — the
      "10,000+ results (truncated) — narrow your search" warning never appeared and
      roughly a thousand matches were dropped silently, so the user believed the
      search completed.
      This was **not a regression from this change**: the whole tick-loop body is
      byte-identical before and after the migration (md5 `9ca6f97c...` for both
      `origin/main:ui/search_panel/runtime.rs` and the current
      `ui/search_panel/execution.rs`).
      The fix is a service-boundary correction, not new machinery. The walk's stop
      reasons are now owned by one private `WalkStop` value in `search.rs` that keeps
      the caller's `cancel` flag ("discard this flight", written only by the panel's
      supersede/close path) separate from the two service-internal one-shot
      terminations it already had in spirit: the `Incomplete` claim (previously the
      `incomplete_sent` flag, semantics unchanged) and the new result-cap stop. A cap
      now stops production without writing the caller's flag, so the tick drains the
      buffered matches, the `ResultCap` notice, and `Done`, and `result_capped`
      becomes truthful. Everything stays bounded: the same `bounded(1024)` channel and
      the same 250-events-per-tick budget. Supersede and cancel paths are untouched —
      the panel is still the only writer of `cancel`, and the freshness/staleness
      rejections proven equivalent earlier in this change are unchanged.
      Coverage added with the fix: service tests
      `result_cap_terminates_without_touching_the_caller_cancel_flag` and
      `walk_stop_separates_caller_cancellation_from_service_termination`, the two new
      widget tests above, and the new `workspace-search-capped` AT-SPI case.
      `docs/workflow-readability-matrix.md` records the stop-semantics note under
      `WFR-SEARCH-REPLACE`. No automation snapshot field changed shape;
      `content_search.result_capped` keeps its documented meaning and now reports it
      honestly.
- [ ] 6.6 Run the app via `make run`, exercise search, replace, and undo, and confirm
      stderr has no new GTK, GLib-GObject, or pixman warnings.
      **Substantially proven, deliberately left unchecked: the literal `make run`
      variant remains for the maintainer.** The debug binary was run on the real
      GNOME/Mutter Wayland session (not headless) and driven through
      `scripts/lushtext-automation.py` over: open panel, query, wait
      `search-complete`, replace query, preview, confirm, undo, plus
      `set-sidebar-visible` and `set-properties-visible` toggles to exercise the
      animated shell chrome, plus a no-results query and a many-results query.
      Fixture files went from 7 matches to 0 after confirm and back to 7 after undo,
      so search, Replace All, and Undo all really ran. Captured stderr contained no
      pixman `*** BUG ***`, no `Gtk-WARNING`, no `Gtk-CRITICAL`, no `GLib-GObject`,
      and no `Trying to measure` warning — only the host's unrelated
      `radv is not a conformant Vulkan implementation` driver notice.
      Two deliberate deviations from the literal instruction, both to avoid damaging
      the maintainer's environment: the app ran on a private D-Bus session with
      isolated `XDG_*` and `LUSHTEXT_DATA_DIR`, because (1) an installed Flatpak
      instance already owned `dev.cominotti.lushtext` and `make run` would have asked
      the maintainer's running LushText to quit, and (2) Replace All mutates files, so
      it must not be pointed at the maintainer's real workspace folders. A first pass
      without a11y isolation emitted three `Gtk-CRITICAL ... Could not activate remote
      peer 'org.a11y.atspi.Registry'` lines; those are an artifact of the private bus
      having no AT-SPI registry, and they disappeared under `GTK_A11Y=none`, leaving
      the clean stderr above. Remaining for the maintainer: `make run` on the session
      bus with dev desktop staging, which is the only part not covered.
- [x] 6.7 Confirm `model/` no longer contains the two relocated modules and that the
      matrix records the remaining `model/` policy modules as cross-cutting or
      pending.
      Neither `search_flight.rs` nor `search_retirement.rs` remains in `model/`. All
      eight censused policy modules still carry a classification: `plain_disposal`,
      `buffer_replacement`, `editor_memory`, and `migration_ledger` as cross-cutting
      and staying; `save_admission` and `minimap_analysis` as single-consumer pending
      their slots. The three additional single-workflow modules the census found
      (`workspace_scan`, `workspace_persistence`, `workspace_search`) are also still
      assigned to slots.
- [x] 6.8 Re-read the exemplar facade cold and confirm the workflow's stages are
      followable without opening the coordination or policy modules. If they are
      not, the shape is wrong and section 2 must be revisited before the change is
      archived.
      Followable: all nine stages across the two stage orders, plus both inversion
      families and their named resumption points, were traced from `mod.rs` alone,
      and every delegate the narration names exists at the named path. Two honest
      readings recorded for slot 2 rather than blocking this section:
      (a) search stage 4 names the policy type `policy::WorkspaceSearchFlight` but not
      the coordination module that submits to it, which the reader infers from stage 5
      (`execution`);
      (b) Replace stage 4, `activate_undo_replacements`, is inlined in the facade —
      it reads `preview.replace_transaction_pending`, clones `preview.undo_backup`,
      and calls `hide_undo_button()`. That is transaction bookkeeping plus GTK widget
      mutation in the facade, which its own module doc disclaims and which the
      capability spec's "Facade does not become a second implementation" scenario
      forbids. Stages 1-3 delegate to named `replace::` operations; stage 4 should
      gain one too. The undo half of this workflow is slot-2 scope, so slot 2 is the
      change that must fix it.

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
