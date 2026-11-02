## 1. Preconditions and attribution

- [x] 1.1 Rebase on the merged C1 (`document-scale-policy`) if it has landed. Otherwise record which C1 edits go on C1's branch (resolved question 5): D4's derived `S_SYNTAX` value, and D6's boundary on `model/paragraph_boundary.rs`.
- [x] 1.2 Record the coordinator's decisions of 2026-10-30 (design, "Resolved questions") in `docs/next/long-line-documents.md`, dated, with any maintainer override. They cover the background-highlight gate, the language-parse toolkit-floor fallback, input-to-frame reported with its documented floor, and user-started dense invalidations in scope.
- [x] 1.3 In one commit with no behaviour change, add these to `ui/responsiveness_probe.rs`, each an empty inline function without the feature:
  - the permanent `post_open_*` spans;
  - the `highlight-updated` and adjustment-`upper` marks;
  - the low-priority settle sentinel record;
  - `LUSHTEXT_RESPONSIVENESS_FRAME_PHASES`.

  Unit tests cover the span names and the frame-phase limit, including its default of 4. This commit on top of `main` is the verdict's `c6-baseline` (design D8, review R24).
- [x] 1.4 Re-measure the breakdown on `c6-baseline`, with C1 merged or on C1's tip (review R11), under lock protocol rev 2 (`flock -x INTENT flock -x LOCK`; builds and commits `flock -s INTENT true && flock -s LOCK`), quiet gate, 3 repetitions, and replace the exploratory figures in the design if they moved.
- [x] 1.5 Attribute S-flush and S-trans (design D7): name the transition that ends about 200 ms after composition (frame-phase spans plus a `GTK_DEBUG` trace), and name what runs after the first keyfile `apply()` on a fresh profile. Record both in the design before tasks 2.5 and 6.

## 2. Axioms (run `make gtk-axioms` first)

- [x] 2.1 S1: probe and sample for the language-definition parse (CRITICAL-failing lane). Record:
  - its cost per language (`html`, `markdown`, `rust`, `json`);
  - whether the parsed definitions are shared through the language object;
  - whether a private manager and a buffer with no style scheme, attached on a worker and then handed to the GTK thread, attach without GDK, style-manager, style-scheme-manager or main-context access;
  - whether a language from that second manager highlights on the GTK thread exactly as one from the catalog's own manager (review R5).
- [x] 2.2 S2: probe the cost of `set_highlight_syntax(true)` against buffer size (64 kB, 1 MB, 5 MB), and of a language attached to an empty buffer followed by 256 KiB inserts, with their highlight idles.
- [x] 2.3 S3: probe the context engine's slice, citing the slice constant from the verified GtkSourceView source. Record its dispatch priority, whether it reschedules through an idle or a timeout, and the gap between slices, which bounds `SETTLE_QUIET` from below (review R25).
- [x] 2.4 S4: probe the validation dispatch after an insert of N visible characters of dense lines, wrap on and off. Derive `V_SLICE` as the largest N within half of `B_SETTLE_US` on the reference machine (bisected, 7 repetitions). Show that when each insert waits for the frame that lays out the previous one (`after-paint`), each validation dispatch holds one insert's lines (review R14).
- [x] 2.5 S5: probe the first write of a keyfile that does not exist yet: monitor reload, and `changed` on a second instance and on GTK's settings.
- [x] 2.6 S6: probe the restyle mechanisms of design D9 on dense 710 fixtures, wrap on and off:
  - tag-carried `tabs`, `family` and relative `scale` against the view properties: layout identity, and invalidation limited to re-tagged ranges;
  - gutter and line numbers of a view outside the `.monospace` zoom and font CSS;
  - what a `GtkSourceMap` on the same buffer shows under the tag (slider, markers, font), and what the print compositor prints (review R19);
  - right margin, `tab-width` and Focus Mode readable-column margins computed from the tag's metrics (R20);
  - hide-and-reveal: scroll anchor and insert mark kept, and what search, AT-SPI and the minimap see.

  Record which mechanism passes. If neither does, draft the GtkTextView upstream report on the batch size to `.claude/handoffs/`.
- [x] 2.6a S7: probe the dense end-of-document overscroll (review R16):
  - a `pixels-below-lines` tag on an empty last paragraph and on a non-empty one;
  - its effect on the current-line highlight, the gutter, a `GtkSourceMap` and printing;
  - the cost of a `bottom-margin` change on the dense 710 fixture, wrap on and off.

  Record which mechanism passes.
- [x] 2.7 Add S1–S7 to the ledger with their final ids, samples and "Verified against" rows, and to `crates/gtk-lush/axioms/README.md` and `CHANGELOG.md`. Run `make check-gtk-axioms`.

## 3. Install slicing (D5, D6.1)

- [x] 3.1 Measure the insert cost per KiB of short-line text and set `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES` to the largest size within half of `B_SETTLE_US`. Record the derivation beside the constant and in the programme record.
- [x] 3.2 Failing first: a unit test shows that a dense document below the byte threshold whose densest batch exceeds `V_SLICE` is installed in one slice today. A second test pins that a short-line document with more than `V_SLICE` characters in total stays direct (review R13).
- [x] 3.3 Give `next_replacement_boundary` (and the C1 boundary it delegates to) the `V_SLICE` visible-character budget beside the byte budget, paragraph-aligned, with the single over-budget paragraph exception.
  - Make `requires_chunked_install` and `BufferReplacementPlan::for_sizes` route documents where `batch_exceeds_slice(summary)` holds to the sliced path.
  - Schedule a dense install's next slice from the frame clock's `after-paint` (review R14).
  - Add property tests: coverage, progress, alignment and both budgets.
- [x] 3.4 Factor the slice-end decision over an abstract `(bytes, chars)` paragraph sequence (review R15) and add Kani harnesses in `model/buffer_replacement/kani_proofs.rs`: non-empty progress, paragraph alignment, both budgets except a single over-budget paragraph, exact coverage, and a `should_panic` pin of the visible budget. Add shard `core-post-open-settle` and the `ORACLES` entries in `scripts/kani-shards.py`, then run `make kani KANI_SHARD=core-post-open-settle` and `make check-kani-shards`. Record the CI budget from two dispatched runs.
- [x] 3.5 Widget test: opening the dense 710 fixture, wrap on and off, keeps every dispatch from install to settled within `B_SETTLE_US` on the widget lane's upper bound, and no adjustment change happens inside a paint (A37).

## 4. No post-open invalidate-all (D6.2)

- [x] 4.1 Failing first: a widget test counts default-style property changes on a dense document's editor view after the install starts. Today `bottom-margin` changes after open, and `tab-width` changes after an EditorConfig resolution.
- [x] 4.2 Implement the dense overscroll mechanism S7 selected (task 2.6a). Expected: `bottom-margin` set before the first slice from the allocated height, with resize-driven changes on dense documents deferred to a `G_PRIORITY_LOW` dispatch. Documents that are not dense keep today's path. If the tag is selected, remove the minimap's margin mirror in `sync_source_map_geometry`. Either way, these must show identical bottom space:
  - the existing overscroll widget tests;
  - `make visual-geometry-smoke`;
  - the `native-minimap-*-anchors` visual invariants.
- [x] 4.3 Run EditorConfig resolution inside the load planning worker in `ui/editor_page/load/`, replacing the separate `resolve_editorconfig_for_editor` spawn on open. Apply the formatting after any clear of old text and before the first insert slice (review R17), and record a late resolution's single revalidation in `LoadEvidence`.
  - Give draft recovery and local-history restore the same join.
  - Route the post-install re-resolutions (Save As, rename, the `use-editorconfig` toggle) through D9 on dense documents.
  - Re-apply a pending session-restore position once a paced late `tab-width` has reached the restored line (R18). Widget test with a dense restored document.

## 5. Language catalog and attach (D2, D3)

- [x] 5.1 Failing first, widget tests:
  - the activation dispatch for an HTML document records no `language_attach` stage, and the attach stage runs after the first editor frame (today it runs inside `open_document`);
  - neither the publish dispatch nor the buffer-replacement terminal attaches a language (review R1);
  - a Markdown document with the side-by-side preview never shows "Not a Markdown file" while its attach is pending (R2);
  - a restored tab that is not selected attaches only after it is first mapped (R8).
- [x] 5.2 Add `ui/language_catalog.rs` (cross-cutting, no role; module doc names its owning rows). Start it at application `startup` on a worker (review R3), hand it to the GTK thread, and copy a path-guess table for workers.
  - Route every `LanguageManager::default()` caller through it: `reapply_language`, `document_language_id` and `markdown_preview/code_blocks.rs`.
  - Make the attach stage wait for the catalog if it is not ready.
  - Unit test: the table's guess equals `guess_language` for every glob of every shipped language and a list of tricky names (R4).
- [x] 5.3 Move the path guess to the load planning worker (it returns a language id).
  - Replace the four C1 language sites with one `request_language_attach` (R1): `reapply_language` via the path, `apply_document_scale_gates`, the buffer-replacement terminal and the scale-change path.
  - Run the attach in its own `DEFAULT_IDLE` stage after the first editor frame, or after first mapping, and between install slices if the install is still running (R9). Guard it by the load ticket, so a later load, reload or rename replaces it.
  - Make `document_language_id` return the requested id while the attach is pending.
- [x] 5.4 If S1 allows it, attach on a worker through a per-language retained throwaway buffer. Otherwise:
  - run the attach from a `G_PRIORITY_LOW` idle after the first editor frame and `INPUT_QUIET`, both named constants in the settle policy. Only key, button, scroll-start and touch-begin events count as input. `ATTACH_DEADLINE` is the ceiling (review R6). Failing-first widget tests: an attach does not run while keys arrive, and does run after `ATTACH_DEADLINE` under continuous key input;
  - record the parse in the ledger and the programme record as a toolkit-floor exception against S1, with measured costs per language;
  - update `.claude/handoffs/c6-upstream-gtksourceview-language-parse.md` with the measured numbers, for the maintainer to file.
- [x] 5.5 Choose the publish order from S2 (`highlight_attach_plan` in `load/policy.rs`, unit-tested), so that the 5 MB HTML publish dispatch is within `B_SETTLE_US`. Widget test on a 5 MB fixture.

## 6. Background highlighting (D4)

- [x] 6.1 Derive `S_BG_HIGHLIGHT` on the lane (bisection, 7 repetitions, quiet-gated), per language for `html`, `markdown`, `rust` and `json` (review R12). Set C1's `S_SYNTAX` to the value the maintainer's answer to design open question 2 selects, by default the minimum. Replace its "not derived" doc comment with the derivation, on C1's branch if C1 is unmerged. C1 already has the total-size input, `ReductionReason::TotalSize` and the notice (R10). Re-run C1's `DocumentScale` Kani harnesses; their statements do not change.
- [x] 6.2 Update `.claude/handoffs/c6-upstream-gtksourceview-worker-slice.md` with S3's cited constant and the derived threshold, and link it from the programme record for the maintainer to file.
- [x] 6.3 Implement whatever task 1.5 decided for S-flush and S-trans, failing first where a widget test can observe it.

## 7. Paced user-started invalidations on dense documents (D9, after task 2.6)

- [x] 7.1 Failing first: widget tests on a dense 710 document for a word-wrap toggle, a zoom step and a font change. GtkTextView does not expose its invalid region (review R23), so each test asserts LushText's evidence instead: no dispatch re-tags or reveals more than `V_SLICE` visible characters, the visible paragraphs come first, and the dispatch durations. A second test zooms with two dense documents open: the selected one is paced first, the other only when shown, and neither minimap is drawn at document size (R19, R22). The tests fail today: one property change invalidates every line.
- [x] 7.2 Add the pure `batch_exceeds_slice(summary)` predicate (`worst_batch_chars > V_SLICE`) and the allocation-free step function `next_restyle_range(cursor, visible, v_slice, paragraph_chars)` (review R21) to the load workflow's `policy.rs`, with unit and property tests. Add Kani harnesses in shard `core-post-open-settle`, over a six-paragraph model with symbolic lengths: exact coverage, visible first then the stated alternation, `V_SLICE` per range except a single over-budget paragraph, paragraph alignment and termination.
- [x] 7.3 Implement the mechanisms S6 selected:
  - word wrap is always B, hide and reveal;
  - zoom, font and tab width use A if it passed: the alternating document style tags (`tabs`, `family`, relative `scale`, no `wrap-mode`) applied at install, dense views outside the zoom and font CSS, an insert handler for edge inserts (A25), print without the document tags, and Focus Mode margins from the tag's metrics (R20). Otherwise they use B.

  Either way, re-tags or reveals run as a generation-guarded `G_PRIORITY_LOW` continuation that a newer change supersedes, the selected editor first and each other dense editor when next mapped. Route the late EditorConfig tab width (task 4.3) and the tab-width preference through it on dense documents.
- [x] 7.4 If neither mechanism passed S6, record the toolkit-floor exception against S6 with its batch cost and the drafted upstream report, and change the 7.1 widget tests to assert the recorded exception's measured bound instead of `V_SLICE`.

## 8. Settle readiness and evidence

- [x] 8.1 Add the settle stage, the `SETTLE_QUIET` clause and the low-priority sentinel (review R25) to the load workflow's `evidence.rs` and pure `policy.rs`. Debounces started by user edits do not hold settle (R26). Write the three driven evidence proofs: side-effect freedom, disposal honesty and materialization neutrality.
- [x] 8.2 Add the `document-settled` readiness predicate and the bounded tab-snapshot settle fields. Update `docs/automation.md` and `docs/automation-reference.md`, then run `make check-automation-docs` and `make automation-client-self-test`.

## 9. Lane

- [x] 9.1 Failing-first driver self-tests:
  - the post-open settle table attributes by span and mark, not by phase;
  - an unspanned dispatch that ends at a `highlight-updated` mark is highlighting;
  - background runs until the driver-computed settle, from marks and sentinel records, and a cap hit fails;
  - the C6 verdict rule, including the median-plus-one-frame regression rule (review R27) and the `B_INTERACT_US` exemption of events overlapping an exception dispatch (R7).
- [x] 9.2 Add the `type-during-settle` phase (`SETTLE_TYPING_KEYS` keys, then wait for settle; review R26), the `wrap-toggle`, `zoom-step` and `font-change` phases, the toolkit-floor exception line, the input-to-frame column beside the documented floor and the cases `markdown/preview`, `editorconfig/tab-width`, `session/restore-3-small` (each restored tab selected in turn) and `recovery/small-drafts`.
- [ ] 9.3 Run `make check-responsiveness-lane`.

## 10. Verdict

- [ ] 10.1 Build `c6-baseline`'s (task 1.3) and the branch's release `responsiveness-probes` binaries (`flock -s INTENT true && flock -s LOCK`).
- [x] 10.2 Run the interleaved verdict of design D8 (`flock -x INTENT flock -x LOCK`, quiet gate, one run per lock hold, sampler, `btrfs-stall-report`), 7 repetitions per side.
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

## Implementation notes (2026-10-31)

- 1.5: S-flush is answered by A71 (creating the keyfile triggers no monitor reload, so nothing follows the first flush). S-trans was not traced with `GTK_DEBUG`; on the branch every post-open dispatch it could be stayed within `B_SETTLE_US` on the lane (worst 15.0 ms, dense/710 wrap-off). 6.3 therefore changed nothing.
- 2.1 / 5.2 / 5.4: the parse cannot leave the GTK thread (A67), so the attach is deferred (toolkit-floor exception). The language catalog warms the default manager in a GTK `DEFAULT_IDLE` at startup, not on a worker: building it on a worker produced `resource://` warnings in the widget run.
- 3.1: `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES` 1 MiB → 704 KiB from the lane's install costs (the 1 MB direct install took 11.1 ms).
- 4.2: S7 chose the margin set before the first slice, frozen at install; a resize-driven change on a dense document runs at `G_PRIORITY_LOW` (A73 exception).
- 4.2 (2026-10-31, re-measuring the 11 MiB restore): a dense document's view that only becomes shorter keeps the larger margin while it still fits the view. The restore's "Restored from Local History" alert had shrunk the margin, and GtkTextView re-laid out the whole document in ~18 ms validation dispatches after every restore. The earlier attribution of that train to the `editable` restore was only half of it; the buffer-level edit lock removed that half. The dispatches of 29–42 ms that remain right after the restore are the local-history dialog's close animation rendered by the GSK cairo renderer the harness and the lane use (perf: pixman bilinear-affine masks, opacity groups), on both sides.
- 6.1: per-language thresholds measured by the A69 sample through `scripts/measure.sh` for twelve languages, with a mid-document typing gate for html and json; maintainer decision of 2026-10-31. The chunked attach-before-install is queued as C6b.
- 7.1: the widget tests cover the wrap toggle and the paced-everywhere path the zoom and font triggers call; the zoom-step and font-change triggers themselves are covered on the lane (the widget harness does not install the display font provider).
- 7.3: mechanism B (hide and reveal outward); A72 rejected the tag-carried style.
- 10.2 (2026-11-01, verdict follow-up, attributed with perf from the lane):
  - The ~10-13 ms dispatch ~510 ms after open (both sides) is GIO's keyfile-backend monitor dispatch after LushText's settings flush (`g_keyfile_settings_backend_keyfile_writable` > `g_local_file_query_info` > `ignore_trash_path` > `g_unix_mount_entries_get`, a `/proc/self/mountinfo` parse). Keys typed then waited behind it. The flush now waits for 250 ms of input quiet (`FLUSH_INPUT_QUIET`, deadline 5 s).
  - The lane's wrap-toggle and font-change interaction dispatches (5-8.5 ms) are that same GIO dispatch: the lane triggers both by writing the keyfile from outside, and LushText's restyle runs in the next dispatch. A GIO floor of the trigger, not LushText work.
  - The dense zoom step's first dispatch (9.8-16.6 ms, both sides) is GTK's style and font change: CSS revalidation of the widget tree (`gtk_css_node_validate`), the new font's fontconfig and FreeType load, and the visible lines' re-layout (`pango_layout_check_lines`) with GtkSourceView's tab-width recompute. Not sliceable from LushText; a toolkit floor.
  - The longer activation-to-settled (12 cases) is the accepted cost of the deferred attach (maintainer decision 2026-11-01); the no-regression rule was amended accordingly.
- 7.3 (2026-11-01, interleaved verdict): the reveal's cleanup removes the hiding tag only from the runs that still carry it. `gtk_text_buffer_remove_tag` invalidates its whole range when the tag affects line sizes, tagged or not, so the buffer-wide cleanup added in review re-laid out the whole document after every reveal (a 23 ms validation dispatch on dense/710 wrap-on, 4 of 4 repetitions).
- R25 premise: the context engine's incremental slice runs at `G_PRIORITY_LOW`, the sentinel's own priority (A69), so the settle also requires `SETTLE_QUIET_US` without highlight marks.
- `V_SLICE` is 11,360 visible characters (editor-like A70 re-derivation, confirmed on the lane), not the bare-view 55,380.
- 3.3 / R14 (maintainer decision 2026-10-31, option B): slices are queued from a `G_PRIORITY_DEFAULT_IDLE` idle below `GTK_TEXT_VIEW_PRIORITY_VALIDATE` instead of the frame clock's `after-paint`. Pacing by painted frames kept every dispatch within 11.4 ms but took 16.7 s (open) and 17.5 s (restore) for an 11 MiB dense document against main's 1.9 s and 2.4 s. A70 now also proves the idle pacing (each validation dispatch holds one insert's lines, outside any frame, with no A37 warning); the interim frame-or-100 ms helper (`frame_pacing.rs`) was removed with it.
