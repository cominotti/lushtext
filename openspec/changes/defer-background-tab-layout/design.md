## Context

`spawn-completion-priority` (SC) attributed `eviction/reselect`'s 6–11 s
batches to GtkTextView validation: 69.5 % of the application's cycles in
`incremental_validate_callback`, hidden tabs included, and each load's 65 MiB
transient permit held through a C6 install paced below that validation. Its
re-verdict also left one shift unattributed (`baseline/small/steady-populated`
`type-during-settle`, worst-stall median 4.9 → 12.2 ms) and cited A69 for
GLib's equal-priority co-dispatch although A69 is the context-engine row.

Probes run first, before any design choice (task 1):

- **A82.** An `AdwTabView` page that was never selected is neither mapped nor
  realized, yet its `GtkSourceView` validates everything loaded into its buffer
  while hidden: 149 iterations, 151 ms for 16,384 lines, the same as a page
  selected once and hidden again. The layout comes from the scrolled window
  handing the view its adjustments (`gtk_text_view_set_vadjustment` →
  `set_vadjustment_values` → `gtk_text_view_ensure_layout`), and even a source
  view in no widget tree validates; only a buffer no view shows is not laid out.
  So "never select the background tabs" — the cheapest idea — cannot work.
- **A83.** `gtk_text_view_set_buffer` to an empty buffer stops a hidden view's
  validation at once (the lower source in iteration 4) and leaves the document
  buffer's text, modified flag, undo state and caret as they were. Setting the
  document back lays it out again from nothing, and the view comes back at the
  buffer start. A scroll to a kept mark queued right after the reattach lands
  at another line (100 instead of 1,024) once the lines above it get heights;
  scrolling to it again (`scroll_to_iter`) on every `upper` change keeps the top
  near it (1,009–1,019), and one more scroll once the view is laid out
  lands on it exactly. An unmapped validating view notifies `upper` on every
  batch (147 times). `set_buffer` also emits an accessible text change for the
  whole old and new contents, which AT-SPI forwards only when a listener is
  registered (read from source; the probe runner has no AT-SPI bus, A35).
- **A81.** Three ready idles at one priority run in one iteration (12.0 ms for
  three 4 ms callbacks); one step lower, or one added during the dispatch,
  waits for the next. So N validating views make every iteration N batches long.

## Goals / Non-Goals

**Goals:** no background tab lays out its document while hidden; background
documents do not delay the selected one; the selected document is usable at
once; every dispatch within budget; bytes, drafts, session, cursor, bookmarks
and search unchanged in behaviour; switching to a large tab within budget.

**Non-Goals:** shortening the visible document's own paced install (C6 decides
its pacing; it is the remaining long part of `eviction/reselect`); restyling
hidden tabs lazily beyond what the detach gives; the transient load budget's
multiplier.

## Decisions

### D1. A hidden view shows an empty placeholder; the document buffer is separate

Each editor page captures its template buffer as **the document buffer** at
construction and creates a per-page empty placeholder. At the end of
`constructed` (after every handler connected to the document buffer) the view
is given the placeholder. `WidgetImpl::map` gives it the document back before
the children map, so the first frame of a shown tab lays out its visible part
as it always did. `LushtextEditorPage::buffer()` returns the document buffer
whatever the view shows; every `source_view().buffer()` read of the document in
production code goes through it or through `live_document_buffer()` (the
evidence readers that must survive a disposed page).

Alternatives, rejected:

- *Never select background tabs* (open them without selecting, restore only the
  active page): A82 shows a never-selected page validates anyway.
- *Unrealize or unmap hidden pages*: `AdwTabView` already unmaps them (A82); an
  unmapped, even unrealized, view keeps validating (A82, A52).
- *Defer installing a hidden tab's content until it is first shown*: the
  buffer would stay empty and nothing would be laid out, but every workflow
  that reads a background document (draft restore, Replace All's open-tab
  rules, session persistence, close prompts, the command palette's open tabs,
  memory accounting, the eviction reload) would have to learn a new
  "installed later" state, a draft-bearing tab could close without its prompt,
  and the first switch to each tab would wait for a sliced install. A83's
  detach keeps every one of those exactly as it is.
- *Destroy and rebuild the hidden view*: the view carries gutters, the minimap,
  the search bar, the Protected View controllers and tens of handlers.

### D2. Detach a shown view only when it is seen laying out while hidden

A view that is hidden and attached stays attached, so switching back to a tab
that finished laying out costs nothing (its layout is kept). When its
adjustment's `upper` changes while unmapped — every 2000 px validation batch
does (A83, A31), so this catches a backlog left at the hide, an edit or reload
made while hidden, and any view-wide restyle (font, zoom, spacing, wrap) — it
is detached from an idle at the completion priority, one step above the
validation idle, so it lays out at most the batch that revealed it. A
re-validation that changes no line's size goes unnoticed and keeps validating:
the safe fallback is today's behaviour.

### D3. Keep the top line across a detach

At the hide the view keeps a left-gravity mark at its top line (or the line it
is still being held at). On attach it scrolls to that mark and is **held**
there while it lays the document out again: on every `upper` change it scrolls
to the mark again (`scroll_to_iter`), and once more from an idle one step below
validation, which dispatches only when no attached view has lines left (A80),
then lets go. The first counted input after the reattach hands the position to
the user, so a scroll or key during the re-layout is never overridden. Scroll
requests made while detached (restore position, search and bookmark
navigation, local-history restore, Protected View navigation) replace the kept
line; `visible_top_line` reads the kept line without creating any layout, so
session persistence stores the right line for a hidden tab.

### D4. Background installs wait behind the visible tab; the permit keeps only the body

`load::policy::install_slice_pacing(dense, laid_out, protected)` decides how a
slice is queued: behind validation (`PACED_SLICE_PRIORITY`,
`G_PRIORITY_DEFAULT_IDLE`) for a paced body whose view lays it out, one step
lower (`background_slice_priority()`, `G_PRIORITY_DEFAULT_IDLE + 1`) for a
paced body going into a background tab, and a 1 ms timeout for an ordinary
body. Both the file load and the whole-buffer replacement use it, and it is
re-read before every slice, so a tab selected mid-install switches at once.
The selected tab of a window that is still composing counts as laid out
(`install_follows_layout`): it is about to be shown, so it installs exactly as
before (D6). Background slices co-dispatch at their one priority (A81), but
they lay nothing out: the worst dispatch while nine of them ran was 6.7 ms.

After the worker read, the transient load permit keeps what the load still
owns beyond the live editor budget, its decoded body plus four bytes for each
character not yet installed plus the fixed overhead
(`model::file_load::install_retained_weight`; review: keeping only the body let
about 1.1 GiB of loads overlap against the 256 MiB bound, because the 8×
weight's residency term covered the buffer the install was filling), and
gives back its reading slot (`FileLoadAdmissionPolicy::retain_for_install`).
An oversize load keeps running alone until released:
`MAX_ADMITTED_FILE_LOADS` mirrors the worker cap, and an install uses no
worker. The buffer's own bytes are the live editor budget's, and the protected
residency rule still counts an installing body as a payload. This is the
"release the permit once the payload is handed to GTK" option, kept honest: the
body really is retained until the install's last slice.

Measured on `eviction/reselect` (one repetition each, `.claude/handoffs/ev/E*`,
`Q-branch*`), against `main`'s 20 s cap:

| Variant | Background installs | Selected document | Notes |
|---|---|---|---|
| D1 alone, pacing and permit unchanged | second batch waited 5.8 s behind the visible view's validation, holding 65 MiB permits | — | the 4th–10th documents are not even read meanwhile |
| background slices from a 1 ms timeout | 1 s per batch of three | — | |
| background slices at the completion priority (124), permit unchanged | all nine in 0.75 s | its layout starved while they ran | `recovery/protected-32mib` scroll input-to-frame 109–145 ms (main 18–40 ms): a 32 MiB background install held the visible view's validation idle back |
| **background slices at `G_PRIORITY_DEFAULT_IDLE + 1`, permit retains the body** | eight start installing within 0.11 s and finish 0.5 s after the visible one | 5.8 s (its own paced install, C6) | protected-32mib scroll 18–25 ms; worst background dispatch 6.7 ms |

The chosen variant gives the CPU to the visible tab first, as the goal asks,
and never stops a background document from being read.

### D5. Attribution of the `steady-populated` shift

From SC's `RV-*` artifacts (`.claude/handoffs/ev/ffe.py`): in every repetition
on both sides the worst dispatch near the start of `type-during-settle` is the
**first editor frame** of the populated window (`post_open_first_editor_frame`:
`frame_layout` ≈ 7 ms, of which `window_measure_h` ≈ 3 ms, plus `frame_paint` ≈
5 ms; 11.4–12.8 ms, 17.7 and 20.9 ms in two loaded repetitions). The phase
starts at load readiness. On `main` readiness was reported after that frame in
4 of 5 repetitions (worst 2.6–6.4 ms) and before it in 1 (16.6 ms, the frame);
on the branch before it in 4 of 5 (11.7–20.8 ms). SC delivers the load's
completion earlier, so the same frame now falls inside the window. It is not a
new or longer dispatch; the frame is within `B_SETTLE_US` unloaded. The perf
profile of that frame is in the record ("EV").

### D6. The selected document's alert waits for the focus handoff

`window_composition::test_the_focus_handoff_precedes_a_protected_documents_alert_announcement`
became flaky in the first `make test`: a protected document's "View-Only" alert
was announced before the opened document took the keyboard. That ordering had
never been enforced; it held because the document's install waited behind its
own view's layout, which ended after the handoff. A view that lays nothing out
until its tab is first shown installs sooner. Two changes, both measured: the
selected tab's paced install keeps its pacing (D4's `install_follows_layout`),
and the projection pass defers the selected editor's announcement while
`opened_document_focus_pending()` (a handoff requested and not finished, a
session restore running, or activation opens queued); the handoff's end, and a
restore or activation ending without one, project the surfaces again. Under
ten CPU spinners the test failed 2 of 5 runs without the deferral and 0 of 5
with it.

### D7. Review fixes (independent review of 31aeb90a)

- **No swap while an assistive technology listens (M2).** With an AT-SPI
  listener, `gtk_text_view_set_buffer` sends the whole old and the whole new
  text as one `TextChanged` each: measured with a PyGObject probe under the
  lane's AT setup (a tree walk realizes the contexts, as an AT does), 8 MiB
  detach 12–19 ms and attach 9–17 ms, 32 MiB 69–82 ms, 100 MiB 180–220 ms; with
  no listener detach 2.6–4.2 ms (8 MiB of 41-byte lines), 9–11 ms (32 MiB),
  25–28 ms (100 MiB, 2,557,503 lines), 0.5–1.1 ms for 100 MiB of 4,000-character
  lines, and attach under 0.2 ms at every size. `ui/at_listeners.rs` mirrors
  GTK's listener table (`gtk_at_spi_root_has_event_listeners`) on its own
  accessibility-bus connection; while it is non-empty no view is detached, so
  hidden tabs lay out in GTK's own bounded batches (CPU, no long dispatch).
  When a listener appears, a detached view of at most 2^20 characters attaches
  at once; a larger one attaches when shown (the one emission a loaded
  document sends anyway). Without a listener a document of more than 2^18
  lines is never detached (6.8–7.0 ms for 261,491 lines on the reference
  machine, under half `B_SETTLE_US`).
- **Primary selection (M1 and the hidden selection).** Protected View's
  bookkeeping records nothing while detached and the attach applies it (M1:
  entering it while detached removed an absent registration and left the
  provider on PRIMARY). A detach holds one extra reference on the document's
  registration, so GTK's removal does not clear PRIMARY
  (`gtk_text_buffer_remove_selection_clipboard` clears it when the refcount
  reaches zero and the document owned it); the attach or an unrealize returns
  it.
- **Document-buffer reads (M3, M4, should-fix).** Bookmark navigation scrolls
  through `scroll_to_document_iter`; print builds its compositor over the
  document buffer with the view's settings; held-typing replay and the search
  bar's match scroll act on the view only while it shows the document.
- **Exact position.** The kept position inside a wrapped paragraph is the
  start of the display line at the top (a walk over the paragraph's display
  lines; `iter_at_location` answers nothing over a margin).
- **A dense document's reattach is paced (found by the review verdict).**
  `eviction/switch-hidden-dense` (11 MiB of 2,000-character lines) measured the
  first switch at 27–29 ms in 3 of 5 branch repetitions against `main`'s
  17–20 ms: a reattached view lays the document out again from nothing (A83),
  and on a dense document each 2000 px batch is the cost C6's restyle pacing
  exists for. The attach now uses the restyle's hide-and-reveal
  (`restyle::prepare_paced_reattach`): only the paragraphs from the kept line
  that fit one `V_SLICE` stay visible, the rest is revealed outward at
  `G_PRIORITY_LOW` once mapped, and the kept-line hold lasts until the reveal
  is done, for documents of at most 2^14 lines: hidden lines add no height,
  so one 2000 px batch validates every hidden line (about 0.4 µs each; a
  204,601-line document took 78–80 ms in V4). Remeasured: 10.1–16.9 ms against
  `main`'s 15.7–20.3 ms; the worst dispatch is the first frame of the newly
  shown tab.
- **Round 2 (second review, 2026-11-04).**
  - The mirror parsed `EventListenerDeregistered` as `(ssas)`, but it is `(ss)`
    (at-spi2-registryd's introspection, GTK's parsing), so a listener never
    left; a pure `listener_change` parses both shapes (failing-first unit
    test). The mirror now reads as listening until the registry answers and
    for good on failure, and the answer replaces its (sender, event) set, so a
    listener registering between subscription and reply is not counted twice.
  - When a listener appears, detached editors attach one per dispatch at
    `G_PRIORITY_DEFAULT_IDLE + 1`, the selected tab first; leaving cancels.
  - **Recorded exception (SF2, 2026-11-04).** A document over 2^20 characters
    detached before a listener appeared sends its whole text once when shown.
    Re-installing it in slices through the buffer-replacement workflow would
    rewrite the buffer (undo, modified state, marks, caret) to change only what
    the AT is told, so it was not done. Measured (PyGObject probe under the
    lane's AT setup, a listener recording payload sizes): 32 MiB, 71–75 ms in
    one dispatch and one `TextChanged` of 33,554,400 characters; 100 MiB,
    182–186 ms and one of 104,857,582; 160 MiB, nothing sent (0.1 ms; over
    D-Bus's 128 MiB message limit, so the AT's text model misses the
    document). The a11y bus allows 1 GB (`accessibility.conf`). Because an
    unanswered mirror reads as listening, only an assistive technology started
    mid-session reaches this.
  - A paced reveal hid paragraphs from the in-tab search (A33): Replace All
    right after switching to a dense tab replaced 17 of 400 matches
    (failing-first). The search context reveals first, and no paced reveal
    starts while one is attached. Writing that test found a pre-existing crash:
    sourceview5 0.11's `replace_all` asserts a zero count carries an error, so
    Replace All that matched nothing panicked; the call is made directly.
  - With a minimap bound the detach walks the lines twice: the lane's
    `view_detach` span on 204,601 laid-out lines, 5.3–5.9 ms without and
    8.6–12.7 ms with the minimap (load 4.5–9), so the cap is 2^17 lines with a
    minimap bound.
- **Pacing aging.** A background slice that waited 256 ms behind visible work
  runs once at `PACED_SLICE_PRIORITY` (`input_quiet::paced_idle_aging`).
- **The activation's present.** The span-less 35–52 ms after the second open of
  `install/100000000` (V2, `bin/branch-8`) is the tail of the activation turn:
  `sync_tab_bar_visibility`, the focus handoff and `gtk_window_present`, which
  on Wayland blocks until the compositor answers an xdg-activation token
  request. The turn now presents only a window that is not active.

## Risks / Trade-offs

- **Accessibility.** With an AT-SPI listener registered, a swap sends the whole
  old or new contents (A83); no view is detached while a listener listens (D7).
  A view detached before the listener appeared and too large to attach at once
  sends its document once, when first shown.
- **Re-layout on show.** A detached tab lays out its document again when shown:
  its first frame validates only the visible part; the rest follows in 2000 px
  batches. Measured as `switch-hidden`.
- **Code that reads the view's buffer.** Anything that still read
  `source_view().buffer()` for the document would see the placeholder; every
  production site was audited (tasks 3.2), the minimap reads its map's own
  buffer consistently and refreshes on attach, and `surrounding_text` and the
  search bar act only on a shown, attached view.
- **Primary-selection bookkeeping.** `set_buffer` on a realized view moves
  GTK's primary-selection provider from the old buffer to the new one, and
  Protected View keeps it off the document; the swap restores it first and the
  attach takes it off again (found by `make test`, pinned by a failing-first
  widget test), a detach holds a reference so a hidden selection keeps
  PRIMARY, and Protected View entered while detached is applied by the attach
  (D7).
- **A view-level call with a document iterator on a detached view** would log
  a `Gtk-CRITICAL` (buffer mismatch); every scroll goes through
  `scroll_to_document_iter`, and the widget and smoke lanes fail on any
  critical.
