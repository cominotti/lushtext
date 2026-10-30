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
bottleneck. They are **exploratory, not a verdict**. Run `a1` was discarded because two builds
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
  `long-line-evolution.md` candidate 5 (a presentation-capable lane), not C6 (Open Question 3).
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

**Non-Goals:**

- Pango layout of giant paragraphs, Protected View, and the recovery of giant single-line
  drafts: C2 (D13), with the residual insert going to C3.
- Window construction and composition: C4/C4b/C4c, all met.
- Invalidations the user starts on dense documents (word-wrap toggle, zoom, font, tab-width
  preference). These are interactions. Their cost is GtkTextView's 2000 px batch (A31), and
  they are recorded as a residual with a candidate owner (Open Question 4).
- The lane's input-to-frame floor (Open Question 3).
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
- no `highlight-updated` has fired for the same period.

The period is a named constant `SETTLE_QUIET` in the settle policy, 100 ms: longer than one
2000 px batch interval, shorter than the debounce windows. This is the only time-based clause,
and it says "quiet", not "complete", because GtkSourceView and GtkTextView expose no
completion signal. The lane's background phase becomes "until `document-settled`, capped".

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
   `sourceview5::LanguageManager`. A worker constructs it and loads its language list right
   after the first frame, through `spawn_blocking_then`. It is handed to the GTK thread and
   used only there afterwards. Every LushText caller of `LanguageManager::default()` uses the
   catalog instead: `reapply_language` and the Markdown preview's code blocks
   (`markdown_preview/code_blocks.rs`). This is a thread-confined handoff: the object is never used on two threads, and the
   worker drops its reference before the handoff. The path guess for a document runs on the
   load planning worker against an immutable glob table copied from the catalog. It returns a
   language **id**, not an object.
2. **The definition parse leaves the activation dispatch.** `set_language` moves out of
   `set_file_path_for_pending_load` (which reaches it through `republish_document_identity` →
   `reapply_language`) into its own stage, a `G_PRIORITY_DEFAULT_IDLE` dispatch
   after the document's first editor frame. The document shows unhighlighted for at most one
   frame. Whether the parse can then also leave the GTK thread is decided by axiom **S1**: a
   worker attaches the language to a private throwaway `GtkSourceBuffer` and holds it until
   the GTK thread attaches its own. Only if S1 shows that the parsed definitions are shared
   through the language object, and that nothing on that path touches GDK or the default style
   manager, does the parse run on the worker. The throwaway buffer is then kept for the
   process lifetime per language used, which bounds memory by the number of languages.

   If S1 fails, the parse stays a GTK dispatch of 36–40 ms for HTML. It is recorded as an
   over-budget residual with an upstream report (a parse that can be split or run ahead), and
   the maintainer decides (Open Question 2). The same precedent was used for C4b's A46 realize
   floor.

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

### D4. Background highlighting: a measured total-size gate (maintainer decision)

GtkSourceView's context engine analyses in idle slices whose length is a compile-time constant
in GtkSourceView. The measured dispatches are 40–47 ms. Axiom **S3** pins this with the slice
constant cited from the GtkSourceView source of the verified version. LushText cannot shorten
the slices.

A document whose full analysis needs more than the first slice therefore stalls for 40–47 ms
per slice. The 16 kB HTML document did not; the 1 MB one did, four times, and so did the 5 MB
one. Those stalls also come between scroll events: 47 ms, input-to-frame 52 ms.

The decision proposed for the maintainer is to measure `S_BG_HIGHLIGHT`: the largest
short-line `.html` total size whose background analysis keeps every dispatch within `B_settle`.
It is bisected on the lane in the same way as C0's thresholds. Above it:

- `DocumentScale` reduces syntax highlighting with a new reason, `TotalSize`;
- the status surface and the document properties say so, as C1's reductions do.

This needs a total-size component in C1's `DocumentScale`, as a policy input next to the
paragraph profile. If C1 is still unmerged, the edit is made on C1's branch. C1's Kani
monotonicity and meet harnesses are re-run unchanged in statement.

An upstream report asks GtkSourceView for a configurable slice or a deadline-aware worker. If
that lands, the gate is revisited.

*Alternatives:*

- Accepting 40–47 ms slices for large HTML. This breaks the budget; it needs the maintainer.
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
   validation dispatch fits half of `B_SETTLE_US` in both wrap modes. The direct path applies
   the same plan: a dense document below the byte threshold but above `V_SLICE` is sliced.

   A37 is unchanged. Every slice is an edit, so `frame_validation.rs` queues the allocation
   before the paint, and validation of the slice runs in layout, bounded by the same plan.
2. **No post-open invalidate-all from LushText.**
   - The end-of-document overscroll sets the view's `bottom-margin`, a default-style property
     that invalidates every line (A37 investigation, 2026-10-14). It is re-implemented as a
     `pixels-below-lines` tag on the last paragraph, which invalidates one line. The tag is
     kept on the last paragraph across edits by the existing overscroll idle.
     *Alternative:* setting the margin once before install. Rejected, because every later page
     resize would still revalidate the document.
   - EditorConfig resolution runs beside load planning. Today its result can arrive after the
     install and change `tab-width`, which invalidates every line. The load joins it before
     the install. The read completion waits for the resolution, which is a bounded walk over
     `.editorconfig` files done by a worker. The formatting is applied while the buffer is
     still empty. If the resolution has not returned by the time the install would start (a
     slow filesystem), the install does not wait. The formatting is applied after publish,
     and the one revalidation that causes is recorded in the load evidence.
3. **Residual.** Invalidations started by the user on a dense editable document pay
   GtkTextView's batch cost. They are not post-open settle and are recorded with a candidate
   owner (Open Question 4).

Kani: the two-budget boundary gets harnesses in `model/buffer_replacement/kani_proofs.rs`
(declared `#[cfg(kani)] mod kani_proofs;`). They go in a new shard `core-post-open-settle`,
gate `scheduled`, with its CI budget measured from two dispatched runs, plus entries in
`ORACLES`. They prove, for any paragraph sequence of up to six paragraphs with symbolic lengths
and symbolic budgets:

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
(`flock -x` for the measurement, builds `flock -s`) and the quiet gate. The reference
precondition is recorded, never assumed.

- **Interleaved:** `main`, branch, `main`, branch, each a release `responsiveness-probes`
  build measured by the branch's lane driver, so both sides get the same spans and stage
  attribution. `main` has no `post_open_*` spans, and the driver attributes its dispatches by
  the same unspanned-dispatch rule.
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
  `type-during-settle` is a new phase: from load readiness, inject one key every 50 ms into
  the document until `document-settled`, and record each key's dispatch latency (compositor
  event time to handler, from the probe's input records).
- **Met when**, on the branch, in every profile and repetition:
  - every dispatch from the activation turn until `document-settled` is at most `B_SETTLE_US`;
  - every `type-during-settle` key is dispatched within `B_INTERACT_US`;
  - no post-open stage's span is larger than on `main`.

  Each over-budget dispatch on `main` is listed with its source. Input-to-frame is reported,
  not gated (Open Question 3).

  If the maintainer chooses to accept D4's slices instead of a gate, the HTML cases above the
  threshold are reported as an accepted residual.

## Risks / Trade-offs

- **[S1 fails: the definition parse cannot leave the GTK thread]** → D2 moves the parse to its
  own dispatch after the first frame. It is still about 37 ms for HTML once per process, and is
  recorded and escalated (Open Question 2). The activation turn and every other stage still
  meet the budget.
- **[The thread-confined language catalog violates a GtkSourceView assumption]** → S1 probes
  the exact handoff, under a CRITICAL-failing lane (`run-gtk-axioms.sh`). The catalog is built
  only after the first frame, so a fallback to the default manager costs one dispatch, never
  correctness.
- **[`S_BG_HIGHLIGHT` reduces highlighting on large HTML]** → user-visible, so it is a
  maintainer decision. The reduction is shown by C1's existing notice. Upstream may make it
  unnecessary.
- **[Visible-character slicing makes dense installs take more turns]** → each slice is bounded
  and the editor stays non-editable during install, as today. A dense 113 kB document takes
  about 4 slices of about 3 ms instead of one 1.8 ms turn followed by 21.6 ms and 14 ms of
  validation.
- **[`document-settled` reads quiet, not completion]** → `SETTLE_QUIET` is longer than a batch
  interval, and the lane caps the wait and records a cap hit as a failure, so a false
  "settled" can only shorten the measured window. The verdict additionally reads every
  dispatch until the phase cap.
- **[The overscroll tag changes the overscroll mechanism]** → the visual-geometry lane and the
  existing overscroll widget tests must show identical bottom space. `pixels-below-lines` on
  the last paragraph is a tag change, which A37's `frame_validation` already queues before the
  paint.

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
9. the background gate (after the maintainer decision);
10. lane;
11. verdict.

Each step is a separate commit, and the lane is re-run after steps 4, 6 and 9. Rollback is
per commit. The language catalog has a fallback to `LanguageManager::default()` behind one
function.

## Open Questions

1. **Background highlighting of large HTML (D4).** Gate highlighting above a measured
   `S_BG_HIGHLIGHT` (proposed), or accept GtkSourceView's 40–47 ms slices as a recorded
   residual until upstream changes? A maintainer decision, before task 7.
2. **If S1 shows the definition parse cannot leave the GTK thread**, accept one ~37 ms dispatch
   per language per process (first HTML open) as a recorded residual, or defer highlighting of
   such languages until the first input-quiet second?
3. **Input-to-frame.** C0's gate kept "2 reference frames". The headless lane's
   double-buffered floor plus a full-screen paint puts every scroll at 20–25 ms on any
   document, independent of post-open work. C6 proposes to report input-to-frame and gate input
   dispatch latency instead, leaving the frame limit to a presentation-capable lane
   (`long-line-evolution.md` candidate 5). Does the maintainer agree?
4. **Dense documents after a user-started invalidation** (word wrap, zoom, font, tab width).
   These pay GtkTextView's 2000 px batch (21.6 ms at dense 710 wrap-off). Own it in C5
   (`explore-editing-long-paragraphs`), in a separate change, or accept it?
5. **C1 merge order.** D4 and the C1 harness re-run assume C1 merges first. If C6 is implemented
   before C1, the total-size component lands on C1's branch.
