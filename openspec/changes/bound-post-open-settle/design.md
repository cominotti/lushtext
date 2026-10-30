## Context

C6 was recorded by C0's budget verdict (2026-10-11, idle re-confirmation 2026-10-13) as
`bound-post-open-settle`: bound the first main-loop turns after a document's load readiness.
C0 named three symptoms:

- the 12–20 ms post-open stall of a 400-byte `.txt`;
- the size-independent ~42 ms first highlight of an HTML document in a fresh process;
- the first-wheel overlay-indicator frame on an unscrollable document.

Its gate: the baseline fits `B_settle` and 2 reference frames on an idle reference run. Since
then:

- **C4, C4b and C4c** (archived 2026-10-20 and 2026-10-29) moved window composition into
  bounded, frame-gated dispatches. The activation's file opens now run after the first painted
  frame, in a turn of their own.
- **C1** `document-scale-policy` (implemented on its branch, not merged) gates syntax, minimap,
  invisibles and search highlighting by paragraph scale through `DocumentScale`.
- **C2** `long-line-protected-view` (revised 2026-10-30, commit 15cf3c17 on
  `review-long-line-protected-view`) enters Protected View only above a measured
  `L_EDIT_STALL`. Density no longer enters, so dense documents made of short lines stay
  editable. C2's open question 11 handed their validation cost to C6 (coordinator decision,
  2026-10-30).

Budgets are read from `model/responsiveness_budget.rs`, never restated in code:
`B_INTERACT_US` (8 ms), `B_SETTLE_US` (16 ms), `REFERENCE_FRAME_US` (120 Hz),
`INPUT_TO_FRAME_FRAMES` (2).

"Post-open settle" here means every main-loop dispatch from the activation turn that creates
an editor page until the document is settled. Settled means:

- the document is installed and published;
- the language is attached;
- GtkSourceView's highlighting has gone quiet;
- GtkTextView's validation is complete;
- no post-open timer, debounce or worker completion is pending.

It covers opens from activation, the Open dialog, the recent popover, the sidebar, session
restore and draft recovery.

### Measured breakdown

**Instrumentation.** A temporary patch (reverted before this commit; the scratch scripts are
named in the report) added `C6Span` spans to every post-open function the load and window code
calls:

- `open_document_with_intent_and_planning_terminal` and `LushtextEditorPage::new`;
- `reapply_language`;
- install direct and install slice;
- `complete_loaded_installation`, and every statement in it that could be heavy;
- the file-loaded callbacks, `start_file_monitor` and `seed_local_history_*`;
- `refresh_minimap`, `refresh_accessibility_metadata` and `fire_content_republished`;
- `apply_formatting_settings` and the EditorConfig completion;
- the debounced session and recent-documents saves, the palette index start and install, and
  `refresh_preview`;
- the draft autosave tick, the notification sweep, the overscroll idle and `notify::is-active`.

It also added a mark on GtkSourceBuffer `highlight-updated`, and recorded the layout, update
and paint phases of every frame (the probe records only the first four by default).

**Runs.** Each run used `make responsiveness-smoke`, tier `full`, `plain` variant,
3 repetitions, phases `background,scroll`, CPUs 8–23, under the measurement flock:

- `a2`: `baseline/empty`, the five `baseline/small*` profiles, and `total-size` 16 kB, 1 MB
  and 5 MB as `.html` and `.txt`;
- `a3`: the same, plus `dense` 120, 500 and 710, wrap on and off.

Both runs were on 2026-10-30 at 03:08 and 03:16 −03:00, with load 1.0–3.7 and storage not a
bottleneck. They are **exploratory, not a verdict**. They ran on `main` **without C1**. On C1,
syntax is already off above `S_SYNTAX` (1 MiB), so the 5 MB `.html` highlight rows do not
occur there. Task 1.4 re-measures on C1 (review R11). Run `a1` was discarded because two builds
overlapped it. Times are from the lane's spawn of the process; "dispatch" is the watchdog's
lateness, the upper bound on one main-loop dispatch.

The table attributes every post-open dispatch over 3.5 ms:

| # | Source | Where | Dispatch (ms) | Over B_settle? |
|---|---|---|---|---|
| S-act | Activation turn, `.txt` | `open_document`: `LushtextEditorPage::new` 6.2–8.2, `reapply_language` 3.9–4.2 (first `LanguageManager` use + guess), rest ≈ 1 | 9.5–13.0 (spans 11.2–13.8) | no, narrow |
| S-lang | Activation turn, first `.html` in the process | `reapply_language` 36.3–39.7 → `set_language(html)` parses html, css, js definitions | 42.1–48.0 | **yes** |
| S-frame1 | First editor frame | frame layout 6.1–8.6 + paint 1.7–2.5 | 6.5–10.1 | no |
| S-direct | Direct install, 1 MB | `install_loaded_direct` 11.2–12.0 (insert ≈ 10.5, publish 0.7–1.1) | 9.6–11.9 | no, narrow |
| S-paint | First paint after install | full screen of text; dense 500/710 | 5.4–7.6; 7.5–11.6 | no |
| S-hlon | Publish after sliced install, 5 MB `.html` | `buffer.set_highlight_syntax(true)` 70.6–70.8 | 70.0–71.6 | **yes** |
| S-hl1 | First highlight idle, 1 MB `.html` | GtkSourceView idle (one `highlight-updated`) | 24.6 | **yes** |
| S-hlbg | Background highlight worker, 1 MB / 5 MB `.html` | GtkSourceView idle, one `highlight-updated` each: 4 dispatches at 1 MB, ≥ 13 at 5 MB (continuing past the 2 s phase), and during scroll | 40.3–47.0 | **yes** |
| S-hlinst | Highlight idles between 5 MB install slices | GtkSourceView idle | 11.0–12.4 each | no |
| S-dense | Validation after a dense open, no LushText code | GtkTextView validation idles: 500 wrap-off 14.8 then 9.6; 710 wrap-off 21.4–21.6 then 14.0–14.1; 710 wrap-on 14.2 | up to 21.6 | **yes** (710) |
| S-trans | End-of-transition relayout ≈ 200 ms after composition | frame layout 3.8–7.8, plus ≈ 3 ms of idle work just before it; in every case, the empty window included | 4.1–14.8 (dense 13.0–14.8) | no, narrow |
| S-flush | First settings flush, fresh keyfile profile only (the empty window included) | `settings_flush` 0.2–0.6, then an unattributed dispatch | 4.4–14.9 | no, narrow |
| — | Every LushText projection and bookkeeping step | file monitor, local-history seed, minimap refresh, accessibility metadata, preview republish, draft check, recent documents, sidebar rows, popover rows, status bar, palette sources, memory policy, EditorConfig apply, session save, overscroll | each < 0.3 | no |

Notes:

- **The small `.txt` post-open stall is met on `main`.** The worst post-open dispatch of
  `baseline/small` is 14.9 ms (the S-flush follow-up, fresh keyfile). In the other profiles it
  is 9.5–13.0 ms (the activation turn). The C4c verdict's interleaved set agrees: background
  phase at most 10.96 ms. C0's 12–20 ms stall overlapped window construction, which C4b and
  C4c removed.
- **The "first highlight" is the language-definition parse.** Highlighting itself is not the
  cost. It happens in the activation turn, before any text exists, and costs the same at 16 kB,
  1 MB and 5 MB.
- **The overlay-indicator frame no longer reproduces as C0 described it.** The wheel on
  `baseline/empty` now draws no frame (input-to-frame not measured). Scroll input-to-frame on
  every document is 20.1–25.0 ms. That is the lane's double-buffered 120 Hz floor (16.7–19.3 ms)
  plus a full-screen paint. No document size or post-open state moves it. It is
  `long-line-evolution.md` candidate 5 (a presentation-capable lane), not C6. Coordinator
  decision 2026-10-30: input-to-frame is reported, not gated, and the floor is documented
  (D8).
- **The lane's phase boundary is unreliable for post-open work (pre-existing).** `open` ends
  when `file-open-complete` is true. In 9 of 85 repetitions (`a2`: 5 of 34, `a3`: 4 of 51)
  that was before the activation's open had finished, so the activation turn and the first
  editor frame fell in `background` instead of `open`. C6's table therefore reads stalls by stage span, not by
  phase.
- **Giant paragraphs.** C1's 1.9M-character lane report
  (`rr-20261030T014134Z-toolbx-3997359`) shows 300–780 ms dispatches in every phase. Each is
  a layout of the giant paragraph: open, every validation batch, click, scroll, find and
  typing at its end. They are excluded here and owned by C2.

## Goals / Non-Goals

**Goals:**

- Every post-open dispatch, from the activation turn until the document is settled, fits
  `B_settle`. This covers every opening path, the four construction profiles, and the lane's
  `baseline`, `total-size` (16 kB–5 MB, `.txt` and `.html`), `dense` (120/500/710, wrap on and
  off), Markdown-with-preview and EditorConfig cases.
- No input waits behind post-open work for more than `B_interact`, measured from the event's
  compositor time to its handler.
- The post-open stages are attributed in the lane report by span, so a regression names its
  source.
- Dense documents of short lines stay editable, and their post-open validation stays within
  the budget (the C2 handoff).
- On a dense editable document, a user-started invalidation (word-wrap toggle, zoom step, font
  change) re-validates in dispatches of at most `B_settle`, and its own interaction dispatch
  stays within `B_interact`. This was added by coordinator decision 2026-10-30 (D9).

**Non-Goals:**

- Pango layout of giant paragraphs, Protected View, and the recovery of giant single-line
  drafts: C2 (D13), with the residual insert going to C3.
- Window construction and composition: C4/C4b/C4c, all met.
- Gating input-to-frame. The headless lane's double-buffered floor (D8) makes the 2-frame
  limit unmeasurable there, so it stays with `long-line-evolution.md` candidate 5.
- Patching GtkSourceView or GTK. Upstream reports are filed instead.

## Decisions

### D1. Post-open stages are named, spanned and gated

The measurement build gets a permanent `post_open_*` span per stage:

- `activation`, `editor_construct`, `language_catalog` and `language_attach`;
- `install_direct` and `install_slice`, `publish` and `highlight_attach`;
- `first_editor_frame` (the layout and paint of the first frame after the page is mapped);
- `validation_idle`. GTK's own validation idles cannot be wrapped. The probe records each
  `notify::upper` change of the editor's vertical adjustment as a mark, and the lane attributes
  an unspanned dispatch that ends at such a mark to validation.
- `highlight_idle`, as a mark on `highlight-updated` with the same attribution rule.

The spans are recorded through `ui::responsiveness_probe::record_span`, an empty inline function
without the `responsiveness-probes` feature, the same mechanism as the construction spans. The
frame-phase recording keeps its default limit of 4 frames. The lane instead asks for a longer
window: `LUSHTEXT_RESPONSIVENESS_FRAME_PHASES` sets the number of frames with phase spans, and
is set only by the lane's post-open cases.

Settle is read from the app, not from a fixed sleep. A new readiness predicate,
`document-settled`, holds when all of these hold for the selected editor:

- `file-open-complete` holds;
- the language attach stage has run;
- no load slice, publish, or post-open debounce is pending;
- the vertical adjustment's `upper` has not changed for one validation-idle period;
- no `highlight-updated` has fired for the same period;
- a `G_PRIORITY_LOW` sentinel idle has dispatched after the last such mark (review R25).
  GTK's validation idles and GtkSourceView's update idles have higher priorities, so the
  sentinel can only run when none of them was ready.

The period is a named constant `SETTLE_QUIET` in the settle policy, 100 ms: longer than one
2000 px batch interval, shorter than the debounce windows, and longer than any gap S3 measures
between context-engine slices. This is the only time-based clause,
and it says "quiet", not "complete", because GtkSourceView and GtkTextView expose no
completion signal. Debounces started by user edits during settle are not post-open work and do
not hold the predicate (R26). The lane's background phase becomes "until settled, capped".
The driver computes "settled" from the same marks and sentinel records in the probe log, so a
baseline build without the predicate is measured identically (R24).

*Alternative rejected:* keeping the 2-second background phase. The 5 MB HTML background
analysis runs past it (to 4.3 s), so the phase cut the measurement short and hid the dispatches
at its tail.

### D2. Language catalog and language attach

Two costs sit in the activation dispatch:

- **The first `LanguageManager` use** (3.9–4.2 ms) loads the metadata of every language file
  and guesses from the path. Every first open of a process pays it.
- **Attaching a language with embedded definitions** (36.3–39.7 ms for HTML) parses the
  definition files and compiles their regular expressions. Every first open of that language
  in a process pays it again, because the parsed data lives on the language object and is
  released with the last buffer using it (to be confirmed by axiom S1).

The decision has two stages.

1. **A language catalog built on a worker.** An application-owned `LanguageCatalog`
   (`ui/language_catalog.rs`, cross-cutting, no role) owns a private
   `sourceview5::LanguageManager`. A worker constructs it and loads its language list. It
   starts at application `startup`, through `spawn_blocking_then`, so it runs while the window
   is being constructed and is ready before C4c's activation turn (review R3). It is handed to
   the GTK thread and used only there afterwards. Every LushText caller of
   `LanguageManager::default()` uses the catalog instead:
   - `reapply_language`;
   - `document_language_id`;
   - the Markdown preview's code blocks (`markdown_preview/code_blocks.rs`).

   This is a thread-confined handoff: the object is never used on two threads, and the worker
   drops its reference before the handoff. The path guess for a document runs on the load
   planning worker against an immutable glob table copied from the catalog. It returns a
   language **id**, not an object. A unit test compares that guess with `guess_language` for
   every glob of every shipped language and a list of tricky names. At attach time the GTK
   thread's `guess_language` is authoritative (R4).
2. **The definition parse leaves the activation dispatch.** Every site that sets the language
   on C1 calls one `request_language_attach` instead (R1):
   - `set_file_path_for_pending_load`, through `republish_document_identity` →
     `reapply_language`;
   - `apply_document_scale_gates` at publish;
   - the buffer-replacement terminal that restores a cleared language;
   - the scale-change path in `editor_page/document_scale.rs`.

   `request_language_attach` records the wanted id and schedules the attach stage. The stage is
   a `G_PRIORITY_DEFAULT_IDLE` dispatch after the editor's first frame. An editor that is not
   mapped, such as a session-restore tab that is not selected, defers it until it is first
   mapped (R8). If a sliced install is still running, the stage runs between two slices (R9).
   Clearing a language for the scale gate stays immediate, because removing a language costs
   nothing (A27).

   While the attach is pending, `document_language_id` returns the recorded id, so the
   Markdown preview, the status bar and the properties panel already know the document's kind
   (R2). Until the attach stage and the first highlight idle have run, the document shows
   unhighlighted text, and bracket matching does not yet skip brackets inside strings and
   comments (H1).

   Whether the parse can also leave the GTK thread is decided by axiom **S1**. A worker creates
   a **private** language manager, takes the language from it and attaches that language to a
   throwaway `GtkSourceBuffer` that has no style scheme. It then hands the language and the
   buffer to the GTK thread. From then on the catalog maps that id to the handed-over language
   object. No language object that GTK-thread code can reach is ever touched by a worker (R5).
   The parse runs on the worker only if S1 shows all of these:
   - the parsed definitions are shared through the language object;
   - a language from a second manager attaches and highlights exactly like the catalog's own;
   - nothing on that path touches GDK, the default style manager or the default style-scheme
     manager.

   The throwaway buffer is then kept for the process lifetime per language used, which bounds
   memory by the number of languages.

   **If S1 fails (coordinator decision 2026-10-30).** The parse stays one GTK dispatch, 36–40
   ms for HTML once per language per process, but it is moved where input cannot be waiting
   behind it.

   The attach stage then runs from a `G_PRIORITY_LOW` idle, so no input event source can be
   ready when it is dispatched, events being `G_PRIORITY_DEFAULT`. It runs only after the
   document's first editor frame, and only once `INPUT_QUIET` has passed with no input event on
   the window. `INPUT_QUIET` is a named constant in the settle policy, two reference frames,
   derived from `REFERENCE_FRAME_US`. The window's capture-phase input timestamp is the source.
   Only key presses, button presses, scroll starts and touch begins count. Pointer motion,
   crossing events and kinetic-scroll continuations do not, because they can arrive every frame
   for as long as the pointer moves (R6). A ceiling, `ATTACH_DEADLINE` (a named constant,
   2 s from the editor's first frame), bounds the wait. After it, the stage runs at the next
   `G_PRIORITY_LOW` dispatch whatever the input.
   Input that arrives during the parse waits for it, which no scheduling can prevent. The
   verdict reports those events on the exception's line and exempts them from `B_INTERACT_US`
   (R7, D8).

   The parse is recorded as a **toolkit-floor exception** against axiom S1, never accepted
   silently. This follows the precedent C4b set for A46's realize floor. The record holds:
   - the measured parse cost per language on the reference machine;
   - the lane case that shows it as the only post-open dispatch over `B_settle`;
   - the drafted upstream report
     (`.claude/handoffs/c6-upstream-gtksourceview-language-parse.md`: a language whose
     definitions can be parsed ahead of attach, or off the GTK thread).

   The verdict reports that dispatch separately and gates every other dispatch.

*Alternatives rejected:*

- Parsing at startup in an idle. It is still a 36 ms dispatch, and it is paid by sessions that
  never open HTML.
- A per-process "warm buffer" without a worker. It removes only the second open's cost.

### D3. Highlight attach is not a document-sized dispatch

The sliced install turns highlighting off at its start (`begin_load_installation`) and on at
publish. Turning it on over 5 MB costs 70.6–70.8 ms. Axiom **S2** measures the cost of:

- `set_highlight_syntax(true)` against buffer size, with and without a language;
- `set_language` on an empty buffer followed by sliced inserts;
- the highlight idles those inserts schedule.

The publish order is then chosen so that no publish step is O(document):

- **Preferred:** keep the engine attached through the install, with its idle work bounded by
  D4's policy.
- **Fallback:** attach the language on the empty buffer before the first slice, if S2 shows
  that inserts into an attached buffer only invalidate.

A pure function in the load policy, `highlight_attach_plan(size_check, scale)`, records which
order applies.

The attach stage of D2 runs after the first editor frame. If the install is still sliced at
that point, the stage runs between two slices and the remaining slices run with the engine
attached (review R9). On C1, syntax is off above `S_SYNTAX`, so the 70.6 ms publish of the 5 MB
case does not occur there. D3 applies to sliced installs up to `S_SYNTAX`, which since D6
include small dense documents (R11).

### D4. Background highlighting: a measured total-size gate (coordinator decision 2026-10-30)

GtkSourceView's context engine analyses in idle slices whose length is a compile-time constant
in GtkSourceView. The measured dispatches are 40–47 ms. Axiom **S3** pins this with the slice
constant cited from the GtkSourceView source of the verified version. LushText cannot shorten
the slices.

A document whose full analysis needs more than the first slice therefore stalls for 40–47 ms
per slice. The 16 kB HTML document did not; the 1 MB one did, four times, and so did the 5 MB
one. Those stalls also come between scroll events: 47 ms, input-to-frame 52 ms.

Budgets are never relaxed (coordinator decision 2026-10-30; the maintainer may override).
LushText measures `S_BG_HIGHLIGHT`: the largest
short-line `.html` total size whose background analysis keeps every dispatch within `B_settle`.
It is bisected on the lane in the same way as C0's thresholds. Above it:

- `DocumentScale` reduces syntax highlighting with C1's existing reason, `TotalSize`;
- the status surface and the document properties say so, as C1's reductions do.

C1 already has this gate (review R10). `DocumentScale::for_profile` turns syntax and bracket
matching off above `S_SYNTAX` with `ReductionReason::TotalSize`. Its doc comment records
`S_SYNTAX` (1 MiB) as **not derived**, a conservative candidate. D4 therefore adds no input and
no reason. It **derives `S_SYNTAX`**: the constant takes the bisected `S_BG_HIGHLIGHT` value, and
its doc comment cites the derivation in place of "not derived". If C1 is still unmerged, the
edit is made on C1's branch. C1's Kani harnesses are re-run, and their statements do not change.

One context-engine slice is already over `B_settle`, so the threshold is wherever a second
slice becomes necessary. For HTML that may be far below 1 MiB, and the cost differs by language
(R12). Whether one language-independent value is acceptable is open question 2.

An upstream report, drafted at `.claude/handoffs/c6-upstream-gtksourceview-worker-slice.md`
for the maintainer to file, asks GtkSourceView for a configurable slice or a deadline-aware
worker. If that lands, the gate is revisited.

*Alternatives:*

- Accepting 40–47 ms slices for large HTML. Rejected: budgets are not relaxed.
- Highlighting only the visible region. GtkSourceView has no public API for it.
- Running our own tokenizer. Out of scope.

### D5. A measured direct-install threshold

The direct path installs up to 1 MiB in one turn: 11.2–12.0 ms at 1 MB idle, narrow under
load. Two places read the threshold:
- `load/policy.rs::requires_chunked_install`;
- `model/buffer_replacement.rs::SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES`, the cross-cutting
  constant shared with whole-buffer replacement.

The constant becomes the measured largest size whose insert fits half of `B_SETTLE_US`. Half,
because the publish and the first frame follow in the same frame interval. Its derivation (task
3.1) is recorded beside the constant, and the value is never restated. A document above it
takes the existing paragraph-aligned slices of `REPLACEMENT_INSERT_SLICE_BYTES` (256 KiB, about
2.9 ms per slice measured at 5 MB). The scenario "Small load remains direct" keeps its meaning,
with the new threshold.

### D6. Dense documents: pace validation through the install, and stop invalidating all lines

GtkTextView validates invalid lines in 2000 px idle batches (A31). The cost of a batch grows
with the visible characters it holds: dense 710 wrap-off is 21.4–21.6 ms. LushText cannot
shrink the batch, but it controls which lines are invalid when a batch runs.

1. **A slice plan weighted by visible characters.** Load and whole-buffer replacement (draft
   recovery, local-history restore, eviction reload) cut their slices with one function,
   `model/buffer_replacement.rs::next_replacement_boundary`. On C1's branch it delegates to
   `model/paragraph_boundary.rs`, the GTK-matching paragraph boundary. It gains a second
   budget: at most `V_SLICE` visible characters per slice, beside the 256 KiB byte budget.
   Slices still end at paragraph boundaries. One paragraph larger than either budget still goes
   in one turn: the existing exception, owned by C2/C3 for giant paragraphs.

   After each slice, the validation idle has at most `V_SLICE` characters of new lines to lay
   out. `V_SLICE` is measured by axiom **S4** as the largest visible-character count whose
   validation dispatch fits half of `B_SETTLE_US` in both wrap modes. "Visible characters" are
   Unicode scalar values excluding paragraph delimiters, as C1's profile counts them.

   **Which documents take the visible budget (review R13).** A direct install's validation cost
   is per 2000 px batch (A31), so what matters is the characters in one batch, not in the
   document. A document is routed to the sliced path for density when
   `batch_exceeds_slice(summary)` holds, that is `summary.worst_batch_chars > V_SLICE`, over
   the `ProfileSummary` that C1's load worker already computes. D9 uses the same predicate. A
   document of short lines below the byte threshold stays direct, however many characters it
   has.

   **Validation runs between slices (R14).** GtkTextView validates in idles at
   `GTK_TEXT_VIEW_PRIORITY_VALIDATE`. If the next slice were dispatched first, the invalid lines
   of several slices would add up, and one batch would cost as much as before. A dense install
   therefore queues its next slice from the frame clock's `after-paint`, after the frame that
   laid out the previous slice. S4 checks that each validation dispatch then holds one slice.

   A37 is unchanged. Every slice is an edit, so `frame_validation.rs` queues the allocation
   before the paint, and validation of the slice runs in layout, bounded by the same plan.
2. **No post-open invalidate-all from LushText on dense documents.**
   - The end-of-document overscroll sets the view's `bottom-margin`, a default-style property
     that invalidates every line (A37 investigation, 2026-10-14). On a document that is not
     dense, that invalidation is within budget, and `bottom-margin` stays as it is. On a dense
     document (`batch_exceeds_slice`), axiom **S7** decides between two mechanisms (review R16):
     - **The tag:** a `pixels-below-lines` tag on the last paragraph. It is used only if S7
       shows that all of these hold:
       - the empty last paragraph of a file ending in a newline carries the space, although a
         tag on an empty range is a no-op;
       - the current-line highlight, the bookmark gutter, the minimap markers and
         `scroll_to_iter` do not see a 75 %-viewport-tall last line;
       - the minimap no longer mirrors `bottom-margin`, and does not get the space twice;
       - printing is unaffected;
       - an Enter at the end moves the tag in the same dispatch, so no frame shows the space
         between two lines.
     - **The fallback (expected):** `bottom-margin` is set once before the first install slice,
       from the view's allocated height. Later resize-driven changes of a dense document's
       margin are deferred to a `G_PRIORITY_LOW` dispatch. If that dispatch exceeds `B_settle`,
       it is recorded as a toolkit-floor exception against S7.

     *Alternative:* the tag on every document. Rejected for the reasons above, which S7 checks
     only where it would matter.
   - EditorConfig resolution runs inside the load planning worker instead of as a separate
     `spawn_blocking_then`. Today its result can arrive after the install and change
     `tab-width`, which invalidates every line. The load joins it before the install. The read
     completion waits for the resolution, which is a bounded walk over `.editorconfig` files done
     by the same worker. The formatting is applied **after any clear of old text has completed
     and before the first insert slice** (R17). Applying it before the clear of a reload would
     invalidate the old text. If the resolution has not returned by the time the install would
     start (a slow filesystem), the install does not wait. The formatting is applied after
     publish: through D9's paced mechanism on a dense document, directly on any other. Four more
     paths are covered:
     - Save As, rename and the `use-editorconfig` toggle re-resolve after the install, and on
       dense documents they go through D9.
     - Draft recovery and local-history restore install through whole-buffer replacement and get
       the same join.
     - A paced late `tab-width` re-applies a pending session-restore position once it has
       reached the restored line (R18).
3. **User-started invalidations** (word wrap, zoom, font, tab width) on dense documents are
   C6's too, through D9.

Kani: the two-budget boundary gets harnesses in `model/buffer_replacement/kani_proofs.rs`
(declared `#[cfg(kani)] mod kani_proofs;`). They go in a new shard `core-post-open-settle`,
gate `scheduled`, with its CI budget measured from two dispatched runs, plus entries in
`ORACLES`. The slice-end decision is factored so that it reads an abstract sequence of
`(bytes, chars)` paragraph lengths. The text scanner that produces those lengths is pinned
by C1's own per-character harnesses and its `pango_find_paragraph_boundary` agreement test, not
by these harnesses (review R15). They prove, for any paragraph sequence of up to six paragraphs
with symbolic lengths and symbolic budgets:

- every slice is non-empty and makes progress;
- every slice ends at a paragraph boundary;
- every slice is within both budgets unless it is a single over-budget paragraph;
- the slices cover the input exactly.

A `should_panic` harness pins the visible-character budget: without it the plan exceeds
`V_SLICE`.

### D7. The fresh-profile flush follow-up and the end-of-transition relayout

Both are within `B_settle` but narrow (14.9 ms and 14.8 ms). Neither is attributed to a
LushText span yet.

- **S-flush.** After the first `apply()` of a fresh keyfile profile, which creates the file,
  an unattributed dispatch follows. Axiom **S5** checks whether creating the keyfile triggers
  the backend's monitor to reload and emit `changed` on GTK's own settings instances, causing
  a `GtkSettings` restyle. A41 covers an existing file only. If it does, the first flush on a
  fresh profile moves after the first input-quiet period. The flush is already off the
  interaction path; the restyle is the cost.
- **S-trans.** A 200 ms transition that starts at composition ends with a relayout of the
  window. The first task identifies the transition from frame-phase spans and a `GTK_DEBUG`
  trace. If its relayout of a dense document can exceed `B_settle`, it is avoided: for example,
  a revealer that finishes hidden is set invisible in the same frame.

### D8. The verdict

The verdict is measured the same way as C4c's verdict. It runs under the lock protocol
(rev 2: the measurement runs as `flock -x INTENT flock -x LOCK`, builds as
`flock -s INTENT true && flock -s LOCK`) and the quiet gate. The reference
precondition is recorded, never assumed.

- **Interleaved:** baseline, branch, baseline, branch, each a release `responsiveness-probes`
  build measured by the branch's lane driver. The **baseline** is `c6-baseline`: `main`
  (with C1 merged) plus only the instrumentation commit of task 1.3, which adds spans, marks,
  the sentinel and frame phases and changes no behaviour (review R24). Both sides therefore
  get the same spans, the same attribution and the same driver-computed settle.
- **Profiles:** the four construction profiles (fresh/steady × keyfile/dconf) for
  `baseline/small`; the other cases in the steady keyfile profile.
- **Repetitions:** 7 per side, 4 + 3 across the two rounds.
- **Cases:**
  - `baseline/small`, `baseline/empty`;
  - `total-size` 16 kB, 1 MB and 5 MB as `.txt` and `.html`;
  - `dense` 120, 500 and 710, wrap on and off;
  - new: `markdown/preview` (a 200 kB Markdown document with side-by-side preview);
  - new: `editorconfig/tab-width` (a `.editorconfig` whose `tab_width` differs from the
    setting);
  - new: `session/restore-3-small` (three 16 kB documents);
  - new: `recovery/small-drafts` (two 64 kB file-backed drafts).
- **Phases:** `open`; `background` until `document-settled`; `type-during-settle`; `scroll`.
  On the `dense` cases there are also three new phases: `wrap-toggle`, `zoom-step`
  (`win.zoom-in`) and `font-change` (the custom-font key). Each runs until `document-settled`,
  and its interaction dispatch and every later dispatch are recorded.
  `type-during-settle` is a new phase. From load readiness it injects one key every 50 ms
  into the document, `SETTLE_TYPING_KEYS` keys in all. That count is a named driver constant,
  large enough to span the 5 MB case's install and attach. The phase then stops typing and waits
  for settle (review R26). It records each key's dispatch latency (compositor event time to
  handler, from the probe's input records). Debounces started by the typed edits are not
  post-open work. The `session/restore-3-small` case selects each restored tab in turn and
  waits for settle on each (R8).
- **Met when**, on the branch, in every profile and repetition:
  - every dispatch from the activation turn until settled is at most `B_SETTLE_US`;
  - every `type-during-settle` key is dispatched within `B_INTERACT_US`;
  - every `wrap-toggle`, `zoom-step` and `font-change` interaction dispatch is within
    `B_INTERACT_US`, and every dispatch after it, until settled, is within `B_SETTLE_US`;
  - no post-open stage regresses: per case, the median over the 7 repetitions of each
    attributed stage's worst dispatch, and the median time from activation to settled, are each
    at most the baseline's plus one reference frame (R27).

  Each over-budget dispatch on the baseline is listed with its source. The only dispatches
  exempted are toolkit-floor exceptions: D2's language parse, D6's dense resize margin (S7), or
  D9's restyle, if their probes fail. Each is exempted from the `B_SETTLE_US` condition. The
  input events whose dispatch overlaps it are exempted from the `B_INTERACT_US` condition (R7).
  Both are reported on the exception's own line, with the axiom id, the events' latencies and
  the upstream draft.

- **Input-to-frame is reported, not gated** (coordinator decision 2026-10-30). The lane's
  floor is documented in the report and in `docs/next/long-line-documents.md`:
  - headless Mutter with the cairo renderer double-buffers at 120 Hz;
  - an input that lands just after a frame starts waits one frame for the next frame and one
    more to be presented, 16.7–19.3 ms;
  - a full-screen paint adds 5–7 ms on an idle machine, so every scroll measures 20–25 ms on
    any document, whatever its size or post-open state (`a3`: 20.1–25.0 ms).

  The 2-frame limit stays with `long-line-evolution.md` candidate 5 (a presentation-capable
  lane). The report prints each case's input-to-frame next to that floor.

### D9. User-started invalidations on dense documents are paced (coordinator decision 2026-10-30)

GtkTextView invalidates **every** line when the view's default text style changes:
- a default-style property (`wrap-mode`, `tabs`, margins: A37);
- a CSS change of the view's font. Zoom (`ui/window/zoom.rs`) and the font preference both
  restyle `.monospace`.

On a dense document every 2000 px validation batch then costs up to 21.6 ms (dense 710
wrap-off), and the whole document is revalidated batch by batch. C6 bounds this with the same
rule as D6: **never more than `V_SLICE` visible characters of invalid lines at a time**.

**Which documents.** A document is *dense* when some window of `R_BATCH` consecutive
paragraphs holds more than `V_SLICE` visible characters. A pure predicate decides it,
`batch_exceeds_slice`, over C1's `ProfileSummary` (`worst_batch_chars > V_SLICE`). The summary is
computed on the load or replacement worker and kept current by C1's profile watch. Every other
document keeps today's direct property changes, which are within budget.

**Every editor, selected first (review R22).** Zoom and the font preference restyle
`.monospace` in every editor at once. The selected editor is paced first. Each other dense
editor keeps the new style as a pending restyle and runs it when it is next mapped. It does
not re-tag while hidden.

**Two candidate mechanisms.** Axiom probe **S6** decides between them before task 7.

- **A, a tag-carried document style (preferred for zoom, font and tab width).** A dense
  document's text carries one lowest-priority `GtkTextTag`. It holds the properties that would
  otherwise be view-wide: `tabs`, `family` and the zoom as a relative `scale`. The tag is
  applied as each install slice is inserted, so it adds no invalidation, and tag changes do not
  dirty the buffer (A27).

  A buffer tag styles every view of the buffer, including the minimap's `GtkSourceMap` and the
  print compositor (review R19):
  - zoom is a relative `scale`, not an absolute `size`, so the map's 1-point font is scaled
    rather than replaced;
  - the tag carries no `wrap-mode`, because the map must not wrap. **A word-wrap toggle on a
    dense document uses B**, or B's recorded exception;
  - the print path strips the document tags or prints from its own buffer, whichever S6 shows
    to be correct.

  Consumers that measure the view font must read the tag's metrics instead (R20):
  - Focus Mode's `readable_column_margin`;
  - GtkSourceView's right-margin column and its `tab-width` to `tabs` conversion;
  - the line-number gutter.

  The view of a dense document is taken out of the `.monospace` zoom and font CSS: its own
  font is fixed, so a restyle invalidates nothing. A user change then re-tags the document
  range by range:
  - two tags alternate, old and new;
  - each dispatch moves at most `V_SLICE` visible characters of paragraph-aligned text from the
    old tag to the new one;
  - the visible paragraphs go first, then outward.

  Each re-tag invalidates only its range. That range's validation runs in the next frame's
  layout, because `frame_validation.rs` already queues the allocation after tag changes (A37).
  Text inserted at a tagged run's edge is untagged (A25), so the insert handler applies the
  current tag to it.

  S6 must show:
  - the tag's `tabs`, `family` and `scale` lay out exactly like the view properties;
  - re-tagging a range invalidates only that range;
  - a view outside the zoom CSS keeps its gutter and line numbers usable;
  - the minimap's slider and markers stay usable under the scaled tag;
  - printing is unchanged;
  - Focus Mode margins and the right margin are right when computed from the tag's metrics.

  The gutter is the known risk: GtkSourceView's line-number renderer uses the view's font.
- **B, hide and reveal (fallback).** In the interaction dispatch, the paragraphs outside the
  visible window are hidden with an `invisible` tag. Hidden text adds no layout (A24). Then the
  view property or CSS changes, and the paragraphs are revealed outward in `V_SLICE` ranges.
  S6 must show that hiding and revealing keep the scroll anchor and the insert mark in place.
  It must also show what find and accessibility see during the reveal: A33 means
  `visible-only` search skips hidden text, and A35 covers the AT-SPI text of hidden ranges.

  S6 must also show what the minimap shows during a reveal, because the `invisible` tag
  reaches it too.

  B is used for word wrap, and for zoom, font and tab width only if A fails S6.

**The interaction contract.**
- The dispatch that handles the user's action takes at most `B_interact`. It records the
  change and schedules the first range.
- The first frame showing the change on the visible paragraphs fits `B_settle`.
- Every later re-tag or reveal dispatch fits `B_settle` and runs at `G_PRIORITY_LOW`, so input
  goes first.
- A second change while a re-tag is running supersedes it. The plan restarts from the visible
  window with the newest values, as a generation-guarded continuation (`rust.md`'s
  coordination vocabulary).

The plan is a pure, allocation-free step function in the load workflow's `policy.rs`
(review R21). C1's `ProfileSummary` has no per-paragraph lengths, so the function reads them
through a callback:

```text
next_restyle_range(cursor, visible, v_slice, paragraph_chars) -> Option<(cursor, range)>
```

It walks outward from the visible paragraphs, alternately below and then above them. The GTK
adapter supplies `paragraph_chars` from the buffer's line offsets. It has Kani harnesses in the
same shard as D6's, over a six-paragraph model with symbolic lengths:
- exact coverage;
- visible paragraphs first, then the stated alternation;
- each range within `V_SLICE` except a single over-budget paragraph (C2's);
- no range split inside a paragraph;
- termination.

GtkTextView does not expose its invalid region. The widget tests therefore assert LushText's
own evidence, the characters re-tagged or revealed per dispatch, together with dispatch
durations (R23).

If neither A nor B passes S6, the user-started invalidation on dense documents is recorded as
a toolkit-floor exception against S6, with numbers and a drafted upstream report on
GtkTextView's batch size. This follows the same pattern as D2.

## Risks / Trade-offs

- **[S1 fails: the definition parse cannot leave the GTK thread]** → D2 moves the parse to a
  `G_PRIORITY_LOW` idle after the first frame and after `INPUT_QUIET` with no input. It is
  still about 37 ms for HTML once per language per process, recorded as a toolkit-floor
  exception against S1, with a drafted upstream report. The activation turn and every other
  stage still meet the budget.
- **[The thread-confined language catalog violates a GtkSourceView assumption]** → S1 probes
  the exact handoff, under a CRITICAL-failing lane (`run-gtk-axioms.sh`). The catalog is built
  on a worker from application startup, so a fallback to the default manager costs one
  dispatch (the 3.9 ms language-list load), never correctness.
- **[`S_BG_HIGHLIGHT` reduces highlighting on large HTML]** → user-visible. The coordinator
  decided it (budgets are never relaxed), and the maintainer may override. The reduction is
  shown by C1's existing notice, and the upstream draft may make it unnecessary.
- **[D9's tag-carried style takes dense views out of the zoom and font CSS]** → the gutter's
  line numbers use the view font. S6 checks that they stay usable. If they do not,
  hide-and-reveal (B) is used, and if that fails too, a toolkit-floor exception is recorded.
  The predicate `batch_exceeds_slice` keeps every other document on the plain property path.
- **[A user changes wrap, zoom or font again while a re-tag is running]** → the newest change
  supersedes, and the plan restarts from the visible window. Generation guards keep a stale
  range from applying old values.
- **[Visible-character slicing makes dense installs take more turns]** → each slice is bounded
  and the editor stays non-editable during install, as today. A dense 113 kB document takes
  about 4 slices of about 3 ms instead of one 1.8 ms turn followed by 21.6 ms and 14 ms of
  validation.
- **[`document-settled` reads quiet, not completion]** → `SETTLE_QUIET` is longer than a batch
  interval, and the lane caps the wait and records a cap hit as a failure, so a false
  "settled" can only shorten the measured window. The verdict additionally reads every
  dispatch until the phase cap.
- **[The overscroll changes on dense documents]** → only dense documents change, and S7
  chooses the mechanism (review R16). The expected fallback keeps `bottom-margin`, set once before
  the first slice. The visual-geometry lane, the existing overscroll widget tests and the
  `native-minimap-*-anchors` invariants must show identical bottom space.
- **[Deferring the language attach shows plain text longer]** → until the attach stage and the
  first highlight idle, the document is unhighlighted, and bracket matching does not skip
  strings and comments. `ATTACH_DEADLINE` bounds the wait under input (R6, H1).
- **[`S_SYNTAX` derived on HTML turns highlighting off for cheaper languages]** → open question 2.

## Migration Plan

There are no data, settings or format changes. Implementation order:

1. attribution;
2. axioms;
3. the language catalog;
4. language attach;
5. highlight attach;
6. the slice plan;
7. overscroll;
8. the EditorConfig join;
9. the background gate;
10. paced user-started invalidations (D9, after S6);
11. lane;
12. verdict.

Each step is a separate commit, and the lane is re-run after steps 4, 6, 9 and 10. Rollback is
per commit. The language catalog has a fallback to `LanguageManager::default()` behind one
function.

## Resolved questions (coordinator, 2026-10-30; the maintainer may override)

1. **Background highlighting of large HTML (D4).** Budgets are never relaxed. Above the
   measured `S_BG_HIGHLIGHT`, highlighting is reduced through `DocumentScale` with the reason
   `TotalSize`. The GtkSourceView upstream report is drafted in `.claude/handoffs/` for the
   maintainer to file.
2. **If the language parse cannot leave the GTK thread (D2).** Attach is deferred to a
   `G_PRIORITY_LOW` idle after the first frame and `INPUT_QUIET`. The parse is recorded as a
   toolkit-floor exception against S1, with measured numbers and a drafted upstream report,
   following the A46 precedent.
3. **Input-to-frame (D8).** Reported, not gated. The double-buffered floor is documented.
4. **User-started invalidations on dense documents (D9).** C6 owns them through the same
   bounded validation rule, with a scenario each for word wrap, zoom and font.
5. **C1 merge order.** If C1 is unmerged when C6 is implemented, the derived `S_SYNTAX` value
   lands on C1's branch, and C1's harnesses are re-run there. C1 already has the total-size
   input and `ReductionReason::TotalSize` (review R10).

## Open Questions

1. Whether S6 selects mechanism A or B for zoom, font and tab width (word wrap is always B,
   review R19), and whether either passes, is decided by the probe in task 2.6. S7 likewise
   decides the dense overscroll mechanism. Neither outcome needs a new decision, because each
   fallback chain ends in a recorded toolkit-floor exception.
2. **For the maintainer: how low may `S_SYNTAX` go, and may it depend on the language?**
   (review R12). One context-engine slice already exceeds `B_settle`, so the bisected value is
   where HTML first needs a second slice. That may be well below 1 MiB: 16 kB needed one slice,
   1 MB needed four. A single value derived on HTML also turns highlighting off for languages
   that analyse in budget at that size. The options are:
   - (a) one value, the minimum over the measured languages. This is the most conservative
     choice and needs no change to C1's model;
   - (b) a per-language threshold. This adds a language input to `DocumentScale` and
     re-states C1's monotonicity harnesses over it;
   - (c) a floor below which highlighting is never reduced, with the background slices above
     the measured value recorded as a toolkit-floor exception against S3 until the upstream
     report lands. This contradicts "budgets are never relaxed" and needs an explicit override.

   Until the maintainer answers, (a) is implemented, and the bisection is recorded per
   language, so that (b) or (c) can be chosen without re-measuring.

## Adversarial review (2026-10-30)

A review against the code on `document-scale-policy` (6af792a3), the archived C4b and C4c
designs and the drafted upstream reports tried to refute each decision. The decisions above
are already revised; this section records why. **R** items changed the design, **H** items
held.

### Language attach (D2, D3)

- **R1. Every C1 attach site must go through the attach stage.** On C1 the language is set
  from four places, not one: `reapply_language` from `set_file_path_for_pending_load`,
  `apply_document_scale_gates` at publish, the buffer-replacement terminal
  (`buffer_replacement/execution.rs`, "a cancelled install … the path's language comes back")
  and the scale-change path in `editor_page/document_scale.rs`. Moving only the first leaves the
  36–40 ms parse in the publish dispatch. D2 now names all four and routes them through one
  `request_language_attach`, which records the wanted id and schedules the stage.
- **R2. The Markdown preview reads the attached language.** `ui/window/preview.rs::is_markdown`
  calls `document_language_id`, which returns the buffer's language, or the path's guess only
  while the scale holds syntax off. While an attach is pending, syntax is on and the buffer
  has no language, so a Markdown document would show "Not a Markdown file" until the attach,
  and the `markdown/preview` case would measure a wrong preview. D2 now makes
  `document_language_id` return the resolved id while an attach is pending.
- **R3. A catalog built after the first frame is never ready for the activation's opens.**
  C4c runs the activation turn in the dispatch right after the first frame (B1), so a catalog
  started then is still on its worker, and every activation open falls back to
  `LanguageManager::default()`. That costs the 3.9 ms the change set out to remove and loads a
  second copy of the language list. The catalog now starts at application `startup`, on a
  worker, so it runs during window construction. If it is still not ready when the attach stage
  runs, the attach waits for it; it is never ready later than the stage needs it. C4c's v9 experiment (warming
  `guess_language` inside W) moved the cost into W. A worker moves it off the GTK thread.
- **R4. A copied glob table is a second language-guessing algorithm.** `guess_language` also
  orders overlapping globs and handles case. A LushText re-implementation over a copied table
  can choose differently (`*.h`, `*.in`, `CMakeLists.txt`). D2 now requires a unit test that
  compares the worker's guess with `guess_language` for every glob of every shipped language,
  plus a set of tricky names. Where they differ, the GTK thread's `guess_language` wins at
  attach time.
- **R5. "Shared through the language object" and "thread-confined" cannot both hold for a
  language the GTK thread already uses.** The parsed context data lives on the
  `GtkSourceLanguage`. If a worker parses through a language object the GTK thread can reach,
  both threads touch it. D2 now allows the worker parse only on a language object from a
  **private manager created on that worker** that no GTK-thread code can reach yet. The worker
  then hands over the pair of language object and throwaway buffer, and from then on the
  catalog maps that id to the handed-over object. S1 must also show that a language from a
  second manager attaches and highlights exactly as one from the catalog's own manager, and that
  the throwaway buffer never touches the default style-scheme manager on the worker.
- **R6. The attach deferral had no ceiling, and its event set was undefined.** "No input event
  on the window" includes pointer motion and kinetic scroll, which arrive every frame for as
  long as the pointer moves. D2 now counts only key, button, scroll-start and touch-begin
  events, and adds a ceiling, `ATTACH_DEADLINE`. Once the ceiling has passed, the stage runs at
  the next `G_PRIORITY_LOW` dispatch whatever the input.
- **R7. The verdict could not be met in the S1-fail case.** At 50 ms between keys,
  `INPUT_QUIET` (two reference frames) passes between keys, and the next key waits behind the
  36–40 ms parse. D8 exempted the exception dispatch only from `B_SETTLE_US`. Every
  `type-during-settle` HTML run would therefore fail on that key. D8 and the measurement spec now
  exempt from `B_INTERACT_US` **the input events whose dispatch overlaps the exempted dispatch**,
  reported on the exception's line with their latency. No other key is exempt.
- **R8. Unmapped editors have no first frame.** Session restore creates pages that are not
  selected, and "after the document's first editor frame" never comes for them. D2 now defers an
  unmapped editor's attach until it is first mapped (selection). The `session/restore-3-small`
  case selects each restored tab in turn and waits for `document-settled` each time.
- **R9. Order with D3.** D3's preferred order (engine attached through a sliced install)
  contradicts D2 (attach after the first editor frame) when the first frame comes before the
  install ends. The parse cost depends on the language, not on the buffer, so the order is
  now: the attach stage runs after the first editor frame and between install slices if the
  install is still running. Slices after it run with the engine attached, with highlighting as
  `highlight_attach_plan` decides.
- **H1. Typing before the attach.** No undo record is involved: `set_language` and highlight
  tags never create an undo action or dirty the buffer (A27). Indentation is not
  language-dependent in LushText: `editor-page.blp` sets only `auto-indent`, and no indenter is
  installed. GtkSourceView 5's default indenter copies the previous line's indentation.
  Text typed before the attach is analysed with the rest at the first highlight. One visible
  effect remains and is now stated in D2: until the context engine has analysed the
  surroundings, bracket matching can pair a bracket inside a string or comment, for at most
  the frames before the first highlight idle. The plain-to-coloured change is not "at most one
  frame": it lasts until the attach stage and the first highlight idle.

### Background highlighting (D4)

- **R10. C1 already has the total-size gate.** `DocumentScale::for_profile` reduces syntax and
  bracket matching above `S_SYNTAX` (1 MiB) with `ReductionReason::TotalSize`, and its doc
  comment records `S_SYNTAX` as **not derived**: a conservative candidate. D4's "new component
  and new reason" therefore already exist. D4 is now the **derivation of `S_SYNTAX`**,
  replacing C1's candidate with the bisected `S_BG_HIGHLIGHT` value. No new `DocumentScale`
  input is added, and C1's Kani harnesses need no restatement, only a re-run.
- **R11. The breakdown was measured without C1.** On C1, the 5 MB `.html` case already has
  syntax off, so S-hlon (70.6 ms) and the 5 MB S-hlbg rows do not occur there. Task 1.4 now
  re-measures on C1's tip (or on `main` after C1 lands), and D3 matters only up to `S_SYNTAX` and
  for small dense documents that D6 newly slices.
- **R12. One context-engine slice is already over `B_settle`.** Any document whose analysis
  needs a second worker slice puts one dispatch of about 40 ms on the main loop. The
  bisected `S_BG_HIGHLIGHT` can therefore come out far below 1 MiB for HTML: the 16 kB document
  needed one slice, the 1 MB one needed four. The threshold is also language-dependent, and C1's
  `DocumentScale` is not. A single value derived on HTML would turn highlighting off for
  Rust or JSON files that analyse within budget. This is now open question 2, for the
  maintainer.

### Install pacing and validation (D5, D6)

- **R13. Slicing by total characters slices documents that do not need it.** The validation
  cost after a direct install is per 2000 px batch (A31). It grows with the characters in one
  batch, not with the document. A 100 kB document of 40-character lines has 100 k characters
  but cheap batches. D6.1 now routes to the sliced path on **C1's batch density**:
  `batch_exceeds_slice(summary)` is `summary.worst_batch_chars > V_SLICE`, over the
  `ProfileSummary` the load worker already computes. It is the same predicate D9 uses. The byte
  threshold of D5 still applies to everything.
- **R14. Slicing only bounds a validation batch if validation runs between slices.**
  GtkTextView validates in idles at `GTK_TEXT_VIEW_PRIORITY_VALIDATE`. If the next install slice
  is dispatched at a higher priority, several slices of invalid lines accumulate, and the next
  batch is as large as before. The slice continuation of a dense install now runs after the
  frame that lays out the previous slice: the next slice is queued from the frame clock's
  `after-paint`. S4 checks that each validation dispatch then holds one slice.
- **R15. The Kani harness cannot range over text.** C1's paragraph scan has its own
  per-character harnesses and its agreement test with `pango_find_paragraph_boundary`. The
  two-budget harness now checks the slice-end decision over an abstract sequence of
  `(bytes, chars)` paragraph lengths (six paragraphs, symbolic). The text scanner is
  pinned separately by C1's existing proofs. `restyle_ranges` (D9) gets the same split.
- **R16. The end-of-document overscroll tag fails on most files.** Several of its premises
  do not hold:
  - A file that ends with a newline has an empty last paragraph. A tag applied to an empty
    range is a no-op, so `pixels-below-lines` cannot be put on that paragraph, and on the
    paragraph before it the space would appear above the last line.
  - `pixels-below-lines` is part of the paragraph's line range. Whatever uses the last line's
    y-range would see it 75 % of a viewport tall: GtkSourceView's current-line highlight
    (`highlight-current-line` is a LushText setting), bookmark gutter marks, minimap markers,
    and `scroll_to_iter` margins.
  - The minimap mirrors the view's `bottom-margin` (`minimap/projection_execution.rs::sync_source_map_geometry`).
    A buffer tag reaches the map by itself, so keeping the mirror would apply the space twice.
    The `native-minimap-*-anchors` visual invariants are keyed on that geometry.
  - The print compositor renders the buffer's tags and would print the space.
  - Moving the tag from the idle leaves, for one frame after an Enter at the end, the space
    between the old last line and the new one.

  D6.2 now limits the overscroll change to **dense documents** (`batch_exceeds_slice`). Every
  other document keeps `bottom-margin`, which invalidates within budget, as D9 already argues
  for its own properties. For dense documents a new axiom, **S7**, decides between the tag and
  the fallback: a margin set once before the first install slice from the view's allocated
  height, whose later resize changes are deferred to a `G_PRIORITY_LOW` dispatch and recorded
  against S7 if they exceed `B_settle`. The tag is used only if S7 shows the empty last line,
  current-line highlight, gutter, minimap and print are all unaffected.
- **R17. "Apply EditorConfig to the empty buffer" is wrong for a reload.** An external-change
  reload or an eviction reload clears old text first. Applying `tab-width` before the clear
  invalidates the old text. The formatting is now applied **after the clear completes and before
  the first insert slice**. Two more points:
  - EditorConfig is re-resolved after the install by Save As (`window/dialogs.rs`), rename and
    the `use-editorconfig` toggle (`on_use_editorconfig_changed`). These are post-install
    `tab-width` changes, and on dense documents they go through D9.
  - Draft recovery and local-history restore install through whole-buffer replacement, not
    load. They get the same join where an EditorConfig resolution is in flight.
- **R18. A late tab width moves the restored position.** Session restore applies cursor and
  scroll after publish (`restore_position.rs`). A late EditorConfig `tab-width`, now paced on
  dense documents, changes line heights under it. The restore position is re-applied once the
  paced update reaches the restored line, and `LoadEvidence` records it.
- **H2. Joining EditorConfig into planning does not slow restore admission.** Today the
  resolution is a separate `spawn_blocking_then` per open, which competes for the 8 shared
  slots. Joined, it runs inside the planning worker's slot. Session restore's limit of two
  planning operations bounds it.

### Paced user-started invalidations (D9)

- **R19. A buffer tag styles every view of the buffer.** `GtkTextTag` has no per-view scope.
  The minimap's `GtkSourceMap` and the print compositor render the same buffer:
  - a tag `size` overrides the map's 1-point font and draws the minimap at document size;
  - a tag `wrap-mode` overrides the map's `WrapMode::None`;
  - printing would use the zoomed size.

  Mechanism A now carries zoom as a relative `scale`, not an absolute `size`. The map then
  scales by the same factor, and S6 must show that its slider and markers stay usable. A also
  gives the map a higher-priority tag that resets `wrap-mode`. A tag cannot be scoped to one
  view, so that tag would apply everywhere. **A therefore cannot carry `wrap-mode`.** A word-wrap
  toggle on a dense document uses B, or the recorded exception, whatever S6 says about A for
  zoom and font. The print path builds its own compositor buffer (to be confirmed by S6), or
  removes the document tags for printing.
- **R20. Character-width consumers read the view font.** With A, the view's own font stays
  fixed while the text is zoomed by the tag. Every consumer that measures the view font would
  then be off by the zoom factor:
  - Focus Mode's readable-column margins (`focus_mode::policy::readable_column_margin`, from a
    Pango character width);
  - GtkSourceView's right-margin column and its `tab-width` to `tabs` conversion;
  - the line-number gutter.

  S6 now checks each of these. Under A, LushText passes the tag's metrics to Focus Mode's policy
  instead of the view's.
- **R21. `restyle_ranges(profile, …)` cannot be computed from the profile.** C1's
  `ProfileSummary` holds three numbers (total bytes, longest paragraph, worst batch), not
  per-paragraph lengths. A function that returns all the ranges also allocates. The plan is
  now a step function:

  ```text
  next_restyle_range(cursor: RestyleCursor, visible: ParagraphSpan, v_slice: u64,
                     paragraph_chars: impl Fn(u32) -> u64) -> Option<(RestyleCursor, ParagraphSpan)>
  ```

  It is allocation-free and walks outward, alternately below and then above the visible span,
  so the order is specified. The GTK adapter supplies `paragraph_chars` from the buffer's line
  offsets, `O(log n)` per line. Kani checks it over a six-paragraph model with symbolic
  lengths: exact coverage, visible span first, alternation, the `V_SLICE` bound except a
  single over-budget paragraph, and termination.
- **R22. Zoom and font apply to every open editor.** `.monospace` restyles every editor and the
  sidebar tree at once. D9 now orders the work: the selected editor first, then each other
  dense editor when it is next mapped. An unmapped dense editor holds its new style as a pending
  restyle, and does not re-tag while hidden.
- **R23. Invalid lines are not observable.** GtkTextView does not expose its invalid region,
  so task 7.1's "no validation dispatch has more than `V_SLICE` characters of invalid lines"
  cannot be asserted directly. The widget tests now assert LushText's own evidence, the
  characters re-tagged or revealed per dispatch, plus the dispatch durations. The lane measures
  the durations.
- **H3. C1's syntax gate, invisibles and search tags.** GtkSourceView's style-scheme tags,
  the search-match tag and the space drawer all draw over the document tag, which has the
  lowest priority. They do not set the properties it carries. One exception: a style scheme can
  set `scale` on headings, and that multiplies the document tag's `scale`. The result matches
  the view-wide zoom.
- **H4. A37.** Re-tags are tag changes, and `frame_validation.rs` already queues the allocation
  before the paint for them. Neither mechanism adds a new way to invalidate inside a frame.
- **H5. The Markdown preview** has its own buffer and is not touched by D9. Tag changes emit
  no `changed`, so they do not trigger a preview refresh either.

### Measurement (D1, D8)

- **R24. `main` cannot be measured as written.** `main` has no `post_open_*` spans, no
  `highlight-updated` or `upper` marks and no `document-settled` predicate. Its background phase
  could not end at settle, and its dispatches could not be attributed. The baseline is now
  **`main` plus the instrumentation commit of task 1.3 only**, built as `c6-baseline`. That
  commit has spans, marks and frame phases, and no behaviour change. The driver computes settle
  from the marks, the same way on both sides. The `document-settled` predicate serves
  automation and widget tests, and its clauses match the driver's.
- **R25. `upper` quiet is a weak proxy for validation.** On a document whose lines are all
  the same height, the estimated heights are already right. Validation then runs without moving
  `upper`. Settle now also requires a **low-priority sentinel**: a `G_PRIORITY_LOW` idle that
  dispatched after the last mark. GTK's validation idles and GtkSourceView's update idles
  have higher priorities, so the sentinel can only dispatch when none was ready. `SETTLE_QUIET`
  must exceed the gap S3 measures between context-engine slices, if the engine reschedules
  with a timeout rather than an idle.
- **R26. Typing keeps the document from settling.** Each key restarts the draft autosave, preview
  and minimap debounces, so "no post-open debounce pending" would not hold while keys arrive, and
  the phase would hit its cap. `type-during-settle` now types for a fixed number of keys
  (`SETTLE_TYPING_KEYS`, enough to cover the 5 MB case's install and attach). It then stops and
  waits for settle. Debounces started by the typed edits do not count as post-open work.
- **R27. "No stage slower than `main`" had no statistic.** Each case now compares the median of
  the 7 repetitions of each attributed stage's worst dispatch. It also compares the median time
  from activation to settle. Each must be no more than `main`'s plus one reference frame. The
  absolute gates are unchanged.
