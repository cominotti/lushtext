## Context

C5 of the long-line stream (`docs/next/long-line-documents.md`). C0 recorded
the budgets (B_interact 8 ms per interaction dispatch, input-to-presented-frame
≤ 2 reference frames, B_settle 16 ms) in `model/responsiveness_budget.rs`; C1
(`document-scale-policy`, archived) made feature availability a function of the
paragraph profile; C2 (`long-line-protected-view`) makes giant paragraphs
view-only. On 2026-10-30 the maintainer chose the **hybrid policy**: paragraphs
up to about `L_protect` ≈ 50,000 characters stay editable, the keystrokes that
miss B_interact there are an explicit toolkit-floor exception, and C5 brings the
cost back within budget as far as the toolkit allows.

C2's `L_EDIT_STALL` sweep (`keystroke-sweep.patch`, median of the worst stall
per key, wrap on): 5.2 ms at 5k, 6.8 ms at 10k, 9.6 ms at 20k, 20 ms at 50k,
43 ms at 100k; wrap off about half below 10k; the probe input method (which
asks for the surrounding text at every point an IBus engine does) adds
1.5–2.5 ms.

### Evidence

All numbers below come from release measurement builds of `main` at 0439d4c4
plus measurement-only instrumentation (kept outside the repository with the
raw runs), on the reference lane: private `mutter --headless`
1280×800@120, `GSK_RENDERER=cairo`, Adwaita Mono 11 (14.667 px), CPUs 8–23,
caret blink off, synthetic `prose` keystroke fixtures (one paragraph of the
swept length between 30 short lines, keys at its start, middle and end, ten
keys per position, 150 ms apart). Each measurement run held the exclusive
measurement lock (`scripts/measure.sh`), one run per hold; load average and
the top processes were recorded before and after every run and other agents'
work ran beside some of them, so differences under about 1 ms are noise.

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
stays bound. A measurement build that also calls `set_buffer(None)` on the
retired, already unrealized map removed it (one layout per key remained,
no CRITICAL, no warning).

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
- Remove every per-keystroke paragraph layout that is not needed to draw the
  edited paragraph once.
- Bring typing in long editable paragraphs within B_interact where the
  toolkit allows, and say exactly where it does not.
- Make the measurement a maintained lane group, so thresholds are derived,
  not asserted.

**Non-Goals:**
- Editing paragraphs above `L_protect` (deferred; proposal "Deferred").
- Changing Pango or GTK; a custom text widget.
- Changing the user's word-wrap preference, adding a GSettings key, action,
  D-Bus member, snapshot field or persisted format.
- Caret motion, Backspace and selection cost (log attributes, A26): they are
  separate interactions; the lane group records them only if a later change
  adds those phases.

## Decisions

### D1. Release the retired minimap's buffer, and find what keeps it alive

`retire_native_source_map` unparents the map and drops LushText's reference
(ledger A45: unparenting unrealizes it, so no update tick runs), but the map
survives with one reference and its `GtkTextView` stays attached to the
document buffer through GtkSourceMap's `view` → `buffer` binding, so it
re-lays out every edited paragraph unwrapped from an idle. The fix:

1. Identify the surviving reference (a signal closure, a binding, a render-hold
   reference or a toolkit-internal one) with a failing-first widget test that
   holds a weak reference to the retired map and asserts it is gone after
   retirement and one main-loop drain.
2. Remove it where it is LushText's. Whether or not the map is finalized, the
   retirement also detaches the retired map from the document: after
   unparenting (unrealized, A45), clear the map's buffer. The measurement build
   did this with `set_buffer(None)` on the map and saw no CRITICAL; the
   implementation pins the exact call by a GTK axiom probe (D5) before relying
   on it, because A32 shows that clearing a live map's `view` with a pending
   tick is not safe.
3. Evidence: the minimap evidence surface counts retired maps still bound to
   a buffer (expected 0); the lane's keystroke group would show the second
   dispatch again if it regressed.

*Alternatives:* keep the map but `set_view(None)` (rejected: A32); never build
a map while the preference is off (worth doing, but it does not fix maps
retired by an edit-time downgrade, so it is not sufficient alone).

### D2. Candidate measurements and ranking

Lane, `prose`, plain variant unless stated, 90 keystrokes per cell; "worst" is
the median over keys of the worst stall in the key's window, "summed" the
median over keys of the summed stall time.

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

(The current-line and A37 runs ran under heavier host load; their summed
stalls rose with it. Removing the A37 queue failed the lane's fixed-defect
check, as A37 predicts.) The surrounding-text input method with the map fix:
see "Input method" below.

Ranking by measured gain, risk and UX cost:

1. **Release the retired map** — gain: one paragraph layout per keystroke
   (−28 % summed at 50k wrapped, −26 % at 100k, half the over-budget stalls
   at 100k); risk low (a retirement path, axiom-pinned); UX none. A defect fix
   under the pre-existing-blockers rule.
2. **Word wrap off above `L_wrap_edit`** — gain 2.2× (16.3 → 7.4 ms at 50k,
   39.4 → 15.8 at 100k); risk low (an existing view property; A28 bounds
   unwrapped x positions far above `L_protect`); UX cost real: long paragraphs
   scroll horizontally. Recommended, behind a maintainer decision (Open
   Question 1).
3. Everything else measured is rejected for no gain or a regression (table in
   the proposal); soft segmentation is deferred (D4).

**Combination at 50k:** map released + wrap off = 7.4 ms worst median, 8.2 ms
p90, 0 of 3 repetitions over budget at the start and middle positions and 1 of
3 at the end (plain input) — at B_interact, not comfortably under it. With
wrap on, nothing measured gets below one wrapped layout (16.1 ms in isolation):
wrapped paragraphs meet B_interact up to about 20,000 characters.

### D3. The long-paragraph wrap gate

`DocumentScale` gains `wrap_allowed`, a presentation feature that is false
when the longest paragraph exceeds `L_wrap_edit`. It follows every rule the
other presentation features follow: monotone in the profile, session-held by
`join`, applied before content reaches the view on every install path, and
downgraded synchronously by the edit-time watch (the downgrade sets the
view's wrap mode inside the `insert-text` after-handler, before GTK's commit
handler lays the paragraph out, so the edit that crosses the threshold is
already laid out unwrapped). The editor's effective wrap mode is
`word-wrap preference ∧ wrap_allowed`; the preference itself is never written
(no `settings_writes` call), so other documents keep wrapping. The
reduced-features reason is "very long lines", reported like the others.
`wrap-mode` is a default-style property, so `frame_validation.rs` already
queues the allocation its change needs (A37).

`L_wrap_edit` is derived on the lane by the reference sweep's bisection over
the keystroke group: the largest paragraph length whose wrapped keystroke
meets B_interact by the lane's existing rule (worst of plain, AT and
probe-IM). Working value from this change's runs: about 20,000 characters
(plain); with the probe IM the wrapped boundary is about 10,000 (this
change's runs; C2 measured 3,500–5,000 under different load), which the
derivation will settle.

*Alternatives:* (a) a per-paragraph `wrap-mode` tag on long paragraphs only,
so short paragraphs keep wrapping — better UX, but the tag must be
re-established when an insertion at the paragraph's first character lands
outside the run (A25) and on every split or join, and it was not measured;
recorded as a follow-up. (b) No gate, only the exception — acceptable if the
maintainer prefers wrapping to the budget (Open Question 1).

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

New ledger entries with `gtk-lush-axioms` probes and samples:

- **Commit-handler layout:** typing one character into a paragraph builds
  exactly one full line display of that paragraph, in the key dispatch
  (`set_virtual_cursor_pos`), and the frame's validation and snapshot reuse
  it. Pins the floor this change records.
- **Unparented text views keep validating:** a `GtkTextView` (or
  `GtkSourceMap`) that is unparented but alive and still attached to a buffer
  re-validates edited paragraphs from its idle; clearing its buffer after it
  is unrealized stops that without a warning. Pins D1.

Existing entries cited: A23–A26, A28, A30 (surrounding text), A32, A37, A45.

### D6. The lane's keystroke group

C2's measurement-only patch becomes maintained: the `keystroke` fixture kind
(markers at start, middle, end), the `keystroke` group (lengths, `prose` and
`css` shapes, wrap on and off), the `place-*`/`key-*`/`verify` phases and the
`verify` match counts that prove where each burst landed. Its verdict reads
the worst stall in each key's window against B_interact, and the report adds
the summed stall per key, because a defect like D1's lives in a second
dispatch that the worst-stall figure hides.

### Input method

GTK hands a surrounding-text IM the whole paragraph on caret moves and keys
(A30). The profile (probe IM, map released, 50k, wrap off) attributes the extra
cost to `gtk_text_view_retrieve_surrounding_handler`: `backward_word_starts(3)`
and `forward_word_ends(3)` run `move_multiple_steps` → `find_line_log_attrs` →
`compute_log_attrs` over the whole paragraph (2.7 ms each at 50k), about four
times per keystroke (from the key press and from the IM spot update), and the
handler then slices from the paragraph start. Probe-IM runs with the map fix
(host load average 13–30 during them, so these are upper bounds):

| Probe IM, worst median / p90 | 5k | 10k | 20k | 50k |
|---|---|---|---|---|
| Wrap on | 2.7 / 3.7 | 5.3 / 7.0 | 11.6 / 13.5 | 32.0 / 37.5 |
| Wrap off | 3.7 / 5.8 | 6.4 / 9.6 | 10.6 / 15.0 | 26.4 / 37.6 |

The IM context of a `GtkTextView` is private, so LushText cannot answer
`retrieve-surrounding` itself; the cost stays in the toolkit-floor exception
above about 10,000 characters, with an upstream report asking GTK to bound the
word-boundary search and the slice. Whether GNOME's default IBus setup asks
for surrounding text on every key is not known on this host (`ibus-daemon` is
not installed on the reference machine); Open Question 4.

## Risks / Trade-offs

- [Unwrapped long paragraphs are harder to read] → the gate only applies above
  `L_wrap_edit`, is announced once, is visible in document properties and the
  accessible description, and leaves the preference untouched; the
  per-paragraph tag alternative is recorded.
- [The retired map's surviving reference is toolkit-internal] → detaching the
  buffer still removes the layout; the weak-reference test then asserts the
  buffer is detached instead of finalization, and the leak is reported
  upstream.
- [Lane noise from other agents] → every verdict run holds the exclusive lock;
  load is recorded; a threshold boundary is re-run before it counts.
- [8 ms at 50k is met at the median, not with margin] → the exception keeps
  covering the tail; the record states p90 and the over-budget counts.

## Migration Plan

None for data. The wrap gate changes presentation only; reopening a document
re-profiles it, as for the other gates.

## Open Questions

1. **Adopt the wrap gate?** It trades wrapping for the budget above
   `L_wrap_edit`. Without it, wrapped paragraphs from about 20,000 characters
   up to `L_protect` stay in the toolkit-floor exception.
2. **View-wide or per-paragraph?** D3 recommends view-wide (measured, matches
   the other gates); a per-paragraph tag is better UX and unmeasured.
3. **Never build a map while the preference is off?** Independent of D1's
   fix; saves construction work too.
4. **How common is a surrounding-text IM?** If GNOME's default IBus engines
   request surrounding text on every key, the IM floor (about 10,000
   characters) and not the layout floor bounds most users; measuring it needs
   an IBus session, which the reference lane lacks.
