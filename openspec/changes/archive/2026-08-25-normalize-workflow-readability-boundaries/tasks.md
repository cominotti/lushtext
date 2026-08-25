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
- [x] 6.6 Run the app via `make run`, exercise search, replace, and undo, and confirm
      stderr has no new GTK, GLib-GObject, or pixman warnings.
      **Done, including the literal `make run` variant.** Two passes were recorded.
      The first pass ran the debug binary on the real GNOME/Mutter Wayland session on
      a private D-Bus with isolated `XDG_*` and `LUSHTEXT_DATA_DIR`, because an
      installed Flatpak instance still owned `dev.cominotti.lushtext` at the time. It
      drove open panel, query, wait `search-complete`, replace query, preview,
      confirm, undo, `set-sidebar-visible` / `set-properties-visible` toggles for the
      animated shell chrome, and no-results plus many-results queries. Fixture files
      went 7 matches -> 0 after confirm -> 7 after undo. Stderr was clean under
      `GTK_A11Y=none`; without it, three `Gtk-CRITICAL ... Could not activate remote
      peer 'org.a11y.atspi.Registry'` lines appeared, which are an artifact of a
      private bus with no AT-SPI registry rather than an app defect.
      The second pass closed the remaining literal gap: with no Flatpak instance
      running, `make run` was invoked verbatim on the maintainer's real session bus
      through the dev desktop staging path (`gtk-launch dev.cominotti.lushtext.Devel`),
      under `.agents/skills/gtk-agentic-debugging/scripts/run-gtk-debug-session.sh`
      with `--pid-pattern '/target/debug/lushtext$'`, so app stderr was captured on the
      harness PTY (confirmed via `/proc/<pid>/fd/2 -> /dev/pts/3`) with journald
      capture as a second lane. Replace All safety was preserved by scoping the single
      workspace folder to a scratch fixture (four files, seven occurrences of a unique
      nonsense token) with the maintainer's `workspaces.json` backed up and restored
      afterwards, so no real workspace file could be rewritten.
      Exercised on the session bus: `win.set-search-panel-visible` and the
      Ctrl+Shift+F accelerator action `win.toggle-search-panel`; the token query
      (7 matches in 4 files), a no-results query (0/0), and a denser query;
      `win.set-search-panel-replace-query`, `win.preview-search-panel-replacements`
      (7 preview rows, 7 checked), `win.confirm-search-panel-replacements`, and
      `win.undo-search-panel-replacements` — twice, with two different replacement
      templates. On-disk verification after each cycle: token count 7 -> 0 with 7
      replacements present after confirm, then 7 -> 0 back after undo, so search,
      Replace All, and Undo all really ran against real files. Sidebar and properties
      panes were toggled four times each between passes to drive the animated shell
      chrome, then `visual-geometry-settled` was awaited.
      Verdict: the captured session stderr contained no `Trying to measure`, no
      `Gtk-WARNING`, no `Gtk-CRITICAL`, no `Gdk-WARNING`, no `Gdk-CRITICAL`, no
      `Adw-WARNING`, no `Adw-CRITICAL`, no `GLib-GObject-WARNING`, no `GLib-CRITICAL`,
      and no pixman `*** BUG ***`. The only lines present were the host's unrelated
      `radv is not a conformant Vulkan implementation` driver notice and two
      pre-existing app-level tracing errors from session restore of files the
      maintainer had already deleted (`Cannot stat ...`), neither of which is a
      toolkit warning nor related to the search/replace/undo workflow. The user
      journal showed no LushText entries for the run window.
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

- [x] 7.1 Write `docs/next/workflow-readability.md` as the programme record,
      following the posture-and-gates shape of `docs/next/gtk-lush.md` and
      `crates/gtk-lush/GOVERNANCE.md`. It MUST contain: the four measured problems
      with their numbers; the baseline quantification from design.md D10 (6% of
      `ui/` + `model/` migrated by this change, 2 of 8 policy modules, 48 of 639 test
      seams, 2 of 90 long signatures); the remaining-scope table; the sequencing
      rationale (why census-first, why vertical slices); the rejected alternatives
      (new policy layer, naming-only pass, horizontal slicing); the deferred work
      with what would justify taking it on; and a link to this change by name for
      full rationale.
      Written with all required content. The D10 baseline figures were verified
      against the tree rather than copied, and the record states both the planned
      and the actual number wherever they differ:
      - 6% / 4,762 of 79,017 lines: holds as the exemplar workflow's censused
        footprint, and the record says explicitly that this is the workflow's
        footprint rather than the diff size.
      - 2 of 8 policy modules: 2 relocated is correct, but the record restates the
        denominator as **2 of 7 relocation candidates**, because 4 of the 8
        mechanism modules are cross-cutting and stay while the census found 3
        previously unlisted single-workflow modules. `model/` went 29 → 27 files.
      - 48 of 639 test seams: the planned figure was exactly right — the exemplar
        workflow held 48 of the 639 `#[cfg(feature = "test-utils")]` sites
        (2 + 6 + 14 + 26 across `mod.rs`, `imp.rs`, `runtime.rs`, `replace.rs` at
        `91fcce5`). It now holds 40, its 23 `*_for_test` functions became 7, and
        repo-wide totals moved 639 → 631 sites and 351 → 335 functions.
      - 2 of 90 long signatures: **wrong as stated**, and the record says so. The
        actual result is 1 seam reified and 0 long signatures shortened, because
        census finding 3 established that this workflow's only ≥6-parameter
        function is an exempt row-item constructor. The record requires later
        changes to report seams reified as the primary unit.
      Also corrected while writing: the facade measures **350** physical lines
      today (75 doc-narration, 166 code), not the 357 the matrix recorded — the
      result-cap fix delegated the undo hand-back out of the facade after the
      migration landed. The matrix's Facade size budget section is updated to 350
      with both numbers and the reason for the change.
- [x] 7.2 Record in the programme record and the matrix the planned migration order
      and per-change scope: search/replace and palette, then save and load, then
      draft/recovery and session, then workspace tree and notes, then minimap, then
      the residual sweep. Note that migration changes are expected to need only
      proposal and tasks, and that needing a spec delta signals an incomplete
      contract.
      Both files carry the seven-slot order with per-slot scope and the
      "proposal + tasks" expectation (slot 6 the expected exception, needing a
      design document for pixel-verified minimap geometry), plus the rule that a
      needed spec delta signals an incomplete phase-0 contract and triggers the
      retroactive-amendment rule. Because the exemplar already migrated the
      search/preview half of slot 1, both files also record the **slot 1 residue**
      slot 2 inherits: the Replace All write path and its undo journal,
      `replace.rs`'s final coordination role name, making
      `activate_undo_replacements` a delegation instead of facade-inlined
      transaction bookkeeping, `model/workspace_search.rs`'s relocation decision,
      the normative facade line budget number, and the first
      `WFR-AUTOMATION-SPINE` projections beyond search. Both files also state that
      slot 2 must **not** re-plan the capped-result delivery fix or the `WalkStop`
      stop-semantics split from commit `f0ab1d9`.
- [x] 7.3 Record the deferred work explicitly and separately from the planned work:
      actuation test seams (the missing workflow/dialog-presentation boundary) and
      state-machine reification of inverted drains, each with its reason and its
      justification bar.
      `## 7. Deferred work` in the programme record, deliberately after the
      remaining-scope table and outside every slot, with a What / Why deferred /
      Justification bar for each. Actuation seams (~98 functions) unblock on a
      change that independently needs the dialog-presentation boundary and can pay
      for real-session proof of every affected dialog path. State-machine
      reification is recorded as possibly never justified: it requires a specific
      workflow that still cannot be reasoned about from its narration *and* real
      defects traced to that opacity.
- [x] 7.4 Record the unblock point: the migration changes become authorable after
      sections 1 and 2 of this change, not after the exemplar. State what each needs
      from the census — value-object names, per-kind seam counts, risk tier.
      `## 4. The unblock point` states that authoring unblocked after sections 1
      and 2, not after the exemplar, and that slots may be authored in any order
      but must land in order. Its table maps each required input to the matrix
      section that already holds it: seam value-object names and their exact field
      bundles, per-kind seam counts, risk tier and slot, ordered stages with every
      inversion, owned pure policy with its relocation target, and the settled
      conventions.
- [x] 7.5 Record the retroactive-amendment rule in the programme record and the
      matrix so a future migration cannot fork the convention without re-migrating
      earlier workflows.
      `## 8. Retroactive amendment` in the record and a new
      `### Retroactive amendment` subsection under the matrix's Completion Rule.
      Both name what the rule covers (role names, facade budget number, seam
      value-object shape, evidence visibility rule, everything in Settled
      Conventions) and both state the practical consequence: the cheapest moment to
      correct the convention is slot 2, while exactly one workflow is migrated.
- [x] 7.6 Make the record discoverable from the surfaces a session loads
      automatically: add a pointer in `AGENTS.md` (architecture or key design
      decisions), a pointer from `docs/workflow-readability-matrix.md`, and a
      reference from the relevant `.agents/rules/*.md` entry revised in section 4.
      A future session MUST reach the programme record without knowing to search
      `docs/next/`.
      Four carriers, all auto-loaded or one hop from an auto-loaded file:
      `AGENTS.md`'s Workflow Role Convention section (a "read it before planning
      any workflow-structure work" bullet beside the matrix bullet),
      `.agents/rules/rust.md`'s Workflow Vocabulary And Boundaries section (chosen
      because that is where a cold session lands for the convention itself, and it
      names the rejected alternatives and deferrals so the pointer is worth
      following), `.agents/rules/build.md`'s `check-workflow-boundaries` paragraph
      (which now has to describe the record because the gate reads it), and the
      matrix's own header paragraph.
- [x] 7.7 Add `docs/next/` planned-work records and
      `docs/workflow-readability-matrix.md` to the mandatory-update trigger list in
      `.agents/rules/documentation.md`, so a later migration change is required to
      advance the record rather than leaving it stale. This closes the existing gap
      where `docs/next/` is the repo's planned-work convention but is not a
      documentation trigger.
      `.agents/rules/documentation.md` gains `docs/next/*.md` as mandatory-update
      item 8 (the skills-references item renumbers to 9) plus two trigger lines: the
      existing workflow-structure trigger now also requires advancing the record's
      status line, baseline, remaining-scope table, and slot ledger, and a new
      trigger covers any `docs/next/` programme phase that advances, is rescoped, or
      is deferred. The matrix was already item 7 from section 4; verified present and
      extended rather than duplicated.
- [x] 7.8 Extend the section 3.3 policy check so it fails when the programme record
      claims a migration is complete but the matching matrix rows are not marked
      migrated, or when the record's remaining-scope table and the matrix disagree
      about which workflows are outstanding.
      Added as rule 6 in `scripts/check-workflow-boundaries.py`. The convention
      mirrors the facade-budget declaration pattern: one machine-readable line per
      slot, `- slot <n> (complete|outstanding): <WFR-ID>[ (partial)][, ...]`,
      documented in the record's own "Slot ledger (machine-readable)" subsection
      with the failure modes spelled out, and parsed outside fenced blocks so the
      format example is inert. Four failure modes: a `complete` slot naming a row
      the matrix does not mark `migrated`; an `outstanding` slot naming a `migrated`
      row without `(partial)`; a row id absent from the matrix; and a matrix row
      that is neither `migrated` nor `exempt` and carries a slot appearing in no
      outstanding line. The `(partial)` marker exists precisely because
      `WFR-SEARCH-REPLACE` is genuinely both — migrated for one half, outstanding
      for the other — and forcing that to be declared is stronger than tolerating
      the disagreement. The `Slot` column is located by header name rather than
      position so a later column insertion cannot silently shift it, and rows whose
      slot is `none` (the two cross-cutting/exempt rows) owe no ledger entry.
      **Absent-record decision:** `check_tree` takes the record path as an optional
      argument. Omitting it leaves rule 6 inert, which is what the fixtures for
      rules 1 through 5 rely on; passing a path whose file is missing is a reported
      finding. The real-tree entry point always passes the canonical path, so the
      rule can never silently no-op in this repository while remaining composable.
      Documented in the module docstring.
      Self-test cases added (all pass, plus the pre-existing cases): agreement
      passes; complete-but-pending fails; a matrix row missing from the ledger
      fails; a `migrated` row listed outstanding without `(partial)` fails while the
      same row with `(partial)` passes; an unknown row id fails; an absent record
      fails when the path is supplied; an omitted record path is inert; and a record
      with no ledger lines fails. Verified independently that rule 6 is really
      exercised on the current tree: 19 unsettled slotted rows, all listed, no
      extras, 7 ledger claims parsed.
- [x] 7.9 Cold-read verification: with this change's conversation and artifacts set
      aside, read only `AGENTS.md`, the revised rules, the matrix, and the programme
      record, and confirm the next change's scope and prerequisites are derivable. If
      they are not, the record is insufficient and section 7 is incomplete.
      Run as a real adversarial cold read, not a self-review: a separate reader with
      no access to this change's conversation or artifacts, restricted to
      `AGENTS.md`, the four revised rules files, the matrix, and the programme
      record, was asked to answer the problem, the completion state, the next
      change's exact scope, its prerequisites, the deferrals, the amendment
      constraint, and the gate's failure modes — and to attack the documents.
      **First pass: derivable, with 10 findings.** Six of the seven questions were
      answered cleanly with quotations; the seventh (prerequisites) landed on a real
      self-contradiction. Findings acted on:
      - **A (real, load-bearing)** — the two-completed-lower-risk-proofs rule for
        `tier-3` versus slot 2, which carries a tier-3 half after only slot 1. The
        reader had to guess. Both the record and the matrix now state the decision
        slot 2 must make explicitly (sequence the tier-2 palette first inside the
        change, or split into 2a/2b) and that slot 2's table position is not a
        waiver.
      - **E (latent gate deadlock)** — `WFR-AUTOMATION-SPINE`'s slot is
        "2 onward", so when slot 2 completes it could neither be marked `migrated`
        truthfully nor pass rule 6. Resolved by defining `(partial)` on a
        `complete` ledger line as "this slot's share is done, the row continues",
        exempting it from the migrated requirement; implemented, documented, and
        self-tested.
      - **D** — `deferred` meant two things (matrix row status = slotted and
        planned; record section = unslotted and possibly permanent). Disambiguated
        in the record's deferred-work section.
      - **F** — the 8 / 23 / 7 seam figures did not reconcile anywhere. The record
        now reconciles them exactly (23 pre-migration `*_for_test` functions = 8
        inspection retired + 5 configuration setters + 3 configuration readers + 7
        remaining actuation/probe), verified against `91fcce5` and the current tree,
        and distinguishes them from the matrix's separate "23 observation getters"
        prose figure.
      - **G** — the record restated the superseded "90 long signatures" figure in
        the same sentence that pointed at its correction; it now gives 88
        receiver-counted / 43 strict and requires later changes to say which.
      - **H** — slots 2 through 7 had no change names or lookup path; the record now
        gives a naming convention and says to check `openspec list` and the archive
        first.
      - **I** — clarified that the one evidence-visibility rule is "narrowest the
        readers require, and a pre-existing wider type is narrowed to it", matching
        `workflow-evidence-surfaces`.
      - **L** — stale "this change" deixis in a durable document: four occurrences
        in the matrix rewritten to name slot 1.
      - **B, C, K were false**: the reader's injected copy of the rules files
        predated section 4, so it reported `make check-workflow-boundaries` missing
        from `build.md`, the matrix missing from `documentation.md`, and a stale
        mutation-scope statement. All three are present on disk
        (`build.md:42`, `build.md:135-142`, `build.md:396-412`,
        `documentation.md:18`, `widget-wiring.md:279+`); verified before dismissing.
      **Second pass on the fixed documents: A, D, E, F, G, H, I, L all confirmed
      resolved, and the cold-start questions answered.** Three new findings, all
      fixed:
      - **N1** — `rust.md` correctly says a new coordination role name amends
        `gtk-adapter-module-boundaries`, but the record's carrier table and the
        matrix header named only the two new capabilities. Since the bounded role-name
        set is exactly what slot 2's `replace.rs` decision needs, both now name all
        five normative specs with what each owns, and the record calls out that row
        for slot 2.
      - **N2** — the "split into 2a/2b" remedy was not expressible in the ledger
        grammar. Documented that `<n>` is a slot *label* and a split keeps its number
        with a letter suffix (never renumbering later slots); the parser already
        accepted it, and a self-test now pins that.
      - **N3** — `build.md`'s failure-mode list omitted the facade-budget check and
        the roles-declared-for-a-non-migrated-row check, and would have gone stale
        the moment slot 2 declares a budget. Both added, the budget one phrased
        conditionally.
      Residual sweeps from the same pass: the matrix's evidence-visibility paragraph
      no longer states the rule in open census tense, and the record explains that a
      migrated row's `Seams (i/c/a/p)` cell holds migration prose rather than a
      tuple, so sizing evidence work uses an unmigrated row's tuple.
      Honest result: **derivable after two passes.** The scope, prerequisites,
      deferrals, amendment constraint, and gate behavior of slot 2 are all readable
      from the four permitted surfaces without this change's artifacts.
