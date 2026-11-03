## 1. Axioms first

- [x] 1.1 A81 probe and sample (`probes/a81.rs`, `examples/a81_equal_priority_sources_share_one_iteration.rs`): three busy idles at one priority, one a step lower, one added from a callback. Holds on GTK 4.22.5.
- [x] 1.2 A82 probe and sample: a never-selected `AdwTabView` page's source view validates a load while hidden (the selection-policy idea is ruled out), as does a page selected once and hidden; a buffer no view shows is the control. Holds.
- [x] 1.3 A83 probe and sample: an empty buffer stops a hidden view's validation; the document buffer is unchanged; reattach re-lays out from nothing at the buffer start; a scroll queued right after a reattach lands elsewhere, re-scrolling on every `upper` change keeps the top near it and one more scroll lands exactly; `upper` notifies while unmapped. Holds.
- [x] 1.4 Ledger rows A81–A83; `make check-gtk-axioms` passes. Retarget the A69 co-dispatch citations to A81 (the SC and held-typing archived designs, the SC tasks, the record, `responsiveness_probe.rs`, `language_catalog.rs`, `held_input.rs`, `composition/execution.rs`).

## 2. Attribution

- [x] 2.1 Attribute the `steady-populated` `type-during-settle` shift from SC's `RV-*` artifacts (design D5): the first editor frame enters the phase window because readiness now arrives before it.
- [x] 2.2 perf of that frame on `main` (record, "EV").
- [x] 2.3 Measure `eviction/reselect` on `main` f79e7a5d (cap hit) and the design alternatives (D4): placeholder alone, a 1 ms timeout for hidden slices, the completion priority, and `G_PRIORITY_DEFAULT_IDLE + 1` with the retained permit (chosen).

## 3. Implementation

- [x] 3.1 `ui/editor_page/view_attachment.rs`: document and placeholder buffers, detach at construction, attach on `map`, the hidden-validation watch and the completion-priority detach, the kept top line and its hold until laid out (released by input), `scroll_to_document_iter`, `detached_top_line`, `live_document_buffer`, `view_attachment_counts`.
- [x] 3.2 `buffer()` returns the document buffer; every production `source_view().buffer()` read of the document goes through it or `live_document_buffer()` (save capture mode, the buffer-replacement, local-history and print evidence); scrolls through `scroll_to_document_iter` (restore position, local-history restore, bookmarks, search navigation, Protected View navigation); Focus Mode's typewriter centring skips a detached view; `visible_top_line` reads the kept line.
- [x] 3.3 `load::policy::install_slice_pacing` (unit-tested over every input) and its use in the load install and whole-buffer replacement scheduling: background paced slices at `G_PRIORITY_DEFAULT_IDLE + 1`. The first verdict's completion-priority variant starved the visible view (`recovery/protected-32mib` scroll input-to-frame 109–145 ms) and was replaced (design D4).
- [x] 3.7 `FileLoadAdmissionPolicy::retain_for_install` (failing-first unit tests: an installing grant frees its reading slot and keeps only its retained weight; it never grows and drops exclusivity once it fits) and `TransientLoadPermit::retain_for_install` at the read's acceptance.
- [x] 3.4 Failing-first widget tests (`tests/widget/background_tab_layout.rs`, shard `editor-page`): hidden tabs hold no lower-priority idle back after a view-wide restyle (657 iterations without the change), and a hidden tab seen laying out is detached and comes back at its top line.

- [x] 3.5 What the first `make test` found (design D6): a widget test that edited through `source_view().buffer()` now uses `buffer()`; the selected tab keeps its pacing (`install_follows_layout`); the projection pass defers the selected editor's announcement while the opened document's focus handoff is pending. The ordering test failed 2 of 5 runs under ten CPU spinners without the deferral, 0 of 5 with it.

- [x] 3.6 What the third `make test` found: Protected View takes GTK's primary-selection provider off the document while realized, and `set_buffer` on a realized view removes it from the old buffer, a `Gtk-CRITICAL` when it is already off. The swap puts it back first and the attach takes it off again (`set_view_buffer`). Failing-first widget test `background_tab_layout::test_a_protected_tab_detaches_and_reattaches_without_primary_selection_criticals` (the CRITICAL without the fix, clean with it).

## 4. Lane

- [x] 4.1 `switch-hidden` phase of `eviction/reselect`; driver self-test passes.

## 5. Verdict and gates

- [x] 5.1 Interleaved verdict against `main` f79e7a5d: `eviction/reselect` (with `switch-hidden`), `session/restore-3`, `session/restore-3-small`, `recovery/protected-32mib` and the C2/C3 protected install cases, `baseline/small` (all profiles, `steady-populated` included), the `at` variant of `eviction/reselect`; load recorded.
- [x] 5.2 Record the verdict in `docs/next/long-line-documents.md` (EV row and section).
- [x] 5.3 Gates: fmt, check-fmt, clippy, workflow boundaries, automation docs, bins, rustdoc, `make test` (1,559 widget tests, 0 FLAKY), `make gtk-axioms` (72 held), Kani `core-post-open-settle` (8 of 8; the shard over `load/policy.rs`), and the accessibility, visual, visual-geometry and crash-recovery smokes.
- [x] 5.4 Data-safety self-review: every save, draft, session, search, print, eviction and held-typing path reads or writes `buffer()` (the document); a detach or attach changes no text, modified flag, undo state or caret (A83, and the widget tests read the document whole while its view shows the placeholder); `is_modified`, close prompts and drafts are buffer-level; the kept top line is a left-gravity mark in the document, deleted on every exit; the transient permit is released exactly once from either the reading or the installing state, including stale, cancelled and disposed terminals.

## 6. Review fixes (review of 31aeb90a)

- [x] 6.1 M1: Protected View's primary-selection state records nothing while the view is detached and the attach applies it. Failing-first `background_tab_layout::test_protected_view_entered_while_detached_keeps_the_primary_provider_off` (the CRITICAL, then GTK's provider on PRIMARY).
- [x] 6.2 A detach holds a reference on the document's primary-selection registration (GTK clears PRIMARY at refcount zero); failing-first `test_detaching_a_hidden_view_keeps_the_users_primary_selection`.
- [x] 6.3 M2: `ui/at_listeners.rs` (GTK's listener table mirrored); no detach while a listener listens, small detached views attach when one appears; `VIEW_DETACH_MAX_LINES` 2^18 measured (design D7); widget tests for both (the line cap failing-first).
- [x] 6.4 M3 and M4: bookmark scroll through `scroll_to_document_iter`; print compositor over the document buffer (evidence field `compositor_char_count`); failing-first `window::test_a_bookmark_opens_in_a_never_shown_tab_in_preview_only_mode` and `window::test_printing_a_never_shown_tab_prints_its_document`. Held-typing replay and the search bar's match scroll check `view_shows_document()`.
- [x] 6.5 Admission: `install_retained_weight` (body + 4 × characters not yet installed + overhead), exclusivity kept; unit tests for the bound and for overlapping installs.
- [x] 6.6 Exact position inside a wrapped paragraph; failing-first `test_a_detached_view_comes_back_inside_a_long_wrapped_paragraph`.
- [x] 6.7 Pacing aging (256 ms); `test_a_background_slice_ages_into_the_paced_priority`.
- [x] 6.8 Lane: `eviction/switch-hidden-giant` and `eviction/switch-hidden-dense`; the `adjustment_value` mark and the wheel rule (driver self-test).
- [x] 6.9 Activation present only for a window that is not active (the span-less tail after the second open, design D7).
- [x] 6.11 A dense document's reattach is paced like a restyle (`restyle::prepare_paced_reattach`), and the kept-line hold lasts until its reveal is done; `test_showing_a_never_shown_dense_tab_reveals_it_in_paced_ranges` (and the wrapped-paragraph test, which failed with the reveal and without the longer hold).
- [ ] 6.10 Verdict from the final commit against `main` f79e7a5d, load recorded; record corrected.
