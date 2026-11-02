## Context

C5 of the long-line stream (`docs/next/long-line-documents.md`). C0 recorded
the budgets (B_interact 8 ms per interaction dispatch, input-to-presented-frame
≤ 2 reference frames, B_settle 16 ms) in `model/responsiveness_budget.rs`; C1
(`document-scale-policy`, archived) made feature availability a function of the
paragraph profile; C2 (`long-line-protected-view`) makes giant paragraphs
view-only. On 2026-10-30 the maintainer chose the **hybrid policy**: paragraphs
up to about `L_protect_edit` ≈ 50,000 characters stay editable, the keystrokes that
miss B_interact there are an explicit toolkit-floor exception, and C5 brings the
cost back within budget as far as the toolkit allows.

C2's `L_EDIT_STALL` sweep (`keystroke-sweep.patch`, median of the worst stall
per key, wrap on): 5.2 ms at 5k, 6.8 ms at 10k, 9.6 ms at 20k, 20 ms at 50k,
43 ms at 100k; wrap off about half below 10k; the probe input method (which
asks for the surrounding text at every point an IBus engine does) adds
1.5–2.5 ms.

### Evidence

All numbers below come from release measurement builds of `main` (0439d4c4,
then 10cad94a) plus measurement-only instrumentation and environment knobs
(kept outside the repository with the raw runs), on the reference lane: private `mutter --headless`
1280×800@120, `GSK_RENDERER=cairo`, Adwaita Mono 11 (14.667 px), CPUs 8–23,
caret blink off, synthetic `prose` keystroke fixtures (one paragraph of the
swept length between 30 short lines, keys at its start, middle and end, ten
keys per position, 150 ms apart). Each measurement run held the exclusive
measurement lock (`scripts/measure.sh`), one run per hold; load average and
the top processes were recorded before and after every run and other agents'
work ran beside some of them, so differences under about 1 ms are noise.

**Contamination.** Until 2026-10-31 03:00 a helper of this change's own
measurement scripts busy-waited on one CPU outside the lane's pinned set while
runs held the lock. The profile, the baseline/released-map pairs and the first
per-paragraph and view-wide runs date from then: they are comparative (both
sides of each pair ran under it), not absolute. Every run labelled **clean**
below ran after the helper was removed, at a host load average of 1–2 except
where stated.

**Profile** (per-frame `frame_layout`/`frame_paint` spans, key-press stamps,
and an `LD_PRELOAD` shim timing every Pango entry point GTK calls, with
backtraces symbolized against the GTK 4.22.5 and GtkSourceView 5.20.0
debuginfo):

| 50k, prose | Wrap on | Wrap off |
|---|---|---|
| Key press to the frame's layout phase | 17.4 ms | 8.6–10.2 ms |
| of which: one paragraph layout (`commit_text` → `set_virtual_cursor_pos` → `get_cursor_locations` → `create_display` → `pango_layout_get_extents`) | 16.5 ms | 7.6 ms |
| of which: `pango_layout_get_cursor_pos` on the cached display | 0.2–0.6 ms | 0.6 ms |
| everything else in the key dispatch (B-tree insert, LushText handlers) | < 1 ms | < 1 ms |
| Frame layout phase (validation and scroll hit the cached display) | 0.1–0.3 ms | 0.3–1.3 ms |
| Frame paint | ~0 ms | 0–2.3 ms (the whole line's glyphs) |
| Separate dispatch: retired `GtkSourceMap` `incremental_validate_callback` (unwrapped, `width=-1`, second Pango context) | 7.5 ms | 7.5 ms |
| Buffer log attributes (`pango_get_log_attrs`) while typing | none | none |

The second layout was traced to its view by interposing `g_idle_add_full` at
the validation priorities: a `GtkSourceMap` with no parent, reference count 1
after LushText dropped its own, queued by `_gtk_text_btree_invalidate_region`
from the insertion. With the minimap preference off (the default), every
editor carries one: the map built at construction is retired immediately but
stays bound. The reference that kept it alive was LushText's: the reflow
freeze, `gtk-lush-widgets`' `RenderHoldOverlay`, held its live child (the map)
strongly after the map was unparented. A measurement build that also calls
`set_buffer(None)` on the retired map removed the second layout (one layout
per key remained, no CRITICAL, no warning).

**GNOME's input path.** On GNOME, GTK 4 receives keys through the Wayland
text-input protocol (GTK's `wayland` IM module, IBus behind the compositor).
Its `commit` class handler calls `notify_im_change`, which emits
`retrieve-surrounding` on every committed character, and GtkTextView's
handler computes the cursor paragraph's word boundaries from its start: about
five `compute_log_attrs` passes per keystroke, 2.6 ms each at 50k (ledger
A54). The lane's `wayland` variant runs that module against the headless
compositor. Clean runs (minimap released, wrap on for the whole view, worst
median, ms): 5k 6.4–7.6, 10k 9.6–10.1, 20k 14.4–16.5, 50k 32.6–37.5. Every
GNOME user pays this, so the input-method floor, not the layout floor, bounds
most users.

**Isolated Pango benchmark** (same font, the lane's wrap width, a prose
paragraph, median of 9 `set_text` + `get_extents`, one CPU, under the
exclusive measurement lock):

| Variant | 10k | 20k | 50k | 100k |
|---|---|---|---|---|
| `WRAP_WORD` (the lane) | 2.6 | 5.7 | 16.1 | 39.9 |
| unwrapped | 1.5 | 2.9 | 7.6 | 15.2 |
| `WRAP_CHAR` | 6.4 | 12.9 | 34.9 | 73.1 |
| `WRAP_WORD_CHAR` | 2.6 | 5.7 | 15.9 | 39.8 |
| font features `calt 0, liga 0, kern 0` | 2.7 | 5.8 | 16.6 | 39.0 |
| fallback off | 2.6 | 5.6 | 15.9 | 39.2 |
| hyphens off | 3.2 | 6.0 | 16.2 | 39.7 |
| Noto Sans Mono 11 | 2.3 | 5.1 | 14.8 | 34.5 |

(milliseconds)

It reproduces the lane's layout cost within 10 %, so the per-key floor is a
Pango property of the paragraph, not LushText's or GTK's surrounding work.

## Goals / Non-Goals

**Goals:**
- Remove every per-keystroke paragraph layout and paragraph-length walk that is
  not needed to draw the edited paragraph once.
- Bring typing in long editable paragraphs within B_interact where the
  toolkit allows, on GNOME's input path as well as the plain one, and say
  exactly where it does not.
- Make the measurement a maintained lane group, so thresholds are derived,
  not asserted.

**Non-Goals:**
- Editing paragraphs above `L_protect_edit` (deferred; proposal "Deferred").
- Changing Pango or GTK; a custom text widget.
- Changing the user's word-wrap preference, adding a GSettings key, action,
  D-Bus member, snapshot field or persisted format.
- Caret motion, Backspace and selection cost (log attributes, A26): they are
  separate interactions; the lane group records them only if a later change
  adds those phases.

## Decisions

### D1. Release the retired minimap

`retire_native_source_map` unparents the map and drops LushText's reference
(ledger A45: unparenting unrealizes it, so no update tick runs). The map
nevertheless survived: the reflow freeze (`RenderHoldOverlay`, which covers
the map while a reflow settles) kept a strong reference to its live child. An
unparented but alive text view stays attached to its buffer and re-lays out
the paragraphs an edit invalidates from its own validation idle (ledger A52),
so every editor with the minimap off, the default, laid every edited
paragraph out a second time.

The fix: `RenderHoldOverlay` refers to its live child weakly (`live_child()`
returns `Option`), so a retired map is finalized, which detaches it from the
buffer. No `set_buffer(None)` and no `set_view(None)` (A32) is needed. The
minimap evidence counts retired maps still alive (`retired_maps_alive`,
expected 0, from weak references the retirement records). Two widget tests
(retired at construction, retired by an edit) were confirmed failing with a
temporary strong reference before the fix.

*Alternatives:* `set_buffer(None)` on the retired map (works, but leaves a
leaked widget per retirement); never build a map while the preference is off
(saves construction work, does not cover maps retired by an edit-time
downgrade; not needed once retirement finalizes the map).

### D2. Candidate measurements and ranking

Lane, `prose`, plain variant unless stated, keystrokes at start, middle and
end; "worst" is the median over keys of the worst stall in the key's window,
"summed" the median over keys of the summed stall time (milliseconds).

**Contaminated, comparative only** (both sides under the busy helper):

| Candidate | 10k on | 20k on | 50k on | 100k on | 10k off | 20k off | 50k off | 100k off |
|---|---|---|---|---|---|---|---|---|
| Baseline, worst | 2.8 | 5.7 | 16.3 | 39.4 | 1.6 | 2.6 | 7.4 | 15.8 |
| Baseline, summed | 3.7 | 8.3 | 22.0 | 56.1 | 2.6 | 5.0 | 21.3 | 50.6 |
| Retired map released, worst | 3.0 | 5.7 | 15.9 | 39.0 | 1.1 | 2.4 | 7.4 | 15.7 |
| Retired map released, summed | 3.0 | 5.7 | 15.9 | 41.3 | 1.2 | 2.6 | 19.2 | 38.3 |
| Font features off, worst | 2.4 | 5.7 | 16.2 | 39.9 | 1.4 | 2.6 | 7.5 | 15.8 |
| Current-line highlight off, worst | | | 17.6 | | | | 8.2 | |
| A37 queue removed, worst | | | 17.4 | | | | 8.1 | |
| AT bridge + listener, worst | | 5.6 | 16.4 | | | 2.7 | 8.3 | |

(Removing the A37 queue failed the lane's fixed-defect check, as A37
predicts.) Per-paragraph wrap gate (threshold 15,000 in the measurement build,
wrap preference on) against the view-wide gate (wrap off), map released, worst
median / p90: per-paragraph 50k 7.5–8.3 / ~10, the short paragraph next to it
1.9–2.4; view-wide 50k 7.8–8.8. Equal within the noise.

**Clean:**

- Applying the no-wrap tag to a paragraph's first character: median 91 µs
  (maximum 149 µs); removing it 10 µs. Crossing the threshold while typing at
  20k shows no spike at the crossing key.
- GNOME's input path (`wayland` variant, wrap preference on, per-paragraph
  gate at 15,000 in both columns, worst median by position; p90 in brackets
  where it matters):

| Length | Whole-paragraph surrounding text | Bounded surrounding text (D6) |
|---|---|---|
| 10k (wraps) | 9.1–10.1 | 6.9–7.5 (8.5–8.8) |
| 20k (unwrapped) | 6.8–11.9 | 2.6–6.7 |
| 50k (unwrapped) | 18.9–24.9 | 8.3–11.1 |

The verdict runs of this change's build are recorded in
`docs/next/long-line-documents.md` (C5).

Ranking by measured gain, risk and UX cost:

1. **Release the retired map** — one paragraph layout per keystroke; risk low;
   UX none. A defect fix under the pre-existing-blockers rule.
2. **Bounded surrounding text** — on GNOME's input path, 50k from 19–25 ms to
   8–11 ms and 10k from ~10 ms to ~7 ms; risk: GTK offers no hook, so the
   view's own handler is blocked by its data (A54 and the consumer test pin it); UX none (the input
   method sees 1,024 characters on each side; the protocol sends at most 4,000
   bytes around the cursor).
3. **Per-paragraph wrap gate** — about 2× on the gated paragraph; short
   paragraphs keep wrapping and their keystrokes are unaffected; UX cost: the
   long paragraph scrolls horizontally.
4. Everything else measured is rejected for no gain or a regression; soft
   segmentation is deferred (D4).

### D3. The per-paragraph wrap gate

A paragraph longer than `L_wrap_edit` characters is shown unwrapped; every
other paragraph follows the word-wrap preference. GtkTextLayout takes a
paragraph's wrap mode from the tags on its **first character** (ledger A53),
so a no-wrap tag (`wrap-mode` none, no other property) there unwraps it.
`ui/editor_page/paragraph_wrap.rs` applies the tag to the **whole**
paragraph, because the tag is also the hysteresis state: text inserted before
the first character is untagged (A25) and deleting the first characters
deletes their tags, and with only the first character tagged either edit
forgot that a paragraph inside the band was unwrapped (the band widget test
caught it). A paragraph is unwrapped when any character carries the tag (one
B-tree toggle search); the gate then re-tags the untagged prefix up to the
first tagged character. The pure decision is
`model/paragraph_wrap.rs`: unwrapped above `L_WRAP_EDIT`, following the view
at or below `L_WRAP_EDIT - REWRAP_MARGIN` (`REWRAP_MARGIN` = 1,024), unchanged
in between, so typing around the threshold does not relayout the paragraph on
each key.

The gate runs in the `insert-text` and `delete-range` after-handlers, in the
same emission as the edit and so before GTK's commit handler lays the
paragraph out. After an insertion it re-checks the paragraph the run starts
in, the one it ends in, and the paragraphs wholly inside the run that the
run's own text shows are longer than the threshold (a paste); after a
deletion, the one paragraph the deletion joined. That covers an insertion
before the first character (untagged text, A25), a split, a join, a paste and
an undo (which re-inserts text without tags). The handlers also run while an
installation owns the buffer, so a long paragraph is tagged before its first
layout; an installation of many short lines reads no line from the buffer.
Paragraph lengths come from the buffer's own lines (A42, A43), a few
`chars_in_line` reads per edit. Tags change no text, set no `modified` and
record no history (A27).

`DocumentScale` gains `wrap_allowed`: false once the profile's longest
paragraph exceeds `L_wrap_edit`, monotone and session-held by `join` like the
other presentation features, so the reduced-features notice, the
document-properties row and the accessible description name "word wrap of the
longest lines" with the "very long lines" reason. It reports the reduction;
the per-paragraph tags are what apply it. The preference is never written.

**`L_wrap_edit` = 8,192.** With GNOME's input path and bounded surrounding
text, a wrapped keystroke is at B_interact near 10,000 characters (6.9–7.5 ms
worst median, p90 8.5–8.8); with the plain path, wrapped typing meets it up to
about 20,000. It is a cap, so the power-of-two rule rounds
the ~10,000 break point down to 8,192, which keeps the wrapped range inside
the budget on the GNOME path with a margin, and the rewrap margin keeps an unwrapped paragraph from
wrapping again until it is at or below 7,168. The verdict run confirms it
(programme record).

*Alternatives:* (a) view-wide (`wrap-mode` none on the view while any
paragraph exceeds the threshold): equal cost on the gated paragraph, but every
short paragraph of the document stops wrapping; (b) no gate, only the
exception.

### D4. Display-only soft segmentation (deferred)

An invisible tag or a child anchor does not split a GtkTextView paragraph:
the paragraph is still one `GtkTextLine` with one PangoLayout over its visible
text (A23, A24). The only way to bound the per-key layout with wrap on is to
hide the paragraph outside a moving window around the viewport and caret (the
C2 fold, made dynamic). Its layout cost would scale with the window
(≈ 0.3 µs per wrapped character), but it changes the paragraph's height (the
scrollbar and scrolling no longer reflect the text), GTK moves the caret over
invisible text (arrow keys at the window edge would jump to the next
paragraph), every re-window re-lays out the window (A24: showing hidden text
costs a layout), and the buffer log attributes still span the whole paragraph
(A26). Not prototyped; deferred with a re-open trigger (the maintainer
requires wrapped 50k paragraphs within budget).

### D5. Toolkit facts and the ledger

New ledger entries, each with a `gtk-lush-axioms` probe and sample:

- **A52**: an unparented but alive `GtkTextView` (a `GtkSourceMap` too) stays
  attached to its buffer and re-lays out edited paragraphs from its own idle;
  dropping its last reference finalizes it and detaches it without a warning.
  Pins D1.
- **A53**: a paragraph's wrap mode comes from the tags on its first
  character; text inserted before that character is untagged. Pins D3.
- **A54**: GtkTextView answers `retrieve-surrounding` from the cursor
  paragraph's start, and its handler, on the view's key-controller
  input-method context, stops the emission, so a handler connected later is
  never called: the answer can only be replaced by blocking it. The match by
  signal and data needs `unsafe`, which `gtk-lush-axioms` forbids, so the
  consumer widget test pins it (its answer would be the whole paragraph if the
  match found nothing). GTK's Wayland module requests
  surrounding text on every commit (source-cited and lane-measured; the probe
  runner uses the simple input method). Pins D6.

The commit-handler layout itself (one whole-paragraph layout per key, reused
by validation and paint) is recorded from the profile in the programme record
rather than probed: it is the floor this change measures, not a behaviour
LushText relies on.

Existing entries cited: A23–A28, A30, A32, A37, A42, A43, A45.

### D6. Bounded surrounding text

GTK offers no way to answer `retrieve-surrounding` for a GtkTextView: the
view's input-method context is private and its handler is connected with the
view as data. The view's key controller exposes the context
(`gtk_event_controller_key_get_im_context`), so
`ui/editor_page/surrounding_text.rs` blocks the handlers on it that match the
signal and the view (`g_signal_handlers_block_matched`; one in practice) and
connects its own: the window of `SURROUNDING_WINDOW_CHARS` (1,024) characters
on each side of the cursor within the cursor's paragraph, the selection's
other end included when it is in the paragraph and at most
`SURROUNDING_MAX_SELECTION_CHARS` (8,192) away (`model/surrounding_text.rs`,
pure). If the context or the handler is not found (a GTK change), the editor
logs a warning and GTK's behaviour stays. The one `unsafe` block is that
signal match, with its SAFETY argument beside it.

### D7. The lane's keystroke group

C2's measurement-only patch becomes maintained: the `keystroke` fixture kind
(markers at start, middle and end, and a short paragraph right after the long
one), the `keystroke` group (lengths, `prose` and `css` shapes, wrap on and
off), the `place-*`/`key-*`/`verify` phases and the `verify` match counts that
prove where each burst landed. The report adds a "Keystrokes" table: per case
and position, the worst stall in each key's window and the summed stall per
key against B_interact, with a not-verified flag, because a defect like D1's
lives in a second dispatch that the worst-stall figure hides.

## Risks / Trade-offs

- [Unwrapped long paragraphs are harder to read] → only paragraphs above
  `L_wrap_edit` are affected, the rest of the document keeps wrapping, the
  reduction is announced once and shown in document properties and the
  accessible description, and the preference is untouched.
- [Unwrapped paragraphs scroll horizontally while typing at their right edge]
  → GtkTextView animates that scroll, and every animation frame queries the
  caret position twice at a cost that grows with the caret's offset in the
  line (about 2 ms per frame at the end of 50,000 characters). No dispatch
  crosses B_interact from it, but the summed stall per key at the end of a
  50,000 / 100,000-character paragraph rises on the plain path (27 / 60 →
  32 / 73 ms; it still falls on GNOME's path). Recorded as part of the
  toolkit-floor exception.
- [Blocking a GTK-internal handler] → matched by signal and data, pinned by
  A54's probe and the consumer widget test on every toolkit update; when it is
  not found, GTK's behaviour
  stays and a warning is logged.
- [Lane noise from other agents] → every verdict run holds the exclusive lock;
  load is recorded; contaminated runs are labelled and used only
  comparatively.
- [8 ms is met at the median, not always at p90] → the exception keeps
  covering the tail; the record states p90 and the over-budget counts.
- [50,000 at the budget, 100,000 at 16–20 ms per key] → the toolkit-floor
  exception, **accepted by the maintainer** (C2's hybrid decision of
  2026-10-30, "editable up to ~50k"). Since C2 merged (2026-11-01), paragraphs
  above `L_protect_edit` = 50,000 are in Protected View and take no keys, so
  100,000 is no longer an editable length; the lane's keystroke group sweeps
  up to 49,950.
- [The wrap gate and Protected View] → they compose: a protected paragraph is
  longer than `L_wrap_edit`, so its visible head is shown unwrapped like any
  long paragraph, its hidden tail is never laid out (A65), and the gate's tag
  changes happen only inside edit emissions, never between the edit entry's
  capture dispatches (A66).

## Migration Plan

None for data. The wrap gate changes presentation only; reopening a document
re-profiles it, as for the other gates.

## Resolved Questions

The maintainer decided (2026-10-31): adopt the retired-map fix failing-first
with an axiom; use the per-paragraph wrap gate if measurement supports it (it
does, D2); and measure whether GNOME's input path requests surrounding text on
every key (it does, Evidence and A54), so its floor enters the policy through
`L_wrap_edit` and bounded surrounding text (D6).
