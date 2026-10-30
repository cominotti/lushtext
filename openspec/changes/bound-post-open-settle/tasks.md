## 1. Preconditions and attribution

- [ ] 1.1 Rebase on the merged C1 (`document-scale-policy`) if it has landed. Otherwise record which C1 edits (D4's total-size component, D6's boundary on `model/paragraph_boundary.rs`) go on C1's branch (Open Question 5).
- [ ] 1.2 Record the coordinator's decisions of 2026-10-30 (design, "Resolved questions") in `docs/next/long-line-documents.md`, dated, with any maintainer override. They cover the background-highlight gate, the language-parse toolkit-floor fallback, input-to-frame reported with its documented floor, and user-started dense invalidations in scope.
- [ ] 1.3 Add the permanent `post_open_*` spans, the `highlight-updated` and adjustment-`upper` marks, and `LUSHTEXT_RESPONSIVENESS_FRAME_PHASES` to `ui/responsiveness_probe.rs` (empty inline functions without the feature). Unit tests cover the span names and the frame-phase limit, including its default of 4.
- [ ] 1.4 Re-measure the breakdown on `main` with the permanent spans under lock protocol rev 2 (`flock -x INTENT flock -x LOCK`; builds and commits `flock -s INTENT true && flock -s LOCK`), quiet gate, 3 repetitions, and replace the exploratory figures in the design if they moved.
- [ ] 1.5 Attribute S-flush and S-trans (design D7): name the transition that ends about 200 ms after composition (frame-phase spans plus a `GTK_DEBUG` trace), and name what runs after the first keyfile `apply()` on a fresh profile. Record both in the design before tasks 2.5 and 6.

## 2. Axioms (run `make gtk-axioms` first)

- [ ] 2.1 S1: probe and sample for the language-definition parse. Record its cost per language (`html`, `markdown`, `rust`, `json`), whether the parsed definitions are shared through the language object, and whether a private manager and buffer attached on a worker, then handed to the GTK thread, attach without GDK, style-manager or main-context access (CRITICAL-failing lane).
- [ ] 2.2 S2: probe the cost of `set_highlight_syntax(true)` against buffer size (64 kB, 1 MB, 5 MB), and of a language attached to an empty buffer followed by 256 KiB inserts, with their highlight idles.
- [ ] 2.3 S3: probe the context engine's idle slice, citing the slice constant from the verified GtkSourceView source.
- [ ] 2.4 S4: probe the validation dispatch after an insert of N visible characters of dense lines, wrap on and off. Derive `V_SLICE` as the largest N within half of `B_SETTLE_US` on the reference machine (bisected, 7 repetitions).
- [ ] 2.5 S5: probe the first write of a keyfile that does not exist yet: monitor reload, and `changed` on a second instance and on GTK's settings.
- [ ] 2.6 S6: probe the restyle mechanisms of design D9 on dense 710 fixtures, wrap on and off:
  - tag-carried `wrap-mode`, `tabs`, `family` and `size` against the view properties: layout identity, and invalidation limited to re-tagged ranges;
  - gutter and line numbers of a view outside the `.monospace` zoom and font CSS;
  - hide-and-reveal: scroll anchor and insert mark kept, and what search and AT-SPI see.

  Record which mechanism passes. If neither does, draft the GtkTextView upstream report on the batch size to `.claude/handoffs/`.
- [ ] 2.7 Add S1–S6 to the ledger with their final ids, samples and "Verified against" rows, and to `crates/gtk-lush/axioms/README.md` and `CHANGELOG.md`. Run `make check-gtk-axioms`.

## 3. Install slicing (D5, D6.1)

- [ ] 3.1 Measure the insert cost per KiB of short-line text and set `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES` to the largest size within half of `B_SETTLE_US`. Record the derivation beside the constant and in the programme record.
- [ ] 3.2 Failing first: a unit test shows that a dense document below the byte threshold whose visible characters exceed `V_SLICE` is installed in one slice today.
- [ ] 3.3 Give `next_replacement_boundary` (and the C1 boundary it delegates to) the `V_SLICE` visible-character budget beside the byte budget, paragraph-aligned, with the single over-budget paragraph exception. Make `requires_chunked_install` and `BufferReplacementPlan::for_sizes` route documents over `V_SLICE` to the sliced path. Add property tests: coverage, progress, alignment and both budgets.
- [ ] 3.4 Add Kani harnesses in `model/buffer_replacement/kani_proofs.rs`: non-empty progress, paragraph alignment, both budgets except a single over-budget paragraph, exact coverage, and a `should_panic` pin of the visible budget. Add shard `core-post-open-settle` and the `ORACLES` entries in `scripts/kani-shards.py`, then run `make kani KANI_SHARD=core-post-open-settle` and `make check-kani-shards`. Record the CI budget from two dispatched runs.
- [ ] 3.5 Widget test: opening the dense 710 fixture, wrap on and off, keeps every dispatch from install to settled within `B_SETTLE_US` on the widget lane's upper bound, and no adjustment change happens inside a paint (A37).

## 4. No post-open invalidate-all (D6.2)

- [ ] 4.1 Failing first: a widget test counts default-style property changes on the editor view after the install starts (today `bottom-margin` changes after open, and `tab-width` changes after an EditorConfig resolution).
- [ ] 4.2 Re-implement the end-of-document overscroll as a `pixels-below-lines` tag on the last paragraph, kept there by the overscroll idle. The existing overscroll widget tests and `make visual-geometry-smoke` must show identical bottom space.
- [ ] 4.3 Join EditorConfig resolution before the install in `ui/editor_page/load/`, apply formatting to the empty buffer, and record a late resolution's single revalidation in `LoadEvidence`.

## 5. Language catalog and attach (D2, D3)

- [ ] 5.1 Failing first: a widget test asserts that the activation dispatch for an HTML document records no `language_attach` stage, and that the attach stage runs after the first editor frame (today it runs inside `open_document`).
- [ ] 5.2 Add `ui/language_catalog.rs` (cross-cutting, no role; module doc names its owning rows): built on a worker after the first frame, handed to the GTK thread, with a path-guess table copied for workers. Route every `LanguageManager::default()` caller (`reapply_language`, `markdown_preview/code_blocks.rs`) through it, falling back to the default manager before the catalog is ready.
- [ ] 5.3 Move the path guess to the load planning worker (it returns a language id) and the attach to its own `DEFAULT_IDLE` stage after the first editor frame, guarded by the load ticket so a later load, reload or rename replaces it.
- [ ] 5.4 If S1 allows it, attach on a worker through a per-language retained throwaway buffer. Otherwise:
  - run the attach from a `G_PRIORITY_LOW` idle after the first editor frame and `INPUT_QUIET` (a named constant in the settle policy) with no input on the window; failing-first widget test: an attach does not run while keys arrive;
  - record the parse in the ledger and the programme record as a toolkit-floor exception against S1, with measured costs per language;
  - update `.claude/handoffs/c6-upstream-gtksourceview-language-parse.md` with the measured numbers, for the maintainer to file.
- [ ] 5.5 Choose the publish order from S2 (`highlight_attach_plan` in `load/policy.rs`, unit-tested), so that the 5 MB HTML publish dispatch is within `B_SETTLE_US`. Widget test on a 5 MB fixture.

## 6. Background highlighting (D4)

- [ ] 6.1 Derive `S_BG_HIGHLIGHT` on the lane (bisection, 7 repetitions, quiet-gated). Add the total-size component and `ReductionReason::TotalSize` to `DocumentScale` (on C1's branch if C1 is unmerged), with the notice. Re-run C1's `DocumentScale` Kani harnesses unchanged in statement.
- [ ] 6.2 Update `.claude/handoffs/c6-upstream-gtksourceview-worker-slice.md` with S3's cited constant and the derived threshold, and link it from the programme record for the maintainer to file.
- [ ] 6.3 Implement whatever task 1.5 decided for S-flush and S-trans, failing first where a widget test can observe it.

## 7. Paced user-started invalidations on dense documents (D9, after task 2.6)

- [ ] 7.1 Failing first: widget tests on a dense 710 document for a word-wrap toggle, a zoom step and a font change. Each asserts that no validation dispatch has more than `V_SLICE` visible characters of invalid lines, and that the visible paragraphs are updated first. They fail today: one property change invalidates every line.
- [ ] 7.2 Add the pure `batch_exceeds_slice` predicate over the paragraph profile, and `restyle_ranges(profile, visible, v_slice)`, to the load workflow's `policy.rs`, with unit and property tests. Add Kani harnesses in shard `core-post-open-settle`: exact coverage, visible first, `V_SLICE` per range except a single over-budget paragraph, and paragraph alignment.
- [ ] 7.3 Implement the mechanism S6 selected:
  - A: the alternating document style tags applied at install, dense views outside the zoom and font CSS, and an insert handler for edge inserts (A25);
  - B: hide and reveal.

  Either way, re-tags or reveals run as a generation-guarded `G_PRIORITY_LOW` continuation that a newer change supersedes. Route the late EditorConfig tab width (task 4.3) and the tab-width preference through it on dense documents.
- [ ] 7.4 If neither mechanism passed S6, record the toolkit-floor exception against S6 with its batch cost and the drafted upstream report, and change the 7.1 widget tests to assert the recorded exception's measured bound instead of `V_SLICE`.

## 8. Settle readiness and evidence

- [ ] 8.1 Add the settle stage and the `SETTLE_QUIET` clause to the load workflow's `evidence.rs` and pure `policy.rs`. Write the three driven evidence proofs: side-effect freedom, disposal honesty and materialization neutrality.
- [ ] 8.2 Add the `document-settled` readiness predicate and the bounded tab-snapshot settle fields. Update `docs/automation.md` and `docs/automation-reference.md`, then run `make check-automation-docs` and `make automation-client-self-test`.

## 9. Lane

- [ ] 9.1 Failing-first driver self-tests:
  - the post-open settle table attributes by span and mark, not by phase;
  - an unspanned dispatch that ends at a `highlight-updated` mark is highlighting;
  - background runs until `document-settled`, and a cap hit fails;
  - the C6 verdict rule.
- [ ] 9.2 Add the `type-during-settle`, `wrap-toggle`, `zoom-step` and `font-change` phases, the toolkit-floor exception line, the input-to-frame column beside the documented floor and the cases `markdown/preview`, `editorconfig/tab-width`, `session/restore-3-small` and `recovery/small-drafts`.
- [ ] 9.3 Run `make check-responsiveness-lane`.

## 10. Verdict

- [ ] 10.1 Build `main`'s and the branch's release `responsiveness-probes` binaries (`flock -s INTENT true && flock -s LOCK`).
- [ ] 10.2 Run the interleaved verdict of design D8 (`flock -x INTENT flock -x LOCK`, quiet gate, one run per lock hold, sampler, `btrfs-stall-report`), 7 repetitions per side.
- [ ] 10.3 Record the verdict in `docs/next/long-line-documents.md` (C6 section and sequence row): per stage and case, `main` → branch, run ids, load, storage, toolkit, and any maintainer-accepted residual.

## 11. Docs and gates

- [ ] 11.1 Update the following, re-deriving the `WFR-DOCUMENT-LOAD` cells in the matrix:
  - `AGENTS.md` (post-open settle decision, `ui/language_catalog.rs` in the module layout);
  - `README.md` (if highlighting reductions change);
  - `.agents/rules/build.md` (lane: post-open table, `type-during-settle`, `document-settled`);
  - `.agents/rules/ui.md` (the overscroll tag; no default-style property changes after install);
  - `docs/workflow-readability-matrix.md`;
  - `docs/next/long-line-evolution.md`;
  - `docs/next/formal-verification.md` (new shard);
  - the axiom ledger.

  Then run `make check-workflow-boundaries`.
- [ ] 11.2 Run, on at most 16 cores and headless, and fix every failure, including pre-existing blockers:
  - `make check` and `make test`;
  - `make gtk-axioms` and `make check-widget-shards`;
  - `make kani KANI_SHARD=core-post-open-settle`;
  - `make visual-geometry-smoke` and `make accessibility-smoke`;
  - `make crash-recovery-smoke`;
  - the rustdoc lint gate.
- [ ] 11.3 Run the `gtk-perf-review` and `data-safety` skills (the EditorConfig join and the install reordering touch the load path), and fix every finding.
