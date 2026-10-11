# Long-line responsiveness programme (C0–C5): origin proposal

> Planned-work record. This is the revised, adversarially reviewed explore-mode proposal (2026-10-11) from which the changes `probe-long-line-toolkit-costs` (C0), `document-scale-policy` (C1), `long-line-protected-view` (C2), `sliced-protected-install` (C3), `bound-window-construction-dispatch` (C4) and `explore-editing-long-paragraphs` (C5) were created. C0 owns keeping this record current. Measurements cited here were taken during exploration against a GTK 4.22.5 source checkout and a private headless session; the reproducer file itself is not in the repository.

> **Current state lives in [`long-line-documents.md`](long-line-documents.md)**, the programme record C0 created: the measured thresholds and budget verdict, the axiom ids (A23–A36) each change cites, two probe corrections to claims below (A29's cut-off comes at tens of thousands of characters, not near two million; GtkSourceView's search skips invisible text by default, A33), and the blockers each later change owns. This file stays as the origin proposal.

> This proposal came out of explore mode. No repository file was edited. Claims marked **(verified)** were checked again in this revision against the repository or the GTK 4.22.5 sources under `target/huge-file-explore/src/`. Claims marked **(reviewer-cited)** come from an adversarial review with a citation that I did not re-open. Claims marked **(assumption)** are unverified and need an axiom probe. Scratch evidence is under `target/huge-file-explore/` (`load/`, `scroll/`, `probe/`, `fz/`, `review/`).

## Summary of the revision

The reviews disprove a central claim of the draft. The draft said an editable fold built from `invisible` tags would bound "every interaction". It does not. GtkTextBuffer works out cursor positions, word boundaries and Backspace from `compute_log_attrs`. That function slices the **whole logical paragraph, hidden text included**, and runs `pango_get_log_attrs` over it (gtktextbuffer.c:4469-4503 **(verified)**). Its cache is cleared whenever the buffer's `chars_changed_stamp` moves (gtktextbuffer.c:4532-4537 **(verified)**). On gtrib.html's 1,894,586-character line, two reviewers each measured that one call at 92–99 ms. So the fold cannot make editing *inside* a multi-megabyte paragraph instant in stock GtkTextView. IM surrounding-text, accessibility offsets and visible-cursor iteration add further whole-paragraph costs.

The revised direction has three parts:

1. **Profile and gate.** Measure each document's paragraph lengths on the worker. Turn off syntax, the minimap and the other features that cost time per paragraph before they ever see a giant paragraph. This removes the 2 s stall and both CRITICALs.
2. **Protected View.** Documents whose profile is over the protected thresholds open in a **non-editable view**:
   - the buffer is never changed;
   - giant paragraphs are folded for display only;
   - `cursor-visible=false`, so the arrow keys scroll the viewport and never move a caret through log attrs;
   - LushText takes over every path that would place a caret, select text or run a search inside a giant paragraph;
   - the content of a giant line is reached through a bounded **Line Inspector**, a secondary read-only view over a copied window of the line.
3. **Editing giant paragraphs needs segmentation or a custom widget.** That is a later, separate decision. Until then, editing in these documents is *unavailable*, and saying so is instant. It is never *slow*.

## Proposed split, in order

| # | Change | Delivers | Gate before the next change |
|---|---|---|---|
| C0 | `probe-long-line-toolkit-costs` | The new GTK axioms with `gtk-lush-axioms` probes. A synthetic fixture generator. An in-process responsiveness watchdog. A `make responsiveness-smoke` lane (nightly or manual). PR-gated precondition tests are not part of C0; each later change adds its own. | The probe results decide the threshold values and whether each part of C2 is viable. |
| C1 | `document-scale-policy` | The worker profile. One paragraph-boundary function that matches GTK, which fixes a pre-existing slicer blocker. `DocumentScale` replaces `FileSizeCheck` for all its consumers. Syntax, minimap, invisibles and search highlighting are gated, at load **and on edits**. The minimap-detach CRITICAL is fixed. A user notice. | The 2 s stall and the CRITICALs are gone. Pango stalls on giant paragraphs remain (declared degraded mode). |
| C2 | `long-line-protected-view` | Protected View, the display-only fold, the Line Inspector, find on a worker, and the clipboard, print, navigation and accessibility contracts. | This change satisfies "instantly responsive" for every interaction offered on these documents, within the C0 install bound. |
| C3 | `sliced-protected-install` (conditional) | Inserting a single giant paragraph in slices, with the hidden part inserted already tagged. Adopted only if C0 measures per-turn insert cost above budget at the admitted sizes (100/250/500 MB), or above budget with the AT bridge on. | |
| C4 | `bound-window-construction-dispatch` | Removes the 188–380 ms window-construction stall. It is its own change in the same work stream, as the pre-existing-blockers rule requires. | |
| C5 | (optional, later, explore first) `edit-long-paragraphs` | Segmentation with an offset map, or a custom widget. Only if the maintainer wants giant paragraphs to be editable. | |

The rest of this document is the proposal for the whole stream, with C1 and C2 described in full. Each change gets its own `/openspec-propose` run, in order. C0 goes first.

## Reconciling the investigations (revised)

```
gtrib.html: 5.1 MB, 446 lines, 8 paragraphs > 10k chars, longest 1,894,586
        │
  ┌─────┴───────────────┬─────────────────────────────┬──────────────────────────────────┐
  ▼                     ▼                             ▼                                  ▼
GtkSourceView context   Pango layout of one           whole-paragraph BUFFER costs       per-dispatch batch
engine: atomic line,    paragraph (visible text       that ignore invisibility:          costs: 2000 px of
2000 ms cap, reads      only) at validate and         log attrs (cursor/word/Backspace), validation per idle,
hidden text too         draw time                     IM retrieve-surrounding, AT-SPI    wrap off ≈ 100 rows
                                                      TextChanged / offsets, search
  │ C1 gate             │ C2 fold (display)           │ C2 protected view: no caret,     │ C1/C2 density bound
                                                      │ no edits, no search context      │ in the policy
```

1. **The root cause is paragraph length, not file size.** All three investigations measured this (draft items unchanged).
2. **The fold bounds layout only.** It bounds layout cost at open and while scrolling (5–8 ms measured with synthetic text). Every buffer-level path still pays for the whole paragraph:
   - **log attrs** (verified above);
   - **visible-cursor iteration across hidden runs**, which goes one position at a time (gtktextiter.c:3210-3236, **reviewer-cited**);
   - **IM retrieve-surrounding**, which slices the paragraph (gtktextview.c:8996-9059);
   - **AT-SPI text contents and offsets** (gtktextview.c:10703-10750, **reviewer-cited**);
   - **the context engine**, which slices with hidden text included (gtksourcecontextengine.c:4536, **reviewer-cited**).

   This is why C2 makes the view non-editable and caret-free over these documents instead of relying on the fold alone.
3. **Turning off the caret removes the caret paths at the source.** **(verified)**
   - `gtk_text_view_move_cursor` (gtktextview.c:6602) checks `cursor_visible()` first. When the cursor is not visible it maps every movement step to `gtk_text_view_move_viewport` and never touches buffer iterators (the early branch, about :6616-6657). The caret path ends by setting `need_im_reset = TRUE` and resetting the IM (:6856-6857), and that path is skipped too.
   - Caveat: `cursor_visible()` returns TRUE when `gtk-keynav-use-caret` (caret browsing) is set (gtktextview.c:6336-6343). So LushText must also take over the `move-cursor` signal in Protected View, not only set the property.
   - Clicks still arm an IM reset (:5853) and place the insert mark. The insert-mark handler updates the IM spot and the accessibility caret, then resets the IM (gtktextview.c:9092-9117). So C2 also claims primary-click and drag gestures over protected paragraphs.
4. **The paragraph-delimiter mismatch is a pre-existing blocker.** **(verified)** `next_replacement_boundary` splits only on `'\n'` (`crates/lushtext-core/src/model/buffer_replacement.rs:71-91`). GTK splits paragraphs with `pango_find_paragraph_boundary` (`\n`, `\r`, `\r\n`, U+2029; gtktextbtree.c:1146, **reviewer-cited**). A CR-only or U+2029-separated document of any admitted size therefore installs in one turn. C1 fixes this.
5. **The wrap-off density contradiction (draft item 5) is still unresolved and becomes a policy input.** A per-paragraph limit does not bound one validation dispatch: validation covers 2000 px per idle (gtktextview.c:5094), which is about 100 rows with wrap off. The bound has to be on **visible characters per validated height**. C0's fixture "many lines just under the per-paragraph limit, wrap off" decides the dense-fold rule. It is a gate, not an optional sweep.
6. **Syntax also needs a total-size gate.** Background highlighting of 5 MB of short-line HTML still measured 50–112 ms gaps (prior-art lens). That is unchanged from the draft.

## Why

LushText must stay instantly responsive (open, scroll, every interaction), however large the file. Today gtrib.html freezes the app for seconds at open and makes scrolling unusable. The causes, ranked by evidence:

1. **Pango lays out one PangoLayout per paragraph.** It cost 285–1,256 ms per giant paragraph per layout, at validate and at draw time (gtktextlayout.c:2493; gtktextview.c:5094). The 20 s cache blow at gtktextlinedisplaycache.c:29 brings the cost back.
   - Correction from review: size-only displays *are* cached and reused for size-only requests (gtktextlinedisplaycache.c:330-352, **reviewer-cited**). The "laid out at least twice" claim holds only for the draw path.
   - Even with every optional feature off, open stalls 2.85 s in total.
2. **The GtkSourceView context engine analyses a line atomically.** It stops at a 2000 ms cut-off, emits `g_critical`, and turns highlighting off for the whole buffer (gtksourcecontextengine.c:72, 4459-4465). LushText enables syntax because the file is under 10 MB (`crates/lushtext-core/src/services/file_limits.rs:15-17`; applied in `ui/editor_page/load/execution.rs:575-577`). The engine reads hidden text too, so folding gives it no relief.
3. **Whole-paragraph buffer costs that folding does not remove:**
   - log attrs, about 95 ms per call on the 1.9M-character line, invalidated by any edit **(verified source; reviewer-measured)**;
   - visible-cursor iteration that is linear in the hidden length;
   - IM retrieve-surrounding, armed by every caret move, key and click (gtktextview.c:5785, 5853, 6856 **(verified)**);
   - AT-SPI TextChanged payloads on insert, and offsets that disagree with the folded layout (**reviewer-cited**).

   Measured unfolded: the IM spot is updated on every scroll (42–47 ms frames); quadratic `pango_layout_line_get_cursors` hung for more than 250 s on one Right arrow; typing costs 300–440 ms per character.
4. **Pango's int32 width overflows** above about 238k monospace characters (pango glyphstring.c:303-311). Wrap is then silently wrong.
5. **In-tab find blocks the main loop for 25–28 s.** It runs over a GtkSourceSearchContext (`ui/search_bar/execution.rs:41-42`). The non-regex path searches hidden text, while the regex path uses visible text only (gtksourcesearchcontext.c:559-567, 594, 733, **reviewer-cited**). The two in-tab prefill sites disagree: `ui/editor_page/search.rs:82` reads `text(…, true)` and `ui/window/search.rs:593` reads `text(…, false)` **(verified)**.
6. **LushText policy gaps:**
   - Degradation depends only on byte size (`services/file_limits.rs`). `FileSizeCheck` is used in 17 files, including `editor_io`, `draft_service`, `local_history_service`, `file_peek`, `bookmark_excerpt`, `plain_disposal`, `editor_page/{accessibility,imp,document_identity,load/execution,save/execution}`, `window/local_history/journal.rs`, benches and tests **(verified)**.
   - `word-wrap` defaults to true.
   - The paragraph-boundary slicer is `\n`-only (item 4 above).
   - The load contract deliberately installs any paragraph larger than the slice budget in one turn (`model/buffer_replacement.rs:65-70`; spec `main-thread-responsiveness` `spec.md:373`).
   - No feature is re-evaluated when an edit makes a paragraph giant, for example a paste.
7. **Minimap:**
   - It re-lays out every giant paragraph (8.1 s of stalls measured).
   - Its availability is decided only after attach (`minimap/admission.rs:140-160`).
   - `gtk_source_map_set_view(NULL)` leaves `update_id` pending, which produces `Gtk-CRITICAL` and `GLib-GObject-CRITICAL` (gtksourcemap.c:526-539, 564-576). These are reproducible warnings, so they are **pre-existing blockers**.
8. **Window construction stalls 188–380 ms** for every file. This is unrelated to long lines, but it breaks "instant" (C4).
9. **Unmeasured** (C0 measures them):
   - cost with the AT bridge on;
   - cost with a real text-input-v3 IM (IBus);
   - GtkSourceSpaceDrawer over a folded line (gtksourcespacedrawer.c:1288-1345, **assumption**);
   - tag-removal re-layout;
   - the render node on the GL renderer.

## What Changes

### The invariant, made explicit and enforceable

> **Budgets (values are maintainer decisions).**
> - **Interaction dispatches:** scroll, key, click, tab switch, toggles, find navigation. Each dispatch ≤ **B_interact = 16 ms**. Input-to-presented-frame ≤ **2 frames**.
> - **Settle dispatches:** open, install, restore, eviction reload. Each dispatch ≤ **B_settle = 50 ms**.
> - **Scroll:** p95 frame gap ≤ 16.7 ms on the reference lane.
>
> These apply to every admitted document (≤ `REFUSE_TO_OPEN`).
>
> **Where it is measured.** The reference lane: private `mutter --headless`, `GSK_RENDERER=cairo`, ≤16 cores. Two in-process measures replace the draft's D-Bus heartbeat, which measured queueing rather than dispatch length:
> - a `G_PRIORITY_HIGH` lateness watchdog that ticks every 2 ms on the main context; the largest lateness is the largest dispatch;
> - frame-clock `after-paint` timestamps for frame gaps and input-to-frame latency.
>
> The wall-clock lane runs nightly or on demand. Flake handling is documented, and it records `btrfs-stall-report` evidence for the run window.
>
> **Deterministic gates on PRs.** Widget tests assert *preconditions* that make the expensive toolkit paths unreachable. GTK's internal spans cannot be counted directly, so the tests check the preconditions:
> 1. No paragraph shown in any LushText `GtkTextView` has more than **N_visible** visible characters.
> 2. No validated window of 2000 px holds more than **V_batch** visible characters, by the policy's density model.
> 3. In Protected View the main view has `editable=false` and `cursor-visible=false`, and `move-cursor` is intercepted.
> 4. In Protected View no `GtkSourceSearchContext` is attached, the language is none, no minimap is attached, and invisibles are off.
> 5. LushText-owned bounded work stays within its declared chunk sizes: find batches, Line Inspector windows, notice text.

### C1: document scale profile and policy

- **Profile on the worker, with no extra pass.**
  - Extend `analyze_decoded_content_cancellable` (`services/editor_io.rs:1287`) and `LineEndingTally::scan_slice` (`:1194`).
  - It records:
    - the longest paragraph, in characters;
    - the count of paragraphs above each threshold;
    - a bounded list of their offsets;
    - a density summary, the worst visible-characters-per-row window under wrap-off.
  - The profile is carried in `LoadMetadata`.
- **One GTK-matching paragraph-boundary function in `model/`** (`\n`, `\r`, `\r\n`, U+2029).
  - A property test and an axiom probe check it against `pango_find_paragraph_boundary`.
  - The profile, `next_replacement_boundary` and `next_install_boundary` (`model/file_load.rs:287-289`) all use it.
  - CR-only and U+2029 fixtures are added. This fixes the pre-existing one-turn install.
- **`DocumentScale` in `model/`, GTK-free, replacing `FileSizeCheck`.**
  - The input is a pair: (longest paragraph, total bytes, density).
  - The outputs are syntax and bracket matching, minimap availability before attach, invisibles, automatic search highlighting, local-history availability, undo, `estimated_buffer_multiplier`, peek admission, and `protected_view`.
  - Kani proves monotonicity over the partial order: a larger profile never turns a feature back on.
  - Unit tests cover each threshold edge.
  - A property test checks that the profile does not depend on how the text is sliced.
  - Every one of the 17 consumer files is migrated in the same change. `live-editor-memory-budget` *is* affected, through `estimated_buffer_multiplier`.
- **Re-evaluate on edits, synchronously.**
  - A `connect_after("insert-text")` handler on the editor buffer updates the profile incrementally for the touched paragraph. The work is bounded: the inserted run plus its paragraph's cached length.
  - When a paragraph crosses L_syntax, the handler sets the language to none in the same emission. That is before any highlight idle can run (the GTK validation and highlight idles run later; gtktextview.c:5123-5131, **reviewer-cited**).
  - Downgrades are monotone for the session: features are never turned back on.
  - A paste, undo, move-lines or join-lines that creates a paragraph over the protected threshold makes the view switch to Protected View (C2). Until C2 lands, it shows the notice.
- **Minimap.**
  - Availability is decided from the profile before any GtkSourceMap is attached.
  - A map is never detached while its update tick may be pending: hide it, then detach after the next frame.
  - A failing-first regression test catches the CRITICAL.
  - File the upstream GtkSourceView report.
- **Notice.** One inline alert per document says which features are reduced and why. Its accessible text is bounded (`editor-inline-alerts`).
- **Result.** The 2 s stall and the CRITICALs are gone. Whole-paragraph Pango stalls remain until C2, as a declared degraded mode.

### C2: Protected View

Protected View applies when `DocumentScale.protected_view` holds: longest paragraph > L_protect, or density > V_batch. It does the following.

- **Buffer identity.** The buffer is the decoded file, byte for byte, and is never modified. `modified` stays false. Drafts, autosave and local-history capture are not triggered. Save As writes `text(…, true)` (`ui/buffer_snapshot.rs:630`, `:975`). A byte-identity round-trip test covers it.
- **Display-only fold.**
  - Each paragraph over N_visible shows its first N_visible characters. The tail carries `invisible=true, editable=false`.
  - **Only tails are hidden.** Head-hiding is rejected: it misaligns AT-SPI offsets with the layout (**reviewer-cited**).
  - The fold is applied **at insertion**: tails are inserted with the tag (`insert_with_tags`), so an unfolded giant paragraph never exists in the buffer. That makes "atomic" true by construction on every install path (load, eviction reload, file-monitor reload).
  - One cross-cutting GTK executor in `ui/` applies it. The pure placement policy lives in `model/`.
  - Density folding also hides the tails of lines over N_dense when the density bound requires it.
- **No caret, no edits, no IM work.**
  - `editable=false` and `cursor-visible=false`. The view intercepts `move-cursor`, so caret browsing is covered too: arrows, Page, Home/End and Ctrl+Home/End become viewport moves.
  - A capture-phase gesture claims primary press and drag over the view, so no caret is placed inside a protected paragraph. Double- and triple-click are covered too.
  - DnD source and target are disabled.
  - GtkSourceView's logical-line actions (move-lines, join-lines, change-case) and the `select-all`, `cut`, `paste` and `delete` actions are disabled, each with a notice.
  - Context-menu entries match.
  - An axiom probe confirms that IM focus and retrieve-surrounding do not fire on a non-editable view with no caret and no clicks **(assumption)**.
- **Line Inspector.**
  - "Inspect line…" opens a secondary read-only view over a **copied** window of one paragraph, `N_inspect` characters at a time, with wrap on.
  - Next and Previous move through the paragraph. Go to column moves the window.
  - Copying from the inspector is bounded to its window. "Copy whole line" builds the text off-GTK and offers it through `GdkContentProvider` (maintainer decision).
  - The inspector is the accessible path to the content. The main view's accessible text is documented as the folded view.
- **Navigation.**
  - Restore position, go-to-line, bookmarks, local-history targets and workspace-search hits scroll to the paragraph.
  - A target column inside a hidden tail opens the Line Inspector there.
- **Find.**
  - No `GtkSourceSearchContext` is ever attached to a protected buffer.
  - In-tab find runs on a worker over the decoded bytes under single-flight freshness, in bounded batches. Results are line+column records.
  - Activating a result scrolls to it, or opens the inspector when the hit is in a hidden tail.
  - Replace is unavailable.
  - Both prefill sites are unified: `visible=true` and bounded.
- **Invisibles** are off, as a hard rule while any fold exists.
- **Print** is refused with a notice in Protected View (fold-aware pagination is out of scope).
- **Secondary views** follow the same policy: local-history preview (protected, folded), Markdown preview (a long paragraph becomes the existing `Simplified`/limited terminal), notes editors, and file peek (check that the sample is folded).
- **Session.** C0 fixtures cover:
  - restoring three or more protected documents plus tab switching;
  - eviction then reselection;
  - an external reload through the file monitor.
- **Automation snapshot.** New fields: `protected_view`, the folded paragraph count, the hidden-character count. Never content. A readiness predicate for "fold settled" if C0 shows one is needed.

### C3: sliced protected install (conditional)

- **Applies when** C0 measures `insert_with_tags` of one giant paragraph above B_settle at the admitted sizes, or with the AT bridge on.
- **The exception.** A slice may end mid-paragraph, because the rest is inserted already hidden.
- **The new cost.** The btree merges adjacent character segments by copying both strings (`_gtk_char_segment_new_from_two_strings`, gtktextsegment.c:327-354, **reviewer-cited**). That is O(M) per slice.
- **Before adopting the exception:**
  - measure per-slice cost at 100, 250 and 500 MB;
  - if needed, keep a mark or toggle boundary between slices to prevent the merge (check against `toggle_segment_cleanup_func`).
- **Spec change.** `main-thread-responsiveness` "Large file installation yields in bounded GTK slices" (`spec.md:373`) is revised only in this change.

### C4: window construction stall

Bring the first window-construction dispatch under B_settle by splitting or deferring `geometry::execution::migrate_split_view_set` and any other construction work that the C0 lane flags.

## Capabilities

**New**
- `long-line-document-presentation` (C1 + C2): the profile; protected-view entry and exit (monotone); fold semantics (display-only, tail-only, applied at insertion); caret-free interaction; Line Inspector; worker find; clipboard and print contracts; the notice; snapshot projection.
- `responsiveness-measurement` (C0), or fold this into `performance-regression-coverage` (maintainer decision): the watchdog, frame timing, fixture generator, lane, and precondition gates.

**Modified**
- `main-thread-responsiveness`:
  - add B_interact and B_settle;
  - extend "Expensive preview and analysis workflows degrade before blocking input" (`spec.md:187`) to syntax, find, minimap, and edit-time growth;
  - in C3 only, revise `spec.md:373`.
- `editor-minimap`:
  - "Minimap availability adapts…" (`spec.md:128`): profile-based, decided before attach, no detach while a tick is pending;
  - reconcile "Wrapped-layout admission…" (`:424`).
- `performance-regression-coverage` (`spec.md:70`): the lane, the fixtures, and the precondition gates.
- `document-save-safety`: Protected View never dirties the buffer. Save As byte identity with a fold active.
- `editor-inline-alerts`: the reduced-features and Protected View notice.
- `dbus-automation-spine`: snapshot fields, and possibly a readiness predicate.
- `live-editor-memory-budget`: `estimated_buffer_multiplier` moves to `DocumentScale`.
- `search-replace-safety`: in-tab Replace is unavailable in Protected View.
- `gtk-axiom-ledger` and `gtk-lush-axioms`: the entries below.

## Impact

**Code**
- `model/document_scale.rs` (new, with Kani).
- `model/` paragraph-boundary helper, used by `model/buffer_replacement.rs` and `model/file_load.rs`.
- `services/file_limits.rs` (retired) and its 17 consumer files.
- `services/editor_io.rs` (the profile).
- `ui/editor_page/load/{policy,execution}.rs`.
- `ui/editor_page/document_identity.rs` (`reapply_language` gated).
- `ui/editor_page/minimap/{admission,projection_execution,retirement}.rs`.
- `ui/search_bar/execution.rs`, `ui/editor_page/search.rs:82`, `ui/window/search.rs:593`.
- `ui/editor_page/invisibles.rs`, `ui/window/print/`, `ui/window/local_history/preview_execution.rs`, `ui/markdown_preview/`, the notes editors, `services/file_peek.rs`.
- `ui/window/geometry/execution.rs` (C4).

**Workflow convention**
- A new WFR row for Protected View: role home `ui/editor_page/protected_view/`, with `policy.rs` (pure fold, density and gesture decisions), `evidence.rs`, and named coordination (`execution`, plus `find_execution`).
- The Line Inspector is a called presentation surface.
- The fold-at-insertion executor is cross-cutting coordination with no role, called by load, buffer replacement and reload.
- Re-derive the measured cells of the load, buffer_replacement, search_bar, minimap and local_history rows.
- `make check-workflow-boundaries`.

**Tests**
- Unit, property and Kani tests for the boundary function, the profile and `DocumentScale`.
- PR-gated widget tests for the precondition gates, including:
  - the caret-browsing setting on;
  - paste, undo and move-lines creating a giant paragraph, which switches to Protected View in the same emission;
  - Save As byte identity;
  - no CRITICAL on minimap detach;
  - CR-only and U+2029 slicing.
- Synthetic fixtures only. The user's file is never committed.
- `make responsiveness-smoke` sweeps:
  - paragraph lengths 2k/10k/50k/250k/1.9M;
  - many lines just under each threshold, wrap off and wrap on;
  - 5 MB of short-line HTML;
  - CR-only;
  - session restore of three or more protected documents;
  - runs with the AT bridge plus a listener (accessibility-smoke infrastructure) and with an IM that requests surrounding text.

**Docs**
- `AGENTS.md` (large files, bounded loads, minimap classifier, the new decision).
- `.agents/rules/ui.md` (large-buffer guardrails) and `build.md` (the lane).
- `docs/automation*.md`, `docs/accessibility*.md` plus matrix rows (the folded accessible text and the inspector path), `docs/end-user-coverage.md`, README features.
- `docs/next/` record for the C0–C5 programme.

**GTK axiom ledger** (after A22). Each entry gets a probe and a sample. Run `make gtk-axioms` first.
- A23: a paragraph's layout is one PangoLayout over its visible text; cost grows with visible length; there is no partial layout.
- A24: invisible chunks are left out of the PangoLayout text (gtktextlayout.c:2296-2310, **verified**).
- A25: text inserted at a tagged run's start or end boundary is untagged (toggle gravity), while text inserted strictly inside is tagged. Verified by source (**reviewer-cited**), probe pending.
- A26: log attrs are computed over the whole logical paragraph, hidden text included, and are invalidated by any char change (**verified**).
- A27: tag apply and remove do not emit `changed`, set `modified`, or record history. Verified by source (gtktextbuffer.c:3003-3030 and gtktexthistory.c, **reviewer-cited**), probe pending.
- A28: Pango glyph-width int32 overflow.
- A29: the context engine's atomic line analysis, its 2000 ms cut-off, and its reading of hidden text.
- A30: IM spot and surrounding-text cost grows with the cursor's paragraph, armed by caret moves, keys and clicks.
- A31: validation works in 2000 px batches.
- A32: `gtk_source_map_set_view(NULL)` with a pending tick causes a CRITICAL (defect axiom).
- A33: GtkSourceSearchContext: the non-regex path includes hidden text, the regex path uses visible text, and per-match cost grows with line length.
- A34: `move-cursor` with an invisible cursor moves only the viewport (gtktextview.c:6616-6657), and `gtk-keynav-use-caret` overrides it (:6336-6343). **Verified**.
- A35: AT-SPI contents and offsets with invisible text, and TextChanged payloads on insert.
- A36: segment merge copies both strings (C3 only).

## Non-goals

- Editing inside paragraphs over L_protect (C5, if ever).
- A custom virtualized text widget, or in-buffer segmentation, in C0–C4.
- Revealing a whole giant line in the main view.
- Head-hiding windows.
- Depending on upstream fixes. Upstream reports are filed, but the remedies do not wait for them.
- Changing `REFUSE_TO_OPEN`.
- Tuning for complex scripts beyond the sweep.
- GPU-specific work.
- Fold-aware printing.

## Risks

- **Protected View makes some files view-only.** If L_protect is low (about 20k by the 0.15–0.7 µs/char layout cost against a 16 ms budget), minified JS, CSS and JSON files that are editable today become view-only. That is a visible regression. The value comes from C0, and the maintainer must accept the trade-off (decision 2).
- **Unprobed toolkit assumptions:**
  - IM focus and surrounding text on a non-editable, caret-free view;
  - AT-SPI behaviour with folded tails;
  - SpaceDrawer cost.

  If a probe fails, C2 needs one more interception, or the view must not be focusable.
- **The accessibility experience.** Screen-reader users reach giant-line content only through the inspector. This needs an Orca checklist entry.
- **The budget is measured on one reference machine.** Validate on the user's session through an isolated-XDG launch, with consent.
- **Edit-time switch to Protected View.** A paste that creates a giant paragraph switches a modified buffer to view-only. Saving and drafts must still work: the buffer is modified, and only further editing is blocked. Run `data-safety` explicitly on C1 and C2.

## Review disposition

**toolkit-feasibility**
1. Log attrs: **accepted**. Verified at gtktextbuffer.c:4469-4503 and 4532-4537. This is the main reason for the redesign: the editable fold is dropped for giant paragraphs. Option (a), read-only, is adopted as Protected View.
2. IM retrieve-surrounding: **accepted, refined**. The IM reset is gated by `need_im_reset`, but caret moves, keys and clicks arm it (gtktextview.c:5785, 5853, 6856). Protected View avoids the caret path through the cursor-invisible branch, intercepts `move-cursor`, and claims clicks. Switching the view's im-module to simple is **rejected**: it would degrade CJK input in all documents and is unnecessary on a non-editable view. A probe stays in C0.
3. End and boundary insertion traps: **accepted as a defect of the editable fold**. They do not apply in Protected View (no insertion, no caret). A25 is restated with both boundaries and kept as a probe.
4. The per-paragraph invariant does not bound a dispatch: **accepted**. A density bound (V_batch, N_dense) is added to the policy, and its fixture is a gate.
5. Clipboard inconsistency: **accepted**. Copy, cut and paste are disabled in the main protected view; the inspector copies bounded windows; whole-line copy is a decision.
6. Accessibility offsets and TextChanged: **accepted**. Tails only (no head-hiding), A35 probe, a lane run with the AT bridge, and C3 conditioned partly on AT cost.
7. Phase 3 segment-merge cost: **accepted**. Moved to conditional C3 with measurements at 100–500 MB.
8. Idle refold race: **accepted**. Folding is done at insertion. Edit-time downgrades run synchronously in `connect_after("insert-text")`.
9. SpaceDrawer: **accepted**. Invisibles off while any fold exists is a hard rule; a probe stays.
10. Search and hidden text: **accepted**. No search context while protected; worker find; prefills unified (both sites verified).
11. Source-settled claims: **accepted**. A25 and A27 become "verified by source, probe pending". The display-cache statement is corrected.
12. The context engine reads hidden text, and edit-time growth: **accepted**. Edit-time monotone downgrade is in C1.

**requirement-and-conventions**
1. Log attrs: **accepted** (same as above).
2. Hidden-run cursor iteration: **accepted**. It cannot be reached in Protected View, and A34 proves it.
3. IM and AT paths: **accepted** (same as above, plus docs and matrix updates).
4. Refold on idle and the load-only profile: **accepted**. Synchronous edit-time profile in C1.
5. Enforcement and metric: **accepted, partly adapted**. GTK-internal spans cannot be counted, so the PR gates assert the preconditions that make those paths unreachable. The heartbeat is replaced by an in-process high-priority watchdog plus frame-clock timing. The timing lane stays nightly.
6. Paragraph delimiters: **accepted**. The slicer is verified to be `\n`-only. The fix is in C1, as a pre-existing blocker.
7. Data-integrity semantics: **accepted**, and resolved by making the buffer immutable in Protected View.
8. Layering:
   - **accepted** — the pure policy goes in `model/` and the executor in `ui/`;
   - **accepted** — a WFR row plus re-derived measured cells;
   - **accepted, with correction** — the `FileSizeCheck` consumer list (17 files verified), and the memory budget *is* affected;
   - **accepted** — the prefill sites are unified.
9. Missing surfaces: **accepted**. Each surface gets a bounded behaviour in C2 (print refused, logical-line actions disabled, gestures claimed, DnD off, session, eviction and monitor fixtures, search hits open the inspector).
10. Scope split: **accepted**. C0–C5 as above, with C0 first.
11. Frame-relative budgets: **accepted**. B_interact = 16 ms plus 2-frame input latency, B_settle = 50 ms as a separate named budget, values recorded as policy constants.

Nothing was rejected outright. Partly rejected: the im-module-simple mitigation, and doing segmentation now. The latter is deferred to an optional C5 because it needs an offset map across save, drafts, bookmarks, history and search.

## Decisions for the maintainer

1. **Accept view-only for giant-paragraph documents** (Protected View) as the way to meet "instant always", with editing of those paragraphs deferred to an optional C5 (segmentation or a custom widget). Or require editability now, which means C5 comes before C2 and is a much larger effort.
2. **L_protect.** Choose after the C0 sweep. A low value (about 20k) keeps edits instant but makes minified files view-only. A higher value (100k–300k) keeps them editable but allows up to about 16–50 ms of buffer-side cost per edit. Should it be a GSettings key?
3. **Budgets.** B_interact = 16 ms with input-to-frame ≤ 2 frames, and B_settle = 50 ms?
4. **Syntax total-size cutoff.** Keep 10 MB, or lower it to about 1 MiB (GNOME Text Editor's figure), given the 50–112 ms background-highlight gaps?
5. **"Copy whole line"** from the inspector: offer it (built off-GTK, possibly megabytes on the clipboard), or bounded windows only?
6. **Responsiveness capability.** A new `responsiveness-measurement` capability, or fold it into `performance-regression-coverage`?
7. **Upstream reports.** File them now? The candidates are GtkSourceMap `update_id`, Pango width overflow, quadratic `line_get_cursors`, IM spot updated on every scroll, search copying the line per match, log attrs including invisible text, and AT offsets vs invisible text.
8. **Before C2 is proposed,** run C0 against the real LushText binary through the automation client (open, scroll, find, tab switch) with the AT bridge and IBus present, so that the viability of Protected View rests on measurement.

Relevant paths:
- /var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext-core/src/model/buffer_replacement.rs
- /var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext-core/src/services/file_limits.rs
- /var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext-core/src/services/editor_io.rs
- /var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext-core/src/ui/editor_page/search.rs
- /var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext-core/src/ui/window/search.rs
- /var/home/danilo/Workspace/github/cominotti/lushtext/target/huge-file-explore/src/gtk/gtk/gtktextview.c
- /var/home/danilo/Workspace/github/cominotti/lushtext/target/huge-file-explore/src/g/gtktextbuffer.c
## Maintainer decisions recorded (2026-10-11)

1. Protected View now (view-only for giant-paragraph documents); editing those paragraphs is deferred to an optional C5.
2. Syntax total-size cutoff: decided by the C0 measurement, not fixed in advance.
3. Budgets: STRICTER than proposed. Working values: B_interact = 8 ms per interaction dispatch (120 Hz frame), input-to-presented-frame <= 2 frames, B_settle = 16 ms per settle dispatch — to confirm when C0 is proposed.
