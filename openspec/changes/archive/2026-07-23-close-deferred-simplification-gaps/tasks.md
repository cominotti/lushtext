# Tasks: Close Deferred Simplification Gaps

## 1. Type-enforced compact draft restores (D1)

- [x] 1.1 Add `PreloadedDraftSkip` and `FileDraftRestoreSkip` enums to
      `model/draft.rs` and re-shape `PreloadedDraftRestore` /
      `FileDraftRestoreResolution` to nest them
      (`Content(String) | Skip(...)`, `Restore { content } | Skip(...)`),
      updating model unit tests for the new constructors.
- [x] 1.2 Update all model-enum producers and consumers outside
      `drafts.rs` (draft planning, restore resolution services, tests) to
      the nested shape with no behavior change.
- [x] 1.3 Re-shape `GuardedPreloadedDraftRestore` /
      `GuardedDraftRestoreResolution` in `ui/window/drafts.rs` so the
      `Compact` cases hold the skip enums; simplify `take_preloaded_draft`
      to one match (no second `let ... else` re-match) and delete all three
      `unreachable!` arms at the restore consumers.
- [x] 1.4 Run the draft-restore widget tests
      (`test_startup_restore_skips_stale_file_backed_draft_once`,
      `test_startup_restore_keeps_untitled_draft_behavior`,
      `test_document_sized_preloaded_draft_publishes_only_after_bounded_install`)
      plus `cargo nextest run -p lushtext-core`; assertions must be
      unchanged.
- [x] 1.5 Run `make crash-recovery-smoke` to confirm real-process draft
      recovery is unaffected by the enum reshape.

## 2. Metadata/content split for editor loads (D2)

- [x] 2.1 Add `LoadMetadata` to `services/editor_io.rs`, make `LoadResult`
      `{ metadata, content }`, and update service-internal construction and
      the result-equivalence tests to the split shape.
- [x] 2.2 Update `GuardedLoadResult` in `ui/editor_page/load_runtime.rs` to
      hold `LoadMetadata`; move the worker's content drain to consume
      `LoadResult` by parts instead of emptying a field in place.
- [x] 2.3 Remove the `content: empty_content` destructuring and
      `debug_assert!(empty_content.is_empty())` in
      `ui/editor_page/load_save.rs`; update remaining `LoadResult`
      consumers (editor apply, encoding reopen, tests) mechanically.
- [x] 2.4 Run `cargo nextest run -p lushtext-core` and the load-pipeline
      widget tests (including
      `test_large_unicode_load_installs_in_exact_bounded_slices`) with
      unchanged assertions.

## 3. Scope-owned construction charges (D3)

- [x] 3.1 Add the closure-scoped `with_construction_charge` helper (and its
      `ChargeOutcome` result) to the admission impl in
      `services/palette/notes.rs`, with unit tests covering release on
      admit, filter-out, break, and budget-exhausted paths, and double
      release unrepresentable.
- [x] 3.2 Route the folder-note, scoped document-note, and open-tab
      document-note sidecar loops through the helper, deleting all manual
      `release_construction` calls on item exit paths.
- [x] 3.3 Verify admission metrics equivalence: existing palette note
      admission and truncation tests plus the notes-source widget tests
      pass with unchanged metric assertions
      (`peak_construction_bytes`, `current_construction_bytes`, truncation
      reasons).

## 4. Single-source smoke warning allowlist (D4)

- [x] 4.1 Capture a pre-change baseline: run `make accessibility-smoke` and
      preserve the run's `unexpected-warnings.txt` and summary
      `warning_status` for equivalence comparison.
- [x] 4.2 Create `scripts/accessibility_warning_allowlist.py` exposing
      `warning_line_is_allowlisted`, byte-identical in behavior to the two
      embedded copies (ANSI stripping plus both entry patterns).
- [x] 4.3 Rewire both heredocs in `scripts/run-accessibility-smoke.sh` to
      import the module (pass the repo root to the warning-scan heredoc,
      matching the summary heredoc's existing
      `accessibility_source_fingerprint` import pattern) and delete both
      embedded predicate copies.
- [x] 4.4 Rerun `make accessibility-smoke` on the same host and diff
      classification results against the 4.1 baseline; run
      `make check-accessibility-policy` to confirm fingerprint and policy
      health with the new `scripts/*.py` file.

## 5. Verification and documentation

- [x] 5.1 Run `make check` and the full non-widget suite; run the widget
      suite via `scripts/run-widget-tests.sh --headless` for the touched
      draft, load, and notes surfaces.
- [x] 5.2 Rerun fingerprinted smoke lanes invalidated by UI-adjacent edits
      (`make accessibility-smoke`, `make visual-smoke`,
      `make visual-geometry-smoke`) if `make check-policy` reports
      fingerprint drift.
- [x] 5.3 Review `.agents/rules/rust.md` (error/literal ownership and
      guarded-payload guidance), `.agents/rules/build.md` (accessibility
      smoke lane description), and the gtk-testing skill references for
      any wording that should now cite the type-enforced compact/guarded
      split or the shared allowlist module; update in the same change if
      stale.
