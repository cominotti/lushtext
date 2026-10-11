## Context

This is C2 of the long-line programme in `docs/next/long-line-responsiveness.md`.
- **C0** (`probe-long-line-toolkit-costs`) supplies the threshold values, the policy budgets, the synthetic fixture generator, the in-process watchdog, `make responsiveness-smoke`, and axioms A23–A36.
- **C1** (`document-scale-policy`) supplies:
  - the worker-side profile in `LoadMetadata`;
  - one GTK-matching paragraph-boundary function (`\n`, `\r`, `\r\n`, U+2029);
  - `DocumentScale`, which replaces `FileSizeCheck` and exposes `protected_view`;
  - the synchronous edit-time hook (`connect_after("insert-text")`);
  - the minimap and syntax gates;
  - the reduced-features notice.

After C1, a giant-paragraph document no longer hits the GtkSourceView context engine or the minimap. Pango still lays out each giant paragraph whole. GtkTextBuffer still pays whole-paragraph costs on caret, word and IM paths, which ignore invisibility:
- log attrs over the whole paragraph, hidden text included, invalidated by every char change (gtktextbuffer.c:4469-4503, 4532-4537);
- IM retrieve-surrounding (gtktextview.c:8996-9059);
- visible-cursor iteration across hidden runs;
- AT-SPI offsets;
- the GtkSourceSearchContext (25–28 s on the reference file).

Maintainer decisions (2026-10-11):
1. Protected View now. Editing giant paragraphs is deferred to optional C5.
2. The syntax total-size cutoff is decided by C0. It is not used by C2.
3. Budgets are stricter than the draft: `B_interact = 8 ms` per interaction dispatch, input-to-presented-frame at most 2 frames, and `B_settle = 16 ms` per settle dispatch. These are working values that C0 confirms.

Source facts re-checked for this design (GTK 4.22.5 sources under `target/huge-file-explore/src/gtk/gtk/`):
- `gtk_text_view_set_editable(FALSE)` resets the IM, focuses it out, and sets the key controller's IM context to NULL (gtktextview.c:3460-3475). `focus_in` calls `gtk_im_context_focus_in` only when editable (:6044-6046). So a non-editable view does no IM filtering on keys. The click gesture still forces `need_im_reset` and resets the IM on every `n_press == 1` (:5850-5855). Middle-click calls `gtk_text_buffer_paste_clipboard` with `default_editable = priv->editable` (:5865-5875). Context-menu presses call `do_popup` (:5857-5862). The capture-phase claim therefore stays necessary.
- The two selection-prefill sites: `ui/editor_page/search.rs:82` reads `text(…, true)`, and `ui/window/search.rs:593` reads `text(…, false)` (workspace search panel).
- `ui/buffer_snapshot.rs:630` (direct) and `:975` (chunked) read `text(…, true)`, so save and draft snapshots include hidden tails.
- The action catalogue has `win.print` (Ctrl+P), `win.begin-replace` (Ctrl+H), `win.cycle-invisible-characters` (Ctrl+Shift+I), `win.toggle-bookmark` and `win.notes-toggle-bookmark`. There is no go-to-line action today.
- `AutomationTabSnapshot` (`model/automation.rs:776`) is the per-tab snapshot. `file-load` and `editor-search` are readiness **blockers** (`docs/automation-reference.md`); the predicates that wait on them are `file-open-complete` / `session-restore-complete` and `search-complete`.
- GtkTextView recomputes the per-instance enablement of `clipboard.cut/copy/paste`, `selection.delete` and `selection.select-all` on every popup and selection change (gtktextview.c:9317-9348). Ctrl+A is bound to the `select-all` **signal** (gtktextview.c:1835-1864), whose handler calls `gtk_text_buffer_select_range` whatever `editable` says (:9206-9224). F7 is bound to `toggle-cursor-visible` (:2046-2048). Shift+F10 and Menu are bound to `menu.popup` (:1683-1690).
- GtkSourceView's `change-number` (Ctrl+Shift+A / Ctrl+Shift+X, gtksourceview.c:1276-1288), `change-case` and `join-lines` change the buffer with non-interactive `gtk_text_buffer_delete`/`insert` and never check `editable` (gtksourceview.c:434-560). `move-to-matching-bracket` (Ctrl+%) calls `gtk_text_buffer_place_cursor` (:399-431). Only `move-lines` and `move-words` check `editable` (:3831, :4164).
- The loader refuses decodes with errors (`services/editor_io.rs:509-515`). The ordinary save path refuses `Mixed` line endings (`EditorSaveError::MixedLineEndings`, `services/editor_io.rs:1416-1423`).
- GtkTextView implements `GtkAccessibleText` `set_caret_position` and `set_selection` (gtktextview.c:11074-11119), so an assistive technology can move the insert mark.

## Goals / Non-Goals

**Goals:**
- Every interaction offered on a protected document stays within `B_interact` and 2 frames, and every settle stays within `B_settle`, up to the C0 install bound. C3 covers single-paragraph install if C0's lane, run against a build with this change applied, measures it above budget (C3's adoption gate).
- Data integrity is absolute:
  - the buffer is immutable while protected;
  - Save As is byte-identical;
  - no transformation runs on save;
  - a lossy decode is refused, not rewritten.
- Content stays reachable: the Line Inspector, find, whole-line copy and navigation into hidden tails.
- The work is covered by the conventions: a WFR role home, pure policy with Kani, axioms with probes and samples, automation fields, and accessibility docs with an Orca entry.

**Non-Goals:**
- Editing inside paragraphs over `L_protect` (C5).
- Revealing a whole giant line in the main view.
- Head-hiding.
- Fold-aware printing.
- A custom text widget.
- Sliced single-paragraph install (C3, conditional).
- The window-construction stall (C4).
- Changing `REFUSE_TO_OPEN`.
- Depending on upstream fixes.

## Decisions

### D1. Fold placement is a pure model policy, applied at insertion by one cross-cutting executor
`model/protected_fold.rs` maps the sequence of paragraph lengths, plus `N_visible`, `N_dense` and `V_batch`, to an ordered list of `FoldRange { paragraph, head_chars, tail_chars }`. It is a streaming step function (`fold_step(state, paragraph_chars) -> (Option<FoldRange>, state)`) whose state carries the running density window, so the worker that prepares the body runs it over every paragraph in the same pass, with C1's paragraph-boundary function. It cannot run from C1's `DocumentProfile` alone: the profile keeps only per-threshold counts and a bounded, possibly truncated, list of long-paragraph offsets, and density folding needs every paragraph's length. The plan crosses to GTK as one compact range per folded paragraph, and its size is charged to the install's transient admission. Density folding is greedy and forward, by an **excess budget**. The window is C1's `R_batch` consecutive paragraphs (the 2000 px batch of A31 divided by the smallest row height). Every paragraph may show up to `N_dense` characters unconditionally. The characters it shows beyond `N_dense` are its *excess*, and the excess of any `R_batch` consecutive paragraphs is limited to `E = V_batch − R_batch·N_dense`. So a paragraph shows `min(len, N_visible, N_dense + (E − excess of the previous R_batch − 1 paragraphs))` characters. Then every window holds at most `R_batch·N_dense + E = V_batch` visible characters, and every non-empty head keeps at least `min(len, N_dense) ≥ 1` characters. The state is a ring of the last `R_batch − 1` excess values plus their running sum, so it is bounded by `R_batch`.

The earlier draft rule, "fold the tail beyond `N_dense` when adding the paragraph would push the window over `V_batch`", is unsound. With `V_batch = 10`, `R_batch = 2`, `N_dense = 5`, `N_visible = 8` and two paragraphs of 8 characters, it shows 8 + 5 = 13. That counterexample was found by hand while writing the harness statement. Task 2.9 records it in `docs/next/formal-verification.md`.

**Consequence of the excess budget with C0's `N_dense`.** C0 defines `N_dense = V_batch ÷ R_batch` (integer division), so `E = V_batch mod R_batch < R_batch`. With those values every paragraph of a protected document is capped at `N_dense + E < N_dense + R_batch` characters, whatever its neighbours hold: density folding is not "only when the window needs it", it applies to every paragraph over that cap. This is not an artefact of the budget. Any forward, streaming rule that guarantees every later non-empty head `min(len, N_dense)` characters must keep `(R_batch − 1)·N_dense` free for the next paragraphs of the window, so it cannot show more than `V_batch − (R_batch − 1)·N_dense = N_dense + E`. Showing more next to short neighbours needs either a lookahead of `R_batch − 1` paragraph lengths (still bounded state, since the worker has every length) or a smaller guaranteed head than C0's `N_dense`. Task 1.1 reads C0's values and records which of the three holds: the cap is accepted (the notice and the fold markers explain it), the fold uses a lookahead (H1, H1i and H2 then range over the lookahead step), or C0 records a guaranteed head below `V_batch ÷ R_batch`. The Kani properties of D12 hold for every choice that passes `FoldThresholds::new`.

The plan does not depend on the wrap mode. Wrapping only adds rows to a paragraph, so the plan stays sound under both modes, and toggling word wrap never re-folds (D12, H1).

`ui/protected_fold.rs` is the executor, with no role. It wraps each install turn, and the turn inserts `head` with `insert` and `tail` with `insert_with_tags(fold_tag)`. Install turns come from:
- load `execution.rs`;
- `buffer_replacement/execution.rs` (draft recovery, local-history restore and Undo Restore; save-formatting mirror-back never runs on a protected buffer, see D9);
- eviction reload;
- monitor reload;
- reopen with encoding;
- local-history preview.

For a paragraph larger than the slice budget, the existing one-turn exception still applies (C3 is conditional). The head and the tagged tail are inserted in the same dispatch, so no turn boundary ever shows an unfolded paragraph.

*Alternatives:*
- Fold on idle after insertion. Rejected: a validation idle could lay out the unfolded paragraph (review item 8).
- Head-hiding windows. Rejected: AT-SPI offsets would misalign.
- A per-paragraph tag applied after a whole insert. Rejected: the same race.

The policy does integer arithmetic only. It carries `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]` and joins the whole-pixel table at ceiling 0. Following C1, its unit is characters, and `R_batch` carries the pixel relationship: C0 derives it from the 2000 px validation batch (A31) and the smallest row height.

The Kani harnesses for this policy are listed in D12. A proptest mirrors them at larger sizes and with the production `R_batch`.

### D2. Protected View state is a monotone per-editor session with one seam
`ProtectedViewTicket` is the editor's weak identity plus a protected-session generation. `ProtectedViewFacts` holds the reason, the fold ranges summary and the source state. Both are constructed once at entry (`seams.rs`), following the `*Ticket` + `*Facts` + `*_is_current` shape. Entry points:
- load and reload, from `DocumentScale.protected_view`;
- edit growth, from C1's hook calling `protected_view::enter_from_edit`.

The state is never cleared by edits, preference changes, tab switches, eviction reload, draft recovery or local-history restore. A reload of the file from disk (external-change reload or explicit reload) re-derives it from the new profile, which is the only way a smaller file becomes editable again.

Edit-growth entry applies the fold tag to the existing tail. Tag application does not change text (A27), and it runs in the same `insert-text` or `delete-range` after-handler emission (C1's watch handles both, because a deletion of a delimiter joins two paragraphs), before the validation idle. The cost is a re-layout of a paragraph that is at most `N_visible` long; a widget test proves the precondition.

### D3. Caret-free interaction: properties, an intercept, a capture gesture and key-binding pre-emption
`execution.rs` does the following on entry:
1. Sets `editable=false` and `cursor-visible=false`.
2. Connects a `move-cursor` handler that calls the pure `policy::viewport_step(step, count, extend)` and scrolls the vadjustment and hadjustment. It then stops emission, which covers `gtk-keynav-use-caret` (gtktextview.c:6336-6343).
3. Installs a `GtkGestureClick` and a `GtkGestureDrag` in `PropagationPhase::Capture` on the view, for all buttons. Every sequence is claimed. The pure `policy::press_disposition(button, n_press, hit)` decides:
   - fold-marker hit → inspector;
   - secondary press → Protected View popover menu;
   - anything else → claim only.
4. Disables the DnD drop target and drag source.
5. Pre-empts every default key binding that can move a mark or change the buffer, before its default handler runs. Per-instance `action_set_enabled(false)` is **not** used as the guard: GtkTextView recomputes those actions' enablement on every popup and selection change (gtktextview.c:9317-9348), and Ctrl+A reaches the `select-all` signal, not the action. Two mechanisms cover the bindings:
   - keybinding **signals** (GtkTextView `select-all`, `toggle-cursor-visible`, `cut-clipboard`, `copy-clipboard`, `paste-clipboard`, `backspace`, `delete-from-cursor`, `insert-at-cursor`, `toggle-overwrite`, `set-anchor`, `extend-selection`, `insert-emoji`; GtkSourceView `change-number`, `change-case`, `join-lines`, `move-to-matching-bracket`, `move-lines`, `move-words`, `show-completion`, `push-snippet`) get a handler that stops the emission. `change-number`, `change-case` and `join-lines` are the critical ones: their default handlers change the buffer without checking `editable` (gtksourceview.c:434-560);
   - binding **actions** (`menu.popup` on Shift+F10 and Menu, `text.undo`/`text.redo`, `source.change-case`) are claimed by a capture-phase key controller on the view. The pure `policy::action_disposition` maps each to pre-empt, route-to-Protected-View-menu or viewport-safe.

   A widget test walks the view's class shortcut controllers (`gtk_widget_observe_controllers`) on the pinned toolkit and asserts that every bound action and signal has a disposition, so a binding that a toolkit update adds fails the test rather than reaching the buffer. Each pre-empted user request publishes the notice through the rate-limited lane.

Nothing in LushText or on a toolkit input path moves the insert mark, because the `mark-set` handler on insert drives the IM spot and the a11y caret (gtktextview.c:9092-9117). Navigation uses a dedicated `protected-nav` mark with `scroll_to_mark`. The one path LushText cannot pre-empt is an assistive technology writing the caret or selection through `GtkAccessibleText` (gtktextview.c:11074-11119). P4 and the lane's AT case pin its cost, and `ProtectedViewEvidence` counts any insert-mark move it observes.

*Alternatives:*
- Make the view non-focusable. Rejected: keyboard scrolling and AT access would be lost. This stays the fallback if an axiom probe fails.
- Switch the IM module to simple. Rejected: it degrades CJK input everywhere.

### D4. One immutable text source per protected session, captured lazily
Find, inspector windows and whole-line copy all read `ProtectedTextSource`:
- an `Arc<str>` of the buffer text;
- a sparse paragraph index: paragraph start byte offsets, plus one char→byte checkpoint every `K` characters inside paragraphs longer than `N_visible`.

The source is captured on first need through the existing chunked snapshot (`buffer_snapshot`), so each turn stays within `B_settle`. The capture holds transient admission only while it is in flight. Once it is accepted it becomes long-lived editor state, charged to the editor's live-memory estimate and holding a guaranteed handoff to plain disposal, following the plain-data disposal ownership rule. It stays valid while the buffer is immutable. Every whole-content install through the fold executor (reload, draft recovery, local-history restore or Undo Restore) advances the protected-session generation and releases it. The source is released through plain disposal on close, eviction, reload or install.

*Alternatives:*
- Retain the decoded body from load. Rejected: it doubles memory for every protected tab even when no feature needs the copy.
- Read windows from the GtkTextBuffer. Rejected: `get_iter_at_line_offset` walks UTF-8 inside a single huge segment on GTK, and find would still need a copy.
- Re-read the file. Rejected: it is wrong for edit-growth entries and needs identity revalidation.

### D5. Worker find replaces GtkSourceSearchContext in Protected View
The in-tab search bar gains a protected mode, chosen at attach time:
- `ui/editor_page/search.rs` skips `search_bar.attach(&buffer, view)` when protected and calls `protected_view::find_execution`;
- the search entry, the counter and the next/previous buttons are reused;
- the replace row is hidden.

Matching reuses the option model of the search bar (case, regex and whole-word). It uses the regex crate and the `grep-*` primitives already in the workspace, line-oriented through C1's paragraph-boundary function so CR-only and U+2029 are correct.

Single-flight follows `services/single_flight.rs`: one active request, one latest pending request, and a cooperative cancellation token. Batches are bounded by `FIND_RESULTS_PER_TURN` and `FIND_BYTES_PER_TURN` under `FIND_RESULT_CAP` (policy constants, initial 256 / 64 KiB / 10,000, confirmed by C0).

The current match is highlighted with one `protected-find-current` tag only when the match lies entirely in visible text. Otherwise activation opens the inspector. The existing `editor-search` readiness blocker is held while a protected request, including first-use source capture, is active, so the `search-complete` predicate covers it.

Both prefill sites (`ui/editor_page/search.rs:82`, `ui/window/search.rs:593`) move to one helper that reads `visible=true` text under `MAX_SEARCH_SELECTION_PREFILL_CHARS`. In Protected View no selection exists, so the helper returns none.

### D6. Line Inspector is an AdwDialog called presentation surface
`inspector.rs` is a Blueprint template with:
- an `AdwDialog` containing a header with the line number and the column range;
- a read-only `GtkTextView` with wrap on, a visible caret and selection allowed (its buffer is at most `N_inspect` characters, which is at most C0's bound `min(N_visible, L_protect)`, so caret paths are bounded);
- Previous and Next buttons, a Go to Column `GtkSpinButton`, and Copy Window and Copy Whole Line buttons.

`inspector_execution.rs` reads windows on a worker from the source (D4) using the char checkpoints. Reads are generation-guarded, and only the latest window installs. Windows cut on character boundaries. The inspector shows the absolute column of its first character.

Copy Whole Line installs a `GdkContentProvider` subclass whose `write_mime_type_future` writes the paragraph slice of the `Arc<str>` to the output stream as a sequence of bounded asynchronous chunk writes (`FIND_BYTES_PER_TURN`-sized or a dedicated `COPY_CHUNK_BYTES` policy constant). `gio::OutputStream` is not `Send`, so the writes are issued from the GTK main context, but each dispatch copies at most one chunk and the line is never materialized whole on GTK. Window reads for the inspector run on a worker. Decision 5 of the proposal (whether to offer whole-line copy at all) is recorded under Open Questions. This design ships it because it costs no GTK time.

### D7. Navigation targets are classified by a pure function
`policy::navigation_target(paragraph, column, fold)` returns either `ScrollTo { paragraph }` or `Inspect { paragraph, window_start }`. All the navigation entry points call it:
- restore position;
- bookmark activation;
- local-history targets;
- workspace-search hit activation (`open_file_at_line`);
- protected find results.

### D8. Fold marker gutter
`fold_gutter.rs` is a `GtkSourceGutterRendererText` subclass that is enabled only in Protected View. It queries the fold ranges by line through a `BTreeMap` lookup, so the work is O(visible rows · log folds). It shows a symbolic marker with a tooltip and an accessible label giving the hidden count. Presses on it are recognized by the capture gesture (D3), which uses `policy::press_disposition`.

### D9. Save path and data integrity
`save/` gains a `ProtectedSaveMode` fact:
- **Save** is disabled while the document is unmodified.
- **Save As** sets `suppress_transformations` (EditorConfig formatting, line-ending normalization and mirror-back off) and writes the buffer's separators verbatim with the recorded encoding and BOM state through the existing durable writer.

C1's `DocumentProfile` does not report decode round-trip exactness, so C2 adds it to `LoadMetadata`: the load worker already refuses decodes with errors (`services/editor_io.rs:509-515`), and for an encoding whose error-free decode is not injective it re-encodes on the worker and compares with the source bytes. When the decode was not round-trip exact, Save As of an unmodified protected document is refused with a notice.

`ProtectedSaveMode` applies to **every** save of a protected document, including edit-growth documents: no EditorConfig formatting, no line-ending normalization, and no mirror-back. Mirror-back is a buffer replacement, which would contradict buffer immutability, and line-ending normalization is unavailable in Protected View, so the ordinary `Mixed` refusal (`EditorSaveError::MixedLineEndings`, `services/editor_io.rs:1416-1423`) would leave an edit-growth document unsaveable. The buffer is written verbatim in its recorded encoding and BOM state. Edit-growth documents otherwise keep the normal modified-document semantics: Save, close-with-changes, drafts.

A byte-identity integration test round-trips synthetic fixtures: UTF-8 with and without BOM, CRLF, CR-only, mixed endings, U+2029, and a 1.9M-character paragraph.

### D10. Surfaces gated by the Protected View fact
Each surface asks `ProtectedViewFacts`:
- **Invisibles** (`invisibles.rs`): forced off while any fold exists.
- **Print** (`window/print/policy.rs`): a new `PrintOutcome::RefusedProtectedView`.
- **Local-history preview** (`preview_execution.rs`): runs the install executor with the fold.
- **Markdown preview admission:** confirms its existing paragraph and slice budgets produce `Simplified` or limited, with a widget test.
- **Notes editors:** read-only plus notice when the body profile is protected.
- **File peek:** a test confirms the sample's longest visible line is at most `N_visible`; the sample is bounded already.
- **Bookmark toggle:** disabled.
- **Mixed-line-ending normalization:** disabled.
- **Automation:** projects from `evidence.rs`.

### D11. Axioms beyond C0
These are added to the ledger with `gtk-lush-axioms` probes and samples, numbered after C0's last entry (P1–P7 are provisional names that `sliced-protected-install` cites):
- P1: a capture-phase claim prevents insert-mark motion, IM reset, middle-click primary paste, the default context menu and drag start;
- P2: non-editable means no IM focus-in and no key-controller IM;
- P3: a `move-cursor` intercept holds under `gtk-keynav-use-caret` (extends A34);
- P4: AT-SPI text and offsets for an invisible tail, and the cost of an AT-initiated caret or selection write into a folded paragraph (extends A35). Like A35 it has no in-crate probe, because `GtkAccessibleText` is reachable only through an AT-SPI bus; the lane's AT case pins it and the ledger row says so;
- P5: DnD disablement;
- P6: per-instance action enablement is recomputed by GtkTextView (gtktextview.c:9317-9348) and is not a guard, while a stopped keybinding signal or a capture-phase key claim does keep `select-all`, `toggle-cursor-visible`, `menu.popup`, `change-number`, `change-case`, `join-lines` and `move-to-matching-bracket` from reaching their default handlers;
- P7: `insert_with_tags` of an invisible run never lays out the tail.

Run `make gtk-axioms` first. A falsified axiom blocks the dependent task (pre-existing-blockers rule).

### D12. Kani proofs of the Protected View preconditions
Every responsiveness claim in this change assumes the density bound, and every integrity claim assumes the admission table and monotone entry. They are therefore Kani-proved over the production pure cores (`formal-verification-kani`; `.agents/rules/build.md` "Formal Verification (Kani)"). Every harness has a `kani::cover!` on each interesting branch. Each restricted domain is stated in the harness and in the checked function's rustdoc. Both modules are GTK-free and I/O-free.

**Refactors so the cores are pure and checkable.** These are part of tasks 2 and 4, not later additions:
- **`model/protected_fold.rs`**
  - The thresholds are a `FoldThresholds { n_visible, n_dense, v_batch, r_batch }` parameter, not free constants. `FoldThresholds::new` returns `None` unless `n_dense ≥ 1`, `n_dense ≤ n_visible`, `r_batch ≥ 1` and `r_batch·n_dense ≤ v_batch` hold, with checked multiplication. `FoldThresholds::PRODUCTION` is built in a `const` that fails to compile if C0's values break them. The worker passes `PRODUCTION`. The harnesses pass symbolic tuples, so they check the same code.
  - `fold_step` is two named production stages: `length_cap` (the `N_visible` fold) and `density_cap` (the excess budget). The `should_panic` harness calls `length_cap` alone. No test-only switch exists.
  - `FoldState` is the excess ring plus `window_excess`, its running sum. `density_cap` reads the ring only through `window_excess` and the entry it evicts. That is what makes the step-induction argument independent of `R_batch`.
  - `row_span(visible_chars, WrapModel) -> u64` is the policy's row model: `WrapOff` gives 1, and `WrapOn { chars_per_row }` gives `max(1, ceil(visible / chars_per_row))`. It is production code: the PR-gated density widget test (task 3.4) uses it for its expected windows and checks it against GtkTextView's laid-out line y-ranges, so the model Kani reasons about is tied to the toolkit.
  - `FoldRange::locate(column) -> FoldLocation { Visible(col) | Hidden(tail_offset) | PastEnd }` and `FoldLocation::column_of` hold the column mapping. `fold_byte_offset(paragraph: &str, head_chars) -> usize` converts a head length to a byte offset. The fold executor and `navigation_target` call them.
- **`ui/editor_page/protected_view/policy.rs`**
  - `ProtectedAction` is a closed enum of every toolkit binding, signal, action and LushText entry point that the D3 enumeration test classifies. `toolkit_effect(ProtectedAction) -> ToolkitEffect { mutates_buffer, moves_mark }` records the GTK and GtkSourceView source facts in D3/Context. `action_disposition(ProtectedAction) -> Disposition { ViewportSafe | Preempt | RouteToMenu }` is the admission table. An action is *admitted* when its default handler is allowed to run, which only `ViewportSafe` does: `Preempt` stops the emission or claims the binding, and `RouteToMenu` opens the Protected View menu instead. There is no fourth disposition, so the D3 enumeration test's "viewport-safe or pre-empted" classification and the harness speak about the same table. The enumeration widget test fails on any binding that has no `ProtectedAction`.
  - `ProtectedSession { protected: bool, generation: u64 }` and `on_event(SessionEvent) -> ProtectedSession` are the monotone entry state. `execution.rs` stores the session value and only ever replaces it with the result of `on_event`. The events are edit, edit growth, preference change, tab switch, eviction reload, draft recovery, local-history restore, Undo Restore and `ReloadFromDisk { protected }`.
  - `inspector_window(len, index, n_inspect)`, `inspector_window_count` and `inspector_window_containing(len, column, n_inspect)` are loop-free arithmetic.
  - `admit_find_record(record) -> AdmittedRecord` truncates a record's context so it stays within `MAX_FIND_RECORD_BYTES`, where `MAX_FIND_RECORD_BYTES ≤ FIND_BYTES_PER_TURN` is a `const` assertion. `next_find_batch(&[AdmittedRecord], delivered, caps) -> usize` returns how many queued records the next GTK turn publishes. It takes `AdmittedRecord`s only, so an unadmitted record cannot reach it in production.

**Row-window assumption.** H1 models a 2000 px validation batch as `R_batch` consecutive rows, each at least the smallest row height. A batch that starts at an arbitrary pixel can cover one more, partial, row. Task 1.1 confirms that C0's recorded `R_batch` already counts that edge row (`ceil(2000 / h_min) + 1`). If it does not, `FoldThresholds::PRODUCTION` uses `R_batch + 1` as its window, and the rustdoc and the programme record say so. The row model is the stated domain of H1. The widget test of task 3.4 checks the model against GTK's laid-out rows, and C0's lane checks the wall-clock budget it serves.

**Harnesses in `crates/lushtext-core/src/model/protected_fold/kani_proofs.rs`** (shard `core-protected-fold`, gate `pull-request`):

| Id | Harness | Checks | Domain and why it is enough |
| --- | --- | --- | --- |
| H1 | `density_bound_holds_for_every_sequence_and_wrap_mode` | Folding by `fold_step`, then any window of `r_batch` consecutive rows under `row_span` from any start row, in either wrap mode, holds at most `v_batch` visible characters. Every paragraph shows at most `n_visible`. | Up to 6 paragraphs (`#[kani::unwind(7)]`), each any `u64` length. The harness computes row positions and window sums in `u128`, so no harness arithmetic can overflow, and it decides which paragraphs a window `[s, s + r_batch)` touches by interval arithmetic over the 6 row ranges with a symbolic `u128` start row `s`, never by iterating rows. Production code never sums rows. Symbolic thresholds that satisfy `FoldThresholds::new`, with `r_batch` from 1 to 3. Any `WrapModel` and any `chars_per_row ≥ 1`. Six paragraphs with `r_batch ≤ 3` reach warm-up, full-window and evicting states. `kani::cover!` on each. |
| H1i | `density_step_preserves_the_window_invariant` | From any `FoldState` with consistent ring, `window_excess ≤ E`, and any length, the emitted visible count keeps the window within `v_batch` and the next state is consistent with `window_excess ≤ E`. | Ring of up to 3 symbolic entries (`#[kani::unwind(4)]` for the harness's consistency sum) whose sum is assumed not to overflow and to equal `window_excess`, any `u64` length. This is the compositional step: `density_cap` reads the ring only through `window_excess` and the evicted entry, so the invariant holds for any `r_batch`, the production value included, and by induction for sequences of any length. The proof note goes in the harness doc and in `fold_step`'s rustdoc. |
| H1c | `production_thresholds_satisfy_the_preconditions` | `FoldThresholds::PRODUCTION` passes `new`, and `n_inspect ≤ min(n_visible, l_protect)`. | Concrete. It pins the link between the generic proofs and the shipped constants. |
| H1x | `density_bound_fails_without_the_density_fold` (`should_panic`) | `length_cap` alone, the same window assertion as H1. | The H1 domain. It pins the density stage as the guard the bound needs. Because H1 proves the same stages and the same window arithmetic free of overflow and panics, the only check H1x can fail is the window assertion, so its `should_panic` verdict cannot come from an unrelated panic. |
| H2 | `folds_hide_only_tails` | `head + tail = len`; a non-empty paragraph keeps `head ≥ min(len, n_dense) ≥ 1`; `len ≤ n_dense` is never folded; the range starts at the head and ends before the delimiter (paragraph lengths exclude delimiters, C1); the ranges of a sequence are ordered and disjoint. | Any valid state and any `u64` length for the single step. The H1 sequences (`#[kani::unwind(7)]`) for ordering. |
| H2b | `fold_byte_offset_never_splits_a_scalar` | `is_char_boundary(offset)` and `prefix.chars().count() == head_chars`. | Every valid UTF-8 string of a symbolic length up to 8 bytes (`#[kani::unwind(9)]`; the harness assumes `from_utf8` succeeds) with no delimiter, and any `head_chars` up to its character count. The function walks scalars and branches only on the lead-byte class. Eight bytes cover every pair of lead classes, the 4-byte one included, and a scalar boundary on either side. |
| H2c | `fold_location_is_total_and_round_trips` | `locate` never panics for any `u64` column. Visible iff column < head (≤ head when unfolded). Hidden offset + head = column. `column_of(locate(c)) = c` for `c ≤ len`. `PastEnd` iff `c > len`. | Any `u64` length, any valid state, any `u64` column. The function is loop-free, so the domain is the whole type. |
| H2s | `fold_plan_is_independent_of_install_slicing` | One run equals two runs split at any index with the state carried across. | Up to 6 paragraphs (`#[kani::unwind(7)]`), any split index, the H1 thresholds. The executor carries the state only through `FoldState`, which is the object compared. |

**Harnesses in `crates/lushtext-core/src/ui/editor_page/protected_view/policy/kani_proofs.rs`** (shard `core-protected-view`, gate `scheduled`):

| Id | Harness | Checks | Domain and why it is enough |
| --- | --- | --- | --- |
| H3 | `no_mutating_action_is_admitted` | For every `ProtectedAction`, if `toolkit_effect` mutates the buffer or moves a mark, the disposition is not `ViewportSafe` (the only admitting disposition). Also, for an independently written list in the harness (insert-at-cursor, delete-from-cursor, backspace, cut, paste, middle-click paste, move-lines, move-words, join-lines, change-case, change-number, insert-emoji, push-snippet, undo, redo, DnD drop, the search bar's replace and replace-all, line-ending normalization), `toolkit_effect` reports mutation and the disposition is not `ViewportSafe`. That way a weakened effect row cannot hide an admission. `win.begin-replace` is deliberately not in the list: it does not mutate, and in Protected View it opens find only (search-replace-safety). | The whole closed enum, through `kani::any` and a `cfg(kani)` `Arbitrary` derive. Loop-free. `kani::cover!` on each disposition. Completeness against the toolkit is the widget enumeration test's job (D3), not Kani's. |
| H3m | `protected_view_entry_is_monotone` | Through `on_event`, a protected session stays protected for every event except `ReloadFromDisk`, and that event returns editable only when its `protected` is false. The generation never decreases and advances on every whole-content install. | Any start state whose generation is at most `u64::MAX − 6`, and any sequence of up to 6 events (`#[kani::unwind(7)]`). `on_event` advances the generation with `checked_add` and its rustdoc states the same bound: a session cannot install `2^64` contents, so the bound is an envelope, not a gap. `on_event` has no history but the session value, so 6 steps cover every pair of consecutive events and every event from both states. |
| H3x | `entry_is_not_monotone_if_eviction_reload_re_derives` (`should_panic`) | The H3m assertion, with eviction reload mapped to the production `ReloadFromDisk` re-derivation. | The H3m domain. It pins the rule that only a reload from disk re-derives the decision. |
| H4 | `inspector_windows_are_bounded_and_tile` | `1 ≤ len_i ≤ n_inspect`, `start_0 = 0`, `start_i = end_{i−1}`, `end_last = len`, and `inspector_window_containing(c)` contains `c` and equals the window at its index. | Any `u64` length of at least 1 (the inspector only opens on a folded paragraph, which is longer than `N_visible ≥ 1`; `inspector_window_count(0) = 0` is asserted separately), any `n_inspect` from 1 up to the bound, any index below the count, any column below `len`. The functions are loop-free, so tiling of unbounded paragraphs follows from adjacency of any two consecutive windows plus the two ends. |
| H4n | `navigation_target_reaches_every_column` | `navigation_target` returns `ScrollTo` exactly for visible columns, and `Inspect` with a window that contains the column otherwise. | The H2c and H4 domains. |
| H4f | `find_batches_are_bounded_and_partition_results` | Repeated `next_find_batch` calls give batches of 1 to `results_per_turn` records within `bytes_per_turn`, the concatenation is the first `min(n, result_cap)` admitted records in order, and the planner finishes in at most `n` turns. | Up to 6 records with symbolic sizes up to `MAX_FIND_RECORD_BYTES`, at most 6 turns of a prefix scan of at most 3 records (`#[kani::unwind(7)]`), and symbolic caps that satisfy the production `const` relations (results per turn 1 to 3, result cap 1 to 6). The planner is a prefix scan over `(count, bytes)`, so 6 records with caps of 3 cover a cap hit, a byte hit and the result-cap stop. `kani::cover!` on each. |
| H4x | `find_batch_exceeds_the_byte_bound_without_record_admission` (`should_panic`) | The H4f byte assertion over a record built without `admit_find_record`. | One record larger than `bytes_per_turn` (`#[kani::unwind(2)]`). The harness builds the `AdmittedRecord` directly, which the child `kani_proofs` module can do and production code cannot. It pins record admission as the guard that lets a byte-bounded planner still make progress: the planner always delivers at least one record, so an unadmitted oversized record breaks the byte bound rather than stalling. |

**Unwind bounds.** Each harness declares the `#[kani::unwind]` stated in its row; a harness with no loop (H1c, H2c, H3, H4, H4n) declares none. The production functions under proof are loop-free except `fold_byte_offset`, whose scalar walk is bounded by the string length, and `next_find_batch`, whose prefix scan is bounded by the per-turn count cap. `density_cap` touches the ring in O(1) (one read of the evicted entry, one write), so no harness unwinds a loop of length `R_batch`.

**Shards and budgets.** `core-protected-fold` is gated `pull-request`, because H1 is the precondition every responsiveness requirement here rests on, as `widgets-geometry` is for the sidebar. It qualifies only if its runner-measured wall time is at most 15 minutes. If it measures over that, the shard is split, H1, H1i, H1c and H1x stay PR-gated, and the rest move to a `scheduled` shard. Bounds are cut only last, under the fit order in `.agents/rules/build.md`. `core-protected-view` is gated `scheduled`. Both get `ci_minutes`, `ci_peak_gib` and `measured_in` from dispatched runs, the larger of two, within 25 min and 12 GiB.

**Oracles.** `ORACLES` gains `crates/lushtext-core/src/model/protected_fold.rs` and `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs`, each with its harnesses ordered cheapest first and with measured local seconds and a `ci` or `local` tier. `make proof-strength PROOF_STRENGTH_MODULE=<each>` runs once, and its survivors are triaged in `docs/next/formal-verification.md`.

**Alternative considered.** Proving the bound only at the production `R_batch`, with sequences longer than it. Rejected: a ring of about 140 symbolic `u64` sums is far outside the budget, and the step induction (H1i) proves the same property for every `R_batch` at a small cost.

## Risks / Trade-offs

- **[Minified files become view-only]** → `L_protect` comes from C0's sweep, the notice explains the reason, and the inspector, find and copy keep content usable. `L_protect` stays a fixed constant (maintainer decision, 2026-10-11).
- **[An unprobed toolkit path still reaches the whole paragraph]** (IM, AT-SPI, SpaceDrawer) → axioms P1–P7, the C0 AT-bridge and IBus lane runs, and the fallback of a non-focusable view plus an inspector-only reading path.
- **[Source capture of very large documents costs memory]** → lazy capture, charged to live memory and transient admission, released on eviction. An unmodified protected tab stays evictable.
- **[Edit-growth entry with a modified buffer]** → saving and drafts keep working because only further edits are blocked. `data-safety` runs explicitly on this change.
- **[Single-paragraph install exceeds `B_settle`]** → C3 is conditional on C0's lane measuring this change's protected one-turn install and clear above `B_settle` (C3's adoption gate). Until then the one-turn exception is a declared envelope.
- **[Fold-marker gutter adds per-frame work]** → it is bounded by visible rows and enabled only in Protected View.
- **[Clipboard providers outlive the editor]** → the provider holds its own `Arc<str>` clone until the clipboard owner changes. That retention is long-lived, so it is charged as accepted long-lived ownership (the plain-data disposal rule), not to the shared 256 MiB transient admission, which an indefinitely held clipboard would otherwise starve. It is released through plain disposal when the provider is dropped.
- **[Touch panning]** → claiming every sequence on the view in the capture phase also denies the ancestor `GtkScrolledWindow`'s touch pan gesture for that sequence. `press_disposition` leaves touchscreen drag sequences unclaimed unless they start on a fold marker; a widget test checks that a touch drag scrolls and leaves the marks unmoved, and P1's probe covers it.
- **[Assistive-technology caret writes]** → an AT can set the caret or selection through `GtkAccessibleText` (gtktextview.c:11074-11119), which LushText cannot pre-empt. P4 and the lane's AT case measure its cost on a folded paragraph; if it exceeds `B_interact`, the fallback is the non-focusable view with the inspector as the reading path.

## Migration Plan

- There is no persisted format change and no GSettings migration.
- The feature lands behind the `DocumentScale.protected_view` decision from C1, so documents below the thresholds are unaffected.
- **Rollback:** revert the change. C1's degraded mode, with the reduced-features notice, remains as the floor.

## Open Questions

1. **Copy Whole Line** (proposal decision 5): ship the lazy provider (this design), or offer bounded windows only?
2. **`L_protect` as a GSettings key** — resolved 2026-10-11: a fixed policy constant, not a GSettings key. A setting that lets the app block would contradict the always-responsive requirement; revisit only together with `explore-editing-long-paragraphs`.
3. **Bookmark-by-line-number in Protected View:** should a line chooser let users add bookmarks? Bookmarks are sidecars, so this is integrity-safe, but it is out of scope here.
4. **End-of-head fold label:** besides the gutter marker, should the view draw a `⋯ +N` label after each folded head in a `snapshot_layer`? Its cost is bounded by visible rows.
5. **A dedicated readiness blocker** for fold settle, only if C0 shows the `file-load` blocker does not cover it. The inspector's window reads need a blocker in any case; reuse one or add `line-inspector`.
6. **Grapheme-boundary folds:** whether C1's bounded scan supplies grapheme boundaries near `N_visible`, or a character boundary is accepted with a documented split-cluster artifact.
7. **Save-time transformations on edit-growth documents:** this design suppresses EditorConfig formatting and line-ending normalization for every save of a protected document, including one the user edited before it became protected, because both would change the buffer or are unavailable in Protected View. The alternative, applying them to the written bytes only without mirror-back, would make the file differ from the buffer the user sees.
