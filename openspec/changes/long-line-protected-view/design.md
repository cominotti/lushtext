## Context

This is C2 of the long-line programme. The programme record is `docs/next/long-line-documents.md`; the next-candidate list is `docs/next/long-line-evolution.md`; the origin proposal is `docs/next/long-line-responsiveness.md`.

**Revised 2026-10-29 after an adversarial review.** The first draft (2026-10-11) was written before C0's idle re-confirmation, before C1 was implemented, and before C4 and C4b existed. This revision re-bases it on what now exists, folds in the crash-recovery finding (D13), and records what the review confirmed and refuted ("Adversarial review (2026-10-29)" at the end).

- **C0** (`probe-long-line-toolkit-costs`, archived 2026-10-12) supplies the budgets in `model/responsiveness_budget.rs` (`B_INTERACT_US` 8 ms, `B_SETTLE_US` 16 ms, the 120 Hz `REFERENCE_FRAME_US`, `INPUT_TO_FRAME_FRAMES` 2), the fixture generator, the smoke-only probe, `make responsiveness-smoke`, and axioms A23–A37. Its **idle-confirmed** values (run `rr-c0-reconfirm-20261013`) are: `L_protect_stall` 500 (`L_protect` itself is not derivable on the headless lane, so C1 and C2 take the stall basis), `L_syntax` 2,000, `L_search_highlight` 10,000, `L_minimap` 22,000, `L_invisibles` 50,000, `N_visible` 500,000, `V_batch` 56,000, `R_batch` 224, `N_dense` 250, `N_inspect` bound 500, `S_syntax_interact` 16,000 bytes (`S_syntax` not derived).
- **C1** (`document-scale-policy`, implemented on PR #59, branch `document-scale-policy`, not yet on `main`) supplies, under these names:
  - `model/paragraph_boundary.rs`: `next_paragraph_break`, `slice_end_after_paragraph`, `ParagraphProfileAccumulator` (text to closed paragraph lengths), `ParagraphLengthFold`, `ProfileConfig::PRODUCTION`, `DocumentProfile` (with `summary()`), and the GTK-matching delimiter set (`\n`, `\r`, `\r\n`, U+2029; ledger A42);
  - `model/document_scale.rs`: `DocumentScale` (replacing `FileSizeCheck`) with `protected_view`, `for_profile`, `for_total_bytes`, the monotone `join`, `allows(ScaleFeature::…)`, `ReducedFeatures` with `ReductionReason::{LongParagraph, Density, TotalSize}`, and the constants `L_PROTECT` (500), `N_VISIBLE`, `V_BATCH`, `R_BATCH`, `L_SYNTAX`, `L_MINIMAP`, `L_INVISIBLES`, `L_SEARCH_HIGHLIGHT`;
  - `model/paragraph_profile_watch.rs` and `ui/editor_page/document_scale.rs`: the edit-time watch on `insert-text` and `delete-range` after-handlers, which joins each edit into the session scale;
  - `BufferReplacementRequest::with_profile` and the editor's `prepare_for_incoming_profile`/`join_installed_profile`: a whole-content install carries the body's worker-computed profile, and draft recovery already passes the draft body's profile (`GuardedPreloadedDraftRestore::Content(_, profile)`);
  - `model/buffer_replacement.rs::next_replacement_boundary`, which ends every slice after a paragraph, so **a paragraph longer than the 256 KiB slice budget installs in one GTK turn**;
  - the reduced-features inline alert, and axioms A42–A45.
- **C4** and **C4b** (archived 2026-10-20 and 2026-10-29) removed GSettings writes and bounded window construction; C2 inherits their construction profiles in the lane, and the accepted 51–61 ms content fill-in is not a C2 phase. C4c (`shorten-window-content-fill-in`, PR #67, in flight) adds A51.
- **The lane** (`scripts/responsiveness-smoke-driver.py`) now measures presented frames correctly and fails a run in which a measured case wrote no `frame` record (PR #66). The reference precondition is the coordinator's quiet gate (`.claude/handoffs/c4c-quiet-gate.sh`): for 300 s the 1-minute load stays below 4, no process except `claude` uses more than 25 % CPU, and no `cargo`, `rustc`, `nextest`, `kani`, `cbmc` or private `mutter --headless` runs; it is re-checked inside the measurement lock, sampled during the run, and checked again afterwards, and a run that breaks it is discarded and retried.

After C1, a giant-paragraph document no longer reaches the GtkSourceView context engine or the minimap. What remains is what this change removes:
- **layout**: GtkTextView lays a paragraph out whole over its visible text (A23; first layout 34–42 ms per 200k characters unwrapped, 177–217 ms wrapped), and Pango's x positions overflow `int32` past 238,319 monospace characters on one unwrapped line (A28);
- **whole-paragraph buffer costs that hidden text does not reduce**: log attrs over the whole paragraph, hidden text included (A26; 92–99 ms per call at 1.9M characters), IM surrounding text (A30), visible-cursor iteration, AT-SPI text and `TextChanged` payloads (A35);
- **the in-tab `GtkSourceSearchContext`** (25–28 s on the reference file; A33: it skips hidden text by default);
- **GTK-side snapshots of a giant single-segment paragraph**: `ui/buffer_snapshot.rs` advances each chunk with `iter_at_mark` and `forward_chars(64 Ki)`, and inside one character segment GTK walks UTF-8 from the segment start to locate a character offset, so each later chunk of a single-line paragraph costs time in proportion to its depth and the whole capture is quadratic (P8 pins this).

Maintainer decisions (2026-10-11): (1) Protected View now, editing giant paragraphs deferred to optional C5; (2) `S_syntax` is C0's; (3) `B_interact` 8 ms, input-to-frame 2 reference frames, `B_settle` 16 ms, read against main-loop stalls, not relaxed after C0's verdict; (6) `L_protect` is a fixed constant; (7) synthetic fixtures only.

Source facts re-checked for this design (GTK 4.22.5, GtkSourceView 5.20.0 sources):
- `gtk_text_view_set_editable(FALSE)` resets the IM, focuses it out, and sets the key controller's IM context to NULL (gtktextview.c:3460-3475); `focus_in` calls `gtk_im_context_focus_in` only when editable (:6044-6046). The click gesture still forces `need_im_reset` and resets the IM on every `n_press == 1` (:5850-5855). Middle-click calls `gtk_text_buffer_paste_clipboard` with `default_editable = priv->editable` (:5865-5875). Context-menu presses call `do_popup` (:5857-5862). The capture-phase claim therefore stays necessary.
- `editable` is a **default-style property** (A37): its setter invalidates every line and queues no resize. Protected View therefore sets it before the first byte of an install (nothing to invalidate), and on edit-growth entry relies on `frame_validation.rs`'s existing `notify` hook so the re-validation runs in layout, in 2000 px batches (A31).
- The two selection-prefill sites: `ui/editor_page/search.rs` reads `text(…, true)`, `ui/window/search.rs` reads `text(…, false)`.
- `ui/buffer_snapshot.rs` direct and chunked captures read `text(…, true)`, so save and draft snapshots include hidden tails; the chunked capture's per-chunk cost is the concern above.
- The action catalogue has `win.print` (Ctrl+P), `win.begin-replace` (Ctrl+H), `win.cycle-invisible-characters` (Ctrl+Shift+I), `win.toggle-bookmark` and `win.notes-toggle-bookmark`.
- `AutomationTabSnapshot` is the per-tab snapshot. `file-load`, `editor-search` and `draft-autosave` are readiness **blockers**; `draft-autosave` covers "draft persistence, deletion, or recovery resolution", which is why the recovery stall of D13 surfaces as `recovery-restore-complete blocked by draft-autosave`.
- GtkTextView recomputes the per-instance enablement of `clipboard.cut/copy/paste`, `selection.delete` and `selection.select-all` on every popup and selection change (gtktextview.c:9317-9348). Ctrl+A is bound to the `select-all` **signal** (:1835-1864), whose handler calls `gtk_text_buffer_select_range` whatever `editable` says (:9206-9224). F7 is bound to `toggle-cursor-visible` (:2046-2048). Shift+F10 and Menu are bound to `menu.popup` (:1683-1690).
- GtkSourceView's `change-number` (Ctrl+Shift+A/X, gtksourceview.c:1276-1288), `change-case` and `join-lines` change the buffer with non-interactive `gtk_text_buffer_delete`/`insert` and never check `editable` (:434-560). `move-to-matching-bracket` (Ctrl+%) calls `gtk_text_buffer_place_cursor` (:399-431). Only `move-lines` and `move-words` check `editable` (:3831, :4164).
- The loader refuses decodes with errors (`services/editor_io.rs`). The ordinary save path refuses `Mixed` line endings (`EditorSaveError::MixedLineEndings`).
- GtkTextView implements `GtkAccessibleText` `set_caret_position` and `set_selection` (gtktextview.c:11074-11119), so an assistive technology can move the insert mark.
- The btree merges adjacent character segments by copying both strings (A36), and `get_chars_in_line` costs per segment, not per character (A43).

## Goals / Non-Goals

**Goals:**
- Every interaction offered on a protected document stays within `B_interact` and 2 reference frames, and every settle dispatch stays within `B_settle`, on the C0 fixtures including the 1.9M-character single line, with one declared exception: the single GTK turn that inserts a hidden tail longer than the slice budget, which C3 owns (D13).
- Data integrity is absolute: the buffer is immutable while protected, every save writes exactly the buffer's text, no transformation runs on save, and a decode that cannot round-trip is refused, not rewritten.
- Recovering a draft whose body holds a giant paragraph no longer stalls the main thread for the layout of that paragraph (D13): the crash-recovery smoke passes inside its existing 60 s limit on a slow runner.
- Content stays reachable: the Line Inspector, find, whole-line copy and navigation into hidden tails.
- The work is covered by the conventions: a WFR role home, pure policy with Kani, axioms with probes and samples, automation fields, accessibility docs with an Orca entry, and a verdict measured under the quiet gate.

**Non-Goals:**
- Editing inside protected paragraphs (C5).
- Revealing a whole giant line in the main view.
- Head-hiding; fold-aware printing; a custom text widget; depending on upstream fixes.
- Sliced installation of a hidden tail (C3, conditional on this change's verdict).
- Window construction (C4, C4b) and the post-open settle of small documents (C6).
- Changing `REFUSE_TO_OPEN` or C1's thresholds.

## Decisions

### D0. Entry is a measured edit-stall threshold, `L_EDIT_STALL`, not `L_PROTECT` (coordinator decision, 2026-10-30)
Entry reads C1's `DocumentScale::protected_view` and nothing else, and this change re-keys that flag: it holds exactly when the longest paragraph exceeds **`L_EDIT_STALL`**, the shortest paragraph length at which **one keystroke inside that paragraph** exceeds `B_interact` (8 ms) on the reference machine. `L_EDIT_STALL` is measured by task 1.3 (a fixture sweep under the quiet gate, 7 repetitions, before any implementation) and becomes a named constant in `model/document_scale.rs`. Below it, documents stay editable under C1's reduced-features policy, exactly as C1 ships them. `L_PROTECT` (500) stays a C1 profile threshold (threshold 0 of `ProfileConfig::PRODUCTION`: the long-paragraph counts and offsets) and no longer decides `protected_view`. The batch density (`V_BATCH`) no longer decides it either: a document whose paragraphs are all at most `L_EDIT_STALL` stays editable however dense, because the decision says that below `L_EDIT_STALL` documents stay editable. Inside Protected View, density still drives the fold (D1).

**The C1 change this requires.** `DocumentScale::for_profile` sets `protect_by_length = longest > L_EDIT_STALL` and drops `protect_by_density`; `ReductionReason::Density` is no longer produced for `ScaleFeature::ProtectedView`; the C1 Kani harnesses (monotonicity, meet laws, profile-maximum agreement) are re-run unchanged in statement, since the flag stays a monotone function of one profile component. The `long-line-document-presentation` delta of this change carries C1's requirements "One monotone document scale policy governs feature availability" and "Edits downgrade features synchronously and never re-enable them" as MODIFIED, with every scenario name kept: the first re-keys `protected_view`, the second adds the entry-undo exception of D2. If C1 is still unmerged when this lands, the same edit is made on C1's branch instead and the MODIFIED blocks are dropped from this delta.

**Why `L_PROTECT` was rejected as the entry threshold.** At 500 it would have made view-only any document with one paragraph over 500 characters: prose written one paragraph per line, notes, JSON Lines, and this repository's `AGENTS.md` (84 such paragraphs, longest 8,314) and this design file. The 2026-10-11 acceptance of Protected View expected a derived `L_protect` in the tens of thousands; C0 could not derive it, and 500 is the stall basis.

**Risk the measurement must face honestly.** C0 already measured something close to `L_EDIT_STALL`'s definition: `L_protect_stall` (caret, typing and click stalls within `B_interact`, worst variant) is 500 / 710 idle-confirmed, and typing at the start of an unwrapped 2,000-character line stalled 50 ms. The coordinator expects `L_EDIT_STALL` in the tens or hundreds of thousands. Task 1.3 measures keystrokes alone, at the start, middle and end of the paragraph, wrap on and off, with C1's reduced features applied (syntax, bracket matching, search highlighting off above their thresholds), which C0's sweep did not; that may move the value. If the measured value is still near C0's 500 / 710, the decision's premise fails, and task 1.3 stops the change and returns the choice (view-only at that value, editable but over budget, or C5) to the maintainer, recorded in `docs/next/long-line-documents.md`. The value is never chosen by hand.

### D1. Fold placement is a pure, windowed policy applied at insertion by one cross-cutting executor
`model/protected_fold.rs` maps the sequence of paragraph lengths to an ordered list of `FoldRange { paragraph, head_chars, tail_chars }`. It is a streaming step function fed by C1's `ParagraphProfileAccumulator` in the same worker pass that profiles the body (a second consumer of the accumulator's closed lengths, beside C1's `ParagraphLengthFold`). It cannot run from `DocumentProfile` alone: the profile keeps only counts, the worst window, and at most `MAX_PROFILED_OFFSETS` long-paragraph offsets.

Two production stages:
1. **`length_cap`.** Every paragraph shows at most `head_cap` characters, where `head_cap = min(N_VISIBLE, N_HEAD, A28_SAFE_CHARS)`.
   - `N_HEAD` is new and measured by this change (task 1.3): the largest visible head whose **first layout**, wrap on and wrap off, and whose re-layout on a word-wrap toggle each fit `B_settle` per dispatch. `N_VISIBLE` (500,000) is a **scroll-only** threshold: C0 derived it from scroll stalls on an already laid-out paragraph, not from the open. A23's figures put a 500,000-character head's first layout at about 85–105 ms unwrapped and 440–540 ms wrapped, far over `B_settle`; extrapolating A23 linearly, `N_HEAD` is of the order of 15,000 characters wrapped. The lane measures it; the extrapolation is not used.
   - `A28_SAFE_CHARS` keeps every visible head below Pango's `int32` x-position overflow (238,319 characters at the measured advance), with the margin task 1.3 records for the smallest supported zoom. The first draft let heads reach `N_VISIBLE` = 500,000, past the overflow.
2. **`density_cap`.** A window is `R_fold` consecutive paragraphs (`R_fold` is C1's `R_BATCH`, plus one if C0's `R_batch` does not count the partially covered edge row of an unaligned 2000 px batch, task 1.2). A paragraph is **dense** when some window containing it has a length-capped sum over `V_BATCH`. A dense paragraph shows `min(c, N_DENSE)`, where `c` is its length-capped count; every other paragraph shows `c`. `N_DENSE = V_BATCH / R_BATCH` (250) is defined in `model/protected_fold.rs` from C1's constants.

**Soundness, for every sequence length.** Take any window `W`. If its length-capped sum is at most `V_BATCH`, its visible sum is at most that. Otherwise every paragraph of `W` is dense, so its visible sum is at most `R_fold · N_DENSE ≤ V_BATCH`. A 2000 px validation batch, in either wrap mode, covers at most `R_fold` consecutive paragraphs, because every paragraph is at least one row of at least the smallest row height; so it is contained in some window and holds at most `V_BATCH` visible characters. The plan therefore does not depend on the wrap mode, and toggling word wrap never re-folds.

**No gratuitous folds.** A paragraph of at most `head_cap` characters that lies in no over-budget window is never folded. A non-empty paragraph always keeps at least `min(len, N_DENSE) ≥ 1` characters.

**Why the first draft's excess budget was replaced.** It capped every paragraph at `N_DENSE + E` with `E = V_BATCH − R_BATCH · N_DENSE`. With C0's values `224 · 250 = 56,000`, so `E = 0` and every paragraph over 250 characters in every protected document was folded at 250, whatever its neighbours held. Under the first draft's entry at `L_PROTECT` = 500, a document protected by one 501-character line would have shown every line over 250 characters truncated; with any entry threshold it folds paragraphs that need no fold. The windowed rule folds only where a measured budget requires it. The first draft's hand-found counterexample to its earlier "fold when the window would overflow" rule (`V_batch = 10`, `R_batch = 2`, `N_dense = 5`, `N_visible = 8`, two 8-character paragraphs shown as 8 + 5 = 13) stays recorded (task 2.10); the windowed rule shows 5 + 5.

**Streaming state.** A paragraph's decision needs the windows that start up to `R_fold − 1` paragraphs after it, so the step emits paragraph `s` when paragraph `s + R_fold − 1` arrives (a lookahead of `R_fold − 1` lengths), and `finish` emits the rest; a document shorter than one window is one window. `FoldState` is a ring of the last `R_fold` capped lengths, their running sum, the index, and `dense_through` (the last paragraph index covered by a completed over-budget window). It is bounded by `R_fold`, independent of the document.

The plan crosses to GTK as one compact range per folded paragraph, charged to the install's transient admission. The policy is integer-only, carries `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`, and joins the whole-pixel table at ceiling 0. Its unit is characters; `R_fold` carries the pixel relationship.

`ui/protected_fold.rs` is the executor, with no role. For each folded paragraph an install turn inserts the head with `insert` and the tail with `insert_with_tags(fold_tag)` (`invisible=true`, `editable=false`), so no unfolded paragraph exists in a GtkTextView at any dispatch boundary. Install turns come from: load `execution.rs`; `buffer_replacement/execution.rs` (draft recovery, set-aside open, local-history restore and Undo Restore, eviction reload; save-formatting mirror-back never runs on a protected buffer, D9); monitor reload; reopen with encoding; and the local-history preview. The protected view properties (D3) are applied **before** the first byte of an install, so the `editable` change invalidates nothing (A37). A tail longer than the slice budget is inserted in one turn: that is the one declared exception, owned by C3 (D13).

*Alternatives:*
- Fold on idle after insertion. Rejected: a validation idle could lay out the unfolded paragraph.
- Head-hiding windows. Rejected: AT-SPI offsets would misalign.
- **Head-only buffer** (insert heads only and keep tails outside GtkTextBuffer, with an immutable source as the document of record). It would remove the tail's install, snapshot and AT costs. Rejected: the buffer would no longer be the document, so every path that reads the buffer as the document (save, drafts, local history, print, external-change handling, session offsets, statistics, Replace All's open-tab handling) would have to be rerouted, and one missed path would write a truncated file. With the tag fold a missed path costs time, never bytes. Recorded so C5 can revisit it.
- The first draft's excess budget. Rejected above.

### D2. Protected View state is a monotone per-editor session with one seam
`ProtectedViewTicket` is the editor's weak identity plus a protected-session generation. `ProtectedViewFacts` holds the reason (`ReductionReason::LongParagraph` or edit growth), the fold summary and the source state. Both are constructed once at entry (`seams.rs`), in the `*Ticket` + `*Facts` + `*_is_current` shape. Entry points:
- load and reload, from the installed body's `DocumentScale.protected_view`;
- draft recovery, set-aside open, local-history restore and Undo Restore, from the body profile C1 already attaches with `with_profile`;
- edit growth, from C1's watch: when the joined scale's `newly_reduced_presentation` contains `ScaleFeature::ProtectedView`, the watch calls `protected_view::enter_from_edit` in the same `insert-text` or `delete-range` after-handler emission.

The state is never cleared by edits, preference changes, tab switches, eviction reload, draft recovery or local-history restore; C1's `DocumentScale::join` is monotone for the same reason. Two exits exist:
- **Reload from disk** (external-change reload or explicit reload) re-derives it from the new profile, so a smaller file becomes editable again.
- **Entry undo** (coordinator decision, 2026-10-30). When the session was entered through an edit, the Protected View alert offers **Undo** and Ctrl+Z routes to it (`action_disposition(TextUndo) = RouteToEntryUndo` while it is available, `Preempt` otherwise). It is available only while the entry edit is still the newest step of the buffer's undo history and undo history is on (C1's undo tier); Protected View admits no other buffer change, and tag changes record no undo (A27), so after entry nothing can push a newer step. The undo runs `GtkSourceBuffer`'s own undo of that one step, restores the effective `DocumentScale` recorded by C1's watch immediately before the entry edit (the C1 MODIFIED requirement), ends the protected session (`SessionEvent::EntryUndo`, generation advanced), releases the captured source, removes the fold tags, and restores the editable properties. Its cost is the inverse of the entry edit: undoing a paste deletes the pasted run, hidden tail included, in one dispatch, the same declared one-turn envelope as a hidden-tail insert (D13), measured by the verdict; undoing a join re-inserts one delimiter. A document that entered on load, recovery or restore has no entry undo.

**Edit-growth entry.** In the same emission, before any validation idle runs: tag every folded tail the plan names for the edited window (tag application changes no text, A27), set the protected properties, and place the insert and selection-bound marks at the start of the paragraph that grew. The paste or join leaves the insert mark at the end of the inserted run, which can now lie inside a hidden tail, and GTK's post-insert `scroll_mark_onscreen` and visible-cursor iteration would then walk the hidden run (A26). This one placement happens at entry, before the session becomes caret-free; afterwards no LushText path moves either mark. The cost is the re-layout of heads of at most `head_cap`; a widget test proves the precondition.

### D3. Caret-free interaction: properties, an intercept, a capture gesture and key-binding pre-emption
`execution.rs` does the following on entry (before the first install byte for install entries):
1. Sets `editable=false` and `cursor-visible=false`.
2. Connects a `move-cursor` handler that calls the pure `policy::viewport_step(step, count, extend)`, scrolls the vadjustment and hadjustment, and stops the emission, which covers `gtk-keynav-use-caret` (gtktextview.c:6336-6343).
3. Installs a `GtkGestureClick` and a `GtkGestureDrag` in `PropagationPhase::Capture` on the view, for all buttons. The pure `policy::press_disposition(button, n_press, hit, device)` decides: fold-marker hit → inspector; secondary press → Protected View popover menu; touchscreen drag not on a marker → unclaimed (the scrolled window pans); anything else → claim only.
4. Disables the DnD drop target and drag source.
5. Pre-empts every default key binding that can move a mark or change the buffer, before its default handler runs. Per-instance `action_set_enabled(false)` is **not** the guard (GtkTextView recomputes it; Ctrl+A reaches a signal, not the action):
   - keybinding **signals** (GtkTextView `select-all`, `toggle-cursor-visible`, `cut-clipboard`, `copy-clipboard`, `paste-clipboard`, `backspace`, `delete-from-cursor`, `insert-at-cursor`, `toggle-overwrite`, `set-anchor`, `extend-selection`, `insert-emoji`; GtkSourceView `change-number`, `change-case`, `join-lines`, `move-to-matching-bracket`, `move-lines`, `move-words`, `show-completion`, `push-snippet`) get a handler that stops the emission. `change-number`, `change-case` and `join-lines` are critical: their default handlers change the buffer without checking `editable`;
   - binding **actions** (`menu.popup` on Shift+F10 and Menu, `text.undo`/`text.redo`, `source.change-case`) are claimed by a capture-phase key controller. The pure `policy::action_disposition` maps each to pre-empt, route-to-Protected-View-menu, route-to-entry-undo (`text.undo` only, while the entry undo is available, D2) or viewport-safe. The default undo handler is never admitted: the entry undo is LushText's own bounded path.

   A widget test walks the view's class shortcut controllers on the pinned toolkit and asserts that every bound action and signal has a disposition, so a binding a toolkit update adds fails the test rather than reaching the buffer. Each pre-empted request publishes the notice through the rate-limited lane.
6. Turns the minimap off for the session (D10).

Navigation uses a dedicated `protected-nav` mark with `scroll_to_mark`. The one path LushText cannot pre-empt is an assistive technology writing the caret or selection through `GtkAccessibleText`; P4 and the lane's AT case pin its cost, and `ProtectedViewEvidence` counts any insert-mark move it observes.

*Alternatives:* a non-focusable view (rejected: keyboard scrolling and AT access would be lost; it stays the fallback if P1–P6 or P4 fail); switching the IM module to simple (rejected: degrades CJK input everywhere).

### D4. The immutable text source is the install body, retained; only edit-growth entries capture
Find, inspector windows, whole-line copy and every save of a protected document read `ProtectedTextSource`: an `Arc<str>` of the document text plus a sparse index (paragraph start byte offsets, and a char→byte checkpoint every `K` characters inside folded paragraphs), built on the worker.

**Origin by entry kind.**
- **Whole-content install** (load, eviction reload, monitor reload, reopen with encoding, draft recovery, set-aside open, local-history restore and Undo Restore): the body the install inserts is already an owned, disposal-guarded `String` (the load's transient-permit body; the restore's accepted body that `new_guarded_returning_body_on_complete` hands back). The protected install **moves** it into the source at its `Complete` terminal instead of disposing of it. No GTK snapshot runs, ever.
- **Edit-growth entry**: the buffer holds text no worker has. The source is captured once, on first need, after entry (the buffer is immutable from then on). Because the chunked snapshot's per-chunk cost grows with depth inside one character segment (Context; P8), the capture reads chunks by **line and byte index** if P8 shows that bounded, and otherwise is recorded as a declared envelope owned by C3 with its measured per-chunk cost in the verdict. Either way at most one capture per edit-growth session.

**Ownership.** Once accepted the source is long-lived editor state: it releases its transient admission, holds a guaranteed handoff to plain disposal (the plain-data disposal rule), and is charged to the live-editor memory estimate. Undo history is irrelevant to an immutable buffer, so a protected install runs with the irreversible-action envelope and keeps no undo text; the memory charge of a protected editor is the buffer plus the source. An unmodified protected editor stays evictable, which releases both; reselection re-installs from disk and retains the new body. Every whole-content install advances the protected-session generation and releases the previous source.

*Alternatives:*
- Capture lazily through the existing chunked snapshot (the first draft). Rejected: quadratic on a giant single-segment paragraph, contradicting the first draft's own reason for not reading windows from the buffer.
- Re-read the file on first need. Rejected: wrong for recovered drafts and edit-growth entries, and an extra identity race for loads.

### D5. Worker find replaces GtkSourceSearchContext in Protected View
The in-tab search bar gains a protected mode, chosen at attach time: `ui/editor_page/search.rs` skips `search_bar.attach(&buffer, view)` when protected and calls `protected_view::find_execution`; the entry, counter and next/previous buttons are reused; the replace row is hidden.

Matching reuses the search bar's option model (case, regex, whole-word), the `regex` crate and the `grep-*` primitives, line-oriented through C1's `next_paragraph_break`, so CR-only and U+2029 are correct. It searches hidden text too, unlike the stock context (A33). Single-flight follows `services/single_flight.rs`. Batches are bounded by `FIND_RESULTS_PER_TURN` and `FIND_BYTES_PER_TURN` under `FIND_RESULT_CAP` (initial 256 / 64 KiB / 10,000, confirmed by the verdict). The current match is highlighted with one `protected-find-current` tag only when it lies entirely in visible text; otherwise activation opens the inspector. The `editor-search` readiness blocker is held while a protected request is active. Because the source exists from install (D4), find has no first-use capture except after an edit-growth entry.

Both prefill sites move to one helper that reads `visible=true` text under `MAX_SEARCH_SELECTION_PREFILL_CHARS`. In Protected View no selection exists, so it returns none.

### D6. Line Inspector is an AdwDialog called presentation surface
`inspector.rs` is a Blueprint template: an `AdwDialog` with a header (line number, column range, paragraph length); a read-only `GtkTextView` with wrap on, a visible caret and selection allowed (its buffer is at most `N_inspect ≤ 500` characters, within the `L_protect_stall` basis, so caret, IM and AT paths are bounded); Previous and Next, a Go to Column `GtkSpinButton`, Copy Window and Copy Whole Line. `inspector_execution.rs` reads windows on a worker from the source (D4) through the char checkpoints, generation-guarded; only the latest installs. Windows cut on character boundaries.

Copy Whole Line installs a `GdkContentProvider` subclass whose `write_mime_type_future` writes the paragraph slice of the `Arc<str>` as bounded asynchronous chunk writes (`COPY_CHUNK_BYTES`). `gio::OutputStream` is not `Send`, so the writes run on the GTK main context, one chunk per dispatch; the line is never materialized whole on GTK. The provider's `Arc` clone is long-lived ownership, charged as such (Risks).

### D7. Navigation targets are classified by a pure function
`policy::navigation_target(paragraph, column, fold)` returns `ScrollTo { paragraph }` or `Inspect { paragraph, window_start }`. Every navigation entry point calls it: restore position, bookmark activation, local-history targets, workspace-search hit activation (`open_file_at_line`), protected find results.

### D8. Fold marker gutter
`fold_gutter.rs` is a `GtkSourceGutterRendererText` subclass enabled only in Protected View. It queries the fold ranges by line through a `BTreeMap` (O(visible rows · log folds)) and shows a symbolic marker with a tooltip and accessible label giving the hidden count. Presses on it go through the capture gesture (D3).

### D9. Save path and data integrity: every save writes the source
`save/` gains a `ProtectedSaveMode` fact:
- **Save** is disabled while the document is unmodified.
- **Every** save of a protected document (Save, Save As, close-with-changes) writes `ProtectedTextSource` on the worker through the existing durable writer, with the recorded encoding and BOM state, and with every save-time transformation suppressed (EditorConfig formatting and its mirror-back, line-ending normalization). No GTK snapshot runs. Because the buffer is immutable and the source was taken from exactly what was installed (or captured once after entry), the source equals `buffer.text(…, true)`; a debug assertion in the widget tests checks it on every fixture.
- `Mixed` line endings are written verbatim instead of refused, because normalization is unavailable and refusing would leave a recovered or edit-growth document unsaveable.

C1's `DocumentProfile` does not report decode round-trip exactness, so C2 adds it to `LoadMetadata`: for an encoding whose error-free decode is not injective, the load worker re-encodes and compares with the source bytes. When the decode was not exact, Save As of an unmodified protected document is refused with a notice.

A byte-identity integration test round-trips synthetic fixtures: UTF-8 with and without BOM, CRLF, CR-only, mixed endings, U+2029, a 1.9M-character paragraph, and a recovered 32 MiB single-line draft.

### D10. Surfaces gated by the Protected View fact
Each surface asks `ProtectedViewFacts`, and otherwise follows C1's `DocumentScale`:
- **Syntax and bracket matching** follow C1 (`L_SYNTAX` 2,000). With `L_EDIT_STALL` expected above `L_SYNTAX`, every protected document has syntax off by C1; if the measurement places it below, C1's gate still decides. The first draft's precondition "no language attached" is withdrawn as a Protected View rule.
- **Minimap**: off for the whole Protected View session, whatever C1 allows. `GtkSourceMap` is a second `GtkSourceView` over the same buffer, with its own gestures and accessible text; keeping it would duplicate D3 and P1–P6 on a second view.
- **Invisibles** (`invisibles.rs`): forced off while any fold exists.
- **Search highlighting**: no `GtkSourceSearchContext` at all (D5).
- **Print** (`window/print/policy.rs`): `PrintOutcome::RefusedProtectedView`.
- **Local-history preview** (`preview_execution.rs`): installs through the executor, and the preview view is itself caret-free (a click in a 1.9M-character preview paragraph would pay A26).
- **Markdown preview admission**: its existing paragraph and slice budgets produce `Simplified` or limited; a widget test confirms it.
- **Notes editors**: read-only plus notice when the body profile is protected (above `L_EDIT_STALL`).
- **File peek**: a test confirms the sample's longest visible line is at most `head_cap`.
- **Bookmark toggle** and **mixed-line-ending normalization**: disabled.
- **Automation**: projects from `evidence.rs`.

### D11. Axioms beyond C0 and C1
Added to the ledger with `gtk-lush-axioms` probes and samples, numbered after the last applied entry (A51 once C4c lands; provisional names P1–P8, which `sliced-protected-install` cites):
- P1: a capture-phase claim prevents insert-mark motion, IM reset, middle-click primary paste, the default context menu and drag start; an unclaimed touchscreen drag scrolls without moving a mark;
- P2: non-editable means no IM focus-in and no key-controller IM;
- P3: a `move-cursor` intercept holds under `gtk-keynav-use-caret` (extends A34);
- P4 (lane-pinned like A35): the AT-SPI text and offsets of a folded paragraph; the payload and cost of `GetStringAtOffset`/`GetText` at character, word, sentence and line granularity on it (word and sentence granularity read log attrs, A26); the `TextChanged` payload when a hidden tail is inserted; and the cost of an AT-initiated caret or selection write into it;
- P5: DnD disablement;
- P6: per-instance action enablement is recomputed by GtkTextView and is not a guard, while a stopped keybinding signal or a capture-phase key claim keeps `select-all`, `toggle-cursor-visible`, `menu.popup`, `change-number`, `change-case`, `join-lines` and `move-to-matching-bracket` from their default handlers;
- P7: `insert_with_tags` of an invisible run never lays out the run (structural), and its cost is linear in the run (recorded per MiB as the input to C3's gate);
- P8 (cost evidence, like A36): a buffer snapshot chunk of fixed size at depth `d` inside one character segment costs time growing with `d` when positioned by character offset (`iter_at_mark` + `forward_chars`); recorded also for line-and-byte-index positioning, which D4 uses if it is flat.

Run `make gtk-axioms` first. A falsified axiom blocks the dependent task (pre-existing-blockers rule).

### D12. Kani proofs of the Protected View preconditions
Every responsiveness claim here assumes the density bound, and every integrity claim assumes the admission table and monotone entry, so both are Kani-proved over the production pure cores (`formal-verification-kani`). Every harness has a `kani::cover!` on each interesting branch; each restricted domain is stated in the harness and in the checked function's rustdoc. Both modules are GTK-free and I/O-free.

**Refactors so the cores are pure and checkable** (part of tasks 2 and 4):
- **`model/protected_fold.rs`**
  - `FoldThresholds { head_cap, n_dense, v_batch, r_fold }`, built by `FoldThresholds::new`, which returns `None` unless `n_dense ≥ 1`, `n_dense ≤ head_cap`, `r_fold ≥ 1` and `r_fold · n_dense ≤ v_batch` (checked multiplication). `FoldThresholds::PRODUCTION` is built in a `const` that fails to compile if C1's constants, `N_HEAD` or `A28_SAFE_CHARS` break them. The worker passes `PRODUCTION`; the harnesses pass symbolic tuples.
  - `fold_step(state, paragraph_chars) -> (Option<FoldRange>, state)` and `fold_finish(state)` are composed of the two named production stages, `length_cap` and `density_cap`; `window_is_over_budget(sum, v_batch)` and `dense_visible(c, n_dense)` are the per-window decisions. The `should_panic` harness calls `length_cap` alone. No test-only switch exists.
  - `FoldRange::locate(column) -> FoldLocation { Visible(col) | Hidden(tail_offset) | PastEnd }`, `FoldLocation::column_of`, and `fold_byte_offset(paragraph, head_chars) -> usize`.
- **`ui/editor_page/protected_view/policy.rs`**
  - `ProtectedAction` (closed enum), `toolkit_effect`, `action_disposition` (`ViewportSafe | Preempt | RouteToMenu | RouteToEntryUndo`; only `ViewportSafe` admits the default handler).
  - `ProtectedSession { protected, entered_by_edit, generation }` and `on_event(SessionEvent)`, whose events include `EntryUndo { is_entry_step }`; `execution.rs` stores the session value and only replaces it with `on_event`'s result.
  - `inspector_window`, `inspector_window_count`, `inspector_window_containing` (loop-free arithmetic), `navigation_target`, `admit_find_record`, `next_find_batch` (takes `AdmittedRecord`s only).

**Row-window assumption.** A 2000 px batch starting at an arbitrary pixel can cover one more, partial, row. Task 1.2 confirms whether C0's `R_batch` (224 = 112 rows at the default font, doubled for the 50 % minimum zoom) counts that edge row; if not, `R_fold = R_BATCH + 1`, stated in the rustdoc and the programme record. The widget test of task 3.4 checks that every 2000 px window of the laid-out dense fixture holds at most `V_BATCH` visible characters in both wrap modes, which ties the "one row per paragraph at least" lemma to the toolkit.

**Harnesses in `crates/lushtext-core/src/model/protected_fold/kani_proofs.rs`** (shard `core-protected-fold`, gate `pull-request`):

| Id | Harness | Checks | Domain and why it is enough |
| --- | --- | --- | --- |
| H1 | `density_bound_holds_for_every_window` | Folding a sequence by `fold_step`/`fold_finish`, every window of `r_fold` consecutive paragraphs (the whole sequence when shorter) holds at most `v_batch` visible characters, and every paragraph shows at most `head_cap`. | Up to 6 paragraphs (`#[kani::unwind(7)]`), each any `u64` length; symbolic thresholds that satisfy `FoldThresholds::new`, `r_fold` 1 to 3. Window sums in `u128`. Six paragraphs with `r_fold ≤ 3` reach sequences shorter than a window, warm-up, full and evicting windows, and two separate dense runs. |
| H1w | `an_over_budget_window_fits_once_dense` | The window lemma the soundness argument uses: for any `r_fold` capped lengths, if `window_is_over_budget` then the sum of `dense_visible` over them is at most `v_batch`; otherwise the capped sum is. | Loop over at most 3 lengths (`#[kani::unwind(4)]`), any `u64`. Together with H1 and the three-line argument in D1 (stated in `fold_step`'s rustdoc), the bound extends to sequences of any length and to row windows in either wrap mode. |
| H1c | `production_thresholds_satisfy_the_preconditions` | `FoldThresholds::PRODUCTION` passes `new`; `head_cap ≤ min(N_VISIBLE, N_HEAD, A28_SAFE_CHARS)`; `N_INSPECT ≤ min(head_cap, L_PROTECT)`. | Concrete. |
| H1x | `density_bound_fails_without_the_density_fold` (`should_panic`) | `length_cap` alone, the H1 window assertion. | The H1 domain. H1 proves the same stages free of overflow and panics, so the only failure H1x can reach is the window assertion. |
| H2 | `folds_hide_only_tails_and_only_where_needed` | `head + tail = len`; a non-empty paragraph keeps `head ≥ min(len, n_dense) ≥ 1`; a paragraph of at most `head_cap` characters in no over-budget window is not folded; the range ends before the delimiter (lengths exclude delimiters, C1); ranges are ordered and disjoint. | The H1 sequences. |
| H2b | `fold_byte_offset_never_splits_a_scalar` | `is_char_boundary(offset)` and `prefix.chars().count() == head_chars`. | Every valid UTF-8 string up to 8 bytes (`#[kani::unwind(9)]`) with no delimiter, any `head_chars` up to its character count. |
| H2c | `fold_location_is_total_and_round_trips` | `locate` is total over `u64` columns; Visible iff column < head; Hidden offset + head = column; `column_of(locate(c)) = c` for `c ≤ len`; `PastEnd` iff `c > len`. | Any `u64` length, any valid range, any `u64` column; loop-free. |
| H2s | `fold_plan_is_independent_of_install_slicing` | One run equals two runs split at any index with the state carried across. | Up to 6 paragraphs (`#[kani::unwind(7)]`), any split index, the H1 thresholds. |

**Harnesses in `crates/lushtext-core/src/ui/editor_page/protected_view/policy/kani_proofs.rs`** (shard `core-protected-view`, gate `scheduled`), unchanged from the first draft:

| Id | Harness | Checks | Domain |
| --- | --- | --- | --- |
| H3 | `no_mutating_action_is_admitted` | No `ProtectedAction` whose `toolkit_effect` mutates the buffer or moves a mark is `ViewportSafe`; an independently written list in the harness (insert-at-cursor, delete-from-cursor, backspace, cut, paste, middle-click paste, move-lines, move-words, join-lines, change-case, change-number, insert-emoji, push-snippet, undo, redo, DnD drop, the search bar's replace and replace-all, line-ending normalization) is recorded as mutating and not admitted. | The whole closed enum (`cfg(kani)` `Arbitrary`), loop-free. Completeness against the toolkit is the widget enumeration test's job. |
| H3m | `protected_view_entry_is_monotone` | A protected session stays protected for every event except `ReloadFromDisk`, which returns editable only when its `protected` is false, and `EntryUndo`, which returns editable only when the session was entered by an edit and `is_entry_step` holds; the generation never decreases and advances on every whole-content install and every exit. | Start generation ≤ `u64::MAX − 6`, up to 6 events (`#[kani::unwind(7)]`). |
| H3x | `entry_is_not_monotone_if_eviction_reload_re_derives` (`should_panic`) | H3m with eviction reload mapped to the from-disk re-derivation. |
| H3u | `entry_undo_after_load_entry_breaks_monotonicity` (`should_panic`) | H3m with `EntryUndo` accepted for a session entered on load. | The H3m domain; pins the `entered_by_edit` guard. | The H3m domain. |
| H4 | `inspector_windows_are_bounded_and_tile` | `1 ≤ len_i ≤ n_inspect`, adjacency, ends, and `inspector_window_containing`. | Any `u64` length ≥ 1, `n_inspect` 1 to the bound, any index and column; loop-free. |
| H4n | `navigation_target_reaches_every_column` | `ScrollTo` exactly for visible columns, `Inspect` with a containing window otherwise. | The H2c and H4 domains. |
| H4f | `find_batches_are_bounded_and_partition_results` | Batches of 1 to `results_per_turn` records within `bytes_per_turn`, concatenating to the first `min(n, result_cap)` admitted records in order, in at most `n` turns. | Up to 6 records, caps 1–3 and 1–6 (`#[kani::unwind(7)]`). |
| H4x | `find_batch_exceeds_the_byte_bound_without_record_admission` (`should_panic`) | The H4f byte assertion over a record built without `admit_find_record`. | One oversized record (`#[kani::unwind(2)]`). |

**Unwind bounds.** Each harness declares the bound in its row; loop-free harnesses (H1c, H2c, H3, H4, H4n) declare none. The production functions under proof are loop-free except `fold_byte_offset` (bounded by the string), `next_find_batch` (bounded by the per-turn count cap), and the ring update in `fold_step`, which touches O(1) entries per step (one evicted, one written), so no harness unwinds a loop of length `r_fold`.

**Shards and budgets.** `core-protected-fold` is gated `pull-request` if its runner-measured wall time is at most 15 minutes; otherwise it is split, with H1, H1w, H1c and H1x PR-gated. `core-protected-view` is `scheduled`. Both get `ci_minutes`, `ci_peak_gib` and `measured_in` from two dispatched runs (the larger), within 25 min and 12 GiB. **Oracles:** `ORACLES` gains both modules, harnesses cheapest first with measured local seconds and tier; `make proof-strength` runs once per module and its survivors are triaged in `docs/next/formal-verification.md`.

### D13. Recovery of giant-paragraph drafts: C2 owns the stall, C3 owns the residual one-turn insert
**Finding (2026-10-29).** The crash-recovery smoke fails on a slow CI runner with `recovery-restore-complete blocked by draft-autosave`. Its lazy-aggregate fixture (`expand_accepted_drafts_for_lazy_restore`) appends spaces, without a delimiter, to two accepted drafts until each is 32 MiB + 1 byte, so each recovered body ends in one paragraph of about 33.5 million characters. Recovering them is a CPU-bound main-thread stall of 31–53 s in the debug build at 0.55–2 CPUs. `draft-autosave` covers recovery resolution, so the predicate waits for the installs.

**Cause.** The cost is GTK's, not LushText's debug code: the stall is the layout of a giant visible paragraph. On `main` the bounded replacement slices that paragraph every 256 KiB, so each slice re-lays out the growing paragraph (quadratic; C1's `next_replacement_boundary` documents it). With C1 the paragraph installs in one turn and is then laid out whole when its tab is shown. A23 puts a whole layout at 34–42 ms per 200,000 characters unwrapped and 177–217 ms wrapped, so 33.5 million characters cost about 6–7 s unwrapped and 30–36 s wrapped per layout, past Pango's `int32` overflow (A28), before AT-SPI `TextChanged` payloads (A35) when the smoke's AT bridge is on.

**Ownership: C2.** C1 already profiles the draft body on the restore worker and passes it with `with_profile`; the profile marks it protected. With this change:
1. the restore installs the body through the fold executor: the head (at most `head_cap`) visible, the tail inserted hidden in the same dispatch, so the tail is never laid out (A24, P7);
2. the recovered editor enters Protected View modified and draft-restored. Its persisted draft body is exactly its buffer, so autosave must write nothing for it. Today's restore terminal (`finish_applied_draft`) sets `modified` but not `draft_dirty`, so autosave already skips it; this change keeps that, adds a test that no snapshot or body write runs for a recovered protected editor, and makes the buffer immutable so nothing can mark it draft-dirty later. The body is moved into `ProtectedTextSource` (D4), so a later Save, Save As or close-with-changes writes it from the worker;
3. local history is unaffected (above 10 MB it is save-boundary only, `HISTORY_SAVE_ONLY_BYTES`).

**Residual: C3.** What remains is the single GTK turn that inserts each hidden tail: copying 32 MiB into the btree (A36 says the merge copies both strings), GTK's UTF-8 validation, and the AT `TextChanged` payload. P7 records its cost per MiB; the verdict measures the recovery case with the AT bridge off and on. That turn is the declared one-turn envelope that `sliced-protected-install` (C3) exists to remove; this change hands C3 the recovery measurement as an input to C3's adoption gate (C3's gate as written covers only 100/250/500 MB loads; Open Question 4).

**The 60 s smoke limit is not raised.** The smoke records the time to `recovery-restore-complete` and both tabs' `protected_view` in its artifacts, and must pass with the existing limit on the CI runner.

### D14. The verdict is measured under the quiet gate
The budget verdict (tasks 9.3–9.4) uses the lane as it now stands (presented frames, frameless runs failing, PR #66) and the coordinator's reference precondition, modelled on `.claude/handoffs/c4c-quiet-gate.sh` and its measurement wrapper:
- the 300 s quiet gate before taking the shared measurement lock (`target/responsiveness-measure.lock`), a 30 s re-check inside it, a sampler for the whole run that records any violation outside the lane's own process tree, a 60 s check afterwards, and `btrfs-stall-report` for the run's window; a run that breaks the precondition is discarded and retried, never kept;
- main's release responsiveness binary and the branch's are measured by the branch's lane driver, each in its own lock hold;
- **7 repetitions** of each case, the lane's three variants (plain, AT, probe IM), CPUs 8–23, `--tier full`;
- cases that start a process (open, three-document restore, draft recovery) also run in the **four construction profiles** (fresh or steady-state, keyfile or dconf) that C4 added, so construction stays attributed to C4/C4b and not to C2;
- the protected cases: open, scroll, Page Down, caret keys (scroll), typing (no-op), click and double-click, find (first and repeated), inspector open and navigation, Copy Whole Line, Save As, tab switch, word-wrap toggle, eviction and reselection, restore of three protected documents, recovery of two 32 MiB single-line drafts, and the AT bridge with a listener. Fixtures: C0's 2k/10k/50k/250k/1.9M paragraphs, the dense wrap-off fixture, CR-only, U+2029, a prose fixture of 600- to 8,000-character lines and a dense short-line fixture, both of which must stay editable (D0), and the `L_EDIT_STALL` sweep of task 1.3.

The verdict records, per case and phase, the largest interaction and settle stall against `B_INTERACT_US`/`B_SETTLE_US`, the input-to-frame latency against two reference frames, the one-turn hidden-tail insert as the declared envelope (C3's gate input), the run ids, the precondition evidence and the toolkit versions, in `docs/next/long-line-documents.md`.

## Risks / Trade-offs

- **[`L_EDIT_STALL` measures near C0's 500 / 710]** (D0) → the change stops at task 1.3 and the maintainer chooses; no value is chosen by hand.
- **[Dense short-line documents stay editable]** (D0) → documents whose windows exceed `V_BATCH` but whose paragraphs are at most `L_EDIT_STALL` keep today's behaviour under C1's reduced features, including validation batches that C0 measured over `B_settle` past dense 500-character lines; that cost has no owner yet (Open Question 11).
- **[An unprobed toolkit path still reaches the whole paragraph]** (AT word and sentence queries, AT caret writes) → P4 on the lane's AT case; if it exceeds `B_interact`, the fallback is a non-focusable main view whose accessible description points to the Line Inspector as the reading path, documented in `docs/accessibility.md` as a known limitation.
- **[Source retention doubles a protected tab's memory]** → charged to the live-editor estimate, released on eviction; unmodified protected tabs stay evictable. A protected active tab near `REFUSE_TO_OPEN` makes the budget soft, as any protected active page does today.
- **[Edit-growth entry with a modified buffer]** → saving and drafts keep working from the source; `data-safety` runs explicitly on this change. Whether the user can undo the edit that caused entry is Open Question 3.
- **[The one-turn hidden-tail insert exceeds `B_settle`]** → declared envelope; C3 is conditional on this change's verdict (D13).
- **[Fold-marker gutter adds per-frame work]** → bounded by visible rows, enabled only in Protected View.
- **[Clipboard providers outlive the editor]** → the provider's `Arc` clone is long-lived ownership (plain-data disposal rule), not transient admission.
- **[Touch panning]** → touchscreen drags not on a marker stay unclaimed; a widget test checks that a touch drag scrolls and leaves the marks unmoved; P1 covers it.
- **[`R_BATCH` doubles for the 50 % zoom]** → the fold is conservative by about two at the default zoom; re-folding on zoom would need tag changes on every paragraph and is not done.

## Migration Plan

- No persisted format change and no GSettings migration.
- The feature lands behind C1's `DocumentScale.protected_view`, so documents below the thresholds are unaffected.
- **Rollback:** revert the change. C1's degraded mode and its reduced-features notice remain as the floor.

## Open Questions

1. **Entry threshold — resolved 2026-10-30 (coordinator; the maintainer may override):** not `L_PROTECT` = 500 but the measured `L_EDIT_STALL` (D0). If the measurement lands near C0's 500 / 710, the choice returns to the maintainer (task 1.3).
2. **Copy Whole Line** (origin proposal decision 5): ship the lazy provider (this design) or bounded windows only.
3. **Backing out of an edit-growth entry — resolved 2026-10-30 (coordinator):** yes, one bounded entry undo from the alert and Ctrl+Z (D2).
4. **C3's adoption gate — resolved 2026-10-30 (coordinator):** C3's proposal now names the 32 MiB draft-recovery case (two drafts, AT off and on) as a gate input, recorded by this change's verdict.
5. **`L_protect` as a GSettings key** — resolved 2026-10-11: a fixed constant.
6. **Bookmark-by-line-number in Protected View**: out of scope.
7. **End-of-head fold label** (`⋯ +N` in a `snapshot_layer`): bounded by visible rows; out of scope unless the Orca walk asks for it.
8. **A dedicated readiness blocker** for fold settle, only if the verdict shows the `file-load` blocker does not cover it. Inspector window reads need a blocker in any case: reuse one or add `line-inspector`.
9. **Grapheme-boundary folds**: whether a character boundary with a documented split-cluster artifact is accepted (C1's accumulator does not supply grapheme boundaries).
10. **Save-time transformations on edit-growth documents**: this design suppresses them for every save of a protected document; applying them to the written bytes only would make the file differ from the buffer the user sees.
11. **Dense short-line documents**: with density no longer entering Protected View, who owns the validation cost of editable documents whose windows exceed `V_BATCH` (C0 measured 21.8 ms against 15.4 ms post-open at dense 710)? A candidate for C6 or its own change.

## Adversarial review (2026-10-29)

| # | Finding | Verdict | Change |
| --- | --- | --- | --- |
| 1 | The draft re-defined C1's entry, profile and hook under its own names | confirmed (stale) | Context and D2 use C1's API by name; entry reads `DocumentScale.protected_view` only |
| 2 | With C0's values `E = V_batch − R_batch·N_dense = 0`, the excess budget folds every paragraph over 250 characters in every protected document | confirmed | D1: windowed density rule; H1/H1w/H2 restated |
| 3 | `N_visible` is a scroll threshold; a 500,000-character head's first layout is ~85–540 ms (A23), and it passes Pango's `int32` overflow (A28) | confirmed | D1: `head_cap = min(N_VISIBLE, N_HEAD, A28_SAFE_CHARS)`; `N_HEAD` measured (task 1.3) |
| 4 | The source captured through the chunked snapshot is quadratic on a single-segment paragraph, contradicting the draft's own D4 alternative | confirmed by source reading; P8 pins it | D4: source is the retained install body; one capture only after edit-growth entry |
| 5 | Saves of protected documents snapshot the buffer on GTK | confirmed | D9: every protected save writes the source on the worker |
| 6 | The 32 MiB draft-recovery stall | confirmed; cause is layout of the giant visible paragraph | D13: owned by C2; residual one-turn insert to C3 |
| 7 | `L_PROTECT` = 500 makes ordinary prose view-only | confirmed | D0: entry re-keyed to the measured `L_EDIT_STALL` (coordinator, 2026-10-30); density no longer enters |
| 8 | "No language attached" is wrong once `L_PROTECT` < `L_SYNTAX` | confirmed | D10: syntax follows C1; minimap off in Protected View instead |
| 9 | Setting `editable` invalidates every line (A37) | confirmed | D1/D3: properties set before the first install byte; edit-growth relies on `frame_validation.rs` |
| 10 | Edit-growth leaves the insert mark inside a hidden tail | confirmed | D2: marks placed at the paragraph start in the entry emission |
| 11 | Keyboard, click, IM, DnD and key-binding paths (D3) | refuted as gaps: the first draft's interception holds against the re-checked sources | unchanged; P1–P6 still gate it |
| 12 | AT word/sentence queries read whole-paragraph log attrs | open risk | P4 extended; non-focusable fallback stated |
| 13 | C3's conditional trigger and C5's exploration | confirmed as the right split: C3 removes only the one-turn hidden-tail insert, C5 only editing | C3 gets the recovery case as a gate input (Open Question 4) |
| 14 | Verdict measurability | the draft predated the quiet gate and frame-correct lane | D14 |
