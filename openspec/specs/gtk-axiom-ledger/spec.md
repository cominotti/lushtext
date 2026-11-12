# gtk-axiom-ledger Specification

## Purpose
Record the GTK, GtkListBase, GtkScrollable, and Adwaita behaviours that LushText and GTK Lush geometry designs depend on in one normative ledger, pin each axiom with an isolated headless probe, and derive verification envelopes from it.
## Requirements
### Requirement: GTK behavioural axioms are recorded in one normative ledger
The project SHALL keep one ledger of the GTK, GtkListBase, GtkScrollable, and
Adwaita behaviours that LushText and GTK Lush geometry designs depend on. Each
entry SHALL have a stable id (A1, A2, …), a precise statement, the designs
that depend on it, its pinning status, the GTK and Libadwaita versions it was
last verified against, and its `gtk-lush-axioms` sample or the reason it has
none. The ledger SHALL live with the gtk4-libadwaita-internals references, and
the programme record SHALL link to it. A policy check SHALL fail when the
ledger's ids and the `gtk-lush-axioms` catalogue, probes and samples disagree.

#### Scenario: A design relies on a new GTK behaviour
- **WHEN** a geometry change starts depending on a GTK behaviour not yet in the ledger
- **THEN** the same change adds a ledger entry for it and a `gtk-lush-axioms` catalogue entry, with a probe and a sample or a recorded reason it cannot be isolated

#### Scenario: The ledger and the catalogue drift
- **WHEN** a ledger id has no catalogue entry, a catalogue entry has no ledger id, a probed entry lacks its sample, or a "Pinned by" cell names a probe that no longer exists
- **THEN** `make check-policy` fails naming the id

#### Scenario: A verified-against version is recorded
- **WHEN** a ledger entry's verified toolkit versions are updated
- **THEN** the versions are copied from the probe's printed observation, with where it ran, not from an assumption

### Requirement: Every axiom is pinned by an isolated headless probe or declared unpinnable
Each axiom SHALL be pinned by a headless probe in the `gtk-lush-axioms` crate
that asserts the behaviour in isolation against real GTK, not only indirectly
through a consumer test. An axiom that cannot be pinned SHALL record why.
LushText consumer tests that depend on an axiom SHALL remain, as evidence that
LushText stays inside the conditions the probe isolates. A probe failure after
a GTK update MUST be treated as an axiom change and reviewed against every
dependent design.

#### Scenario: Isolated probes run outside LushText
- **WHEN** `make gtk-axioms` runs
- **THEN** axioms A1, A4, A5, A9, A11, and A13 each have an isolated `gtk-lush-axioms` probe, and none runs from LushText's widget-test binary
- **AND** A8 records the phase-0 instrumented evidence, and every other unprobed axiom records why it has no probe

#### Scenario: A GTK update changes a pinned behaviour
- **WHEN** a probe fails after a toolkit update
- **THEN** the ledger entry and every dependent design and verification envelope are revisited before the failure is resolved

### Requirement: Verification envelopes derive from the ledger
A verification model that treats GTK as nondeterministic SHALL constrain it
only by ledger axioms, and SHALL cite each axiom id it uses. Using an
envelope narrower than the ledger's pinned statement MUST be recorded as an
explicit assumption.

#### Scenario: The slice-bin loop model cites its envelope
- **WHEN** the closed-loop harness restricts the child's reaction to an allocation
- **THEN** each restriction names the ledger axiom that justifies it

### Requirement: Adwaita breakpoint and split-view behaviours are ledger axioms
Every Adwaita behaviour that the breakpoint-loop envelope relies on SHALL be a
ledger entry with a new stable id. Existing ids SHALL NOT be reused. At
minimum the ledger SHALL cover:

- **Condition units.** How an `AdwBreakpoint` `max-width` condition in `sp`
  is evaluated against the window width, including any dependence on the text
  scale factor.
- **Re-evaluation timing.** When a changed condition, set with
  `set_condition`, is re-evaluated: synchronously, or at the next allocation.
- **Setter semantics.** When setters are applied during allocation, and what
  value an unapplied breakpoint restores to a property the application wrote
  while it was applied.
- **Split-view allocation.** How `AdwOverlaySplitView` allocates its sidebar
  from `sidebar-width-fraction` and its minimum and maximum widths, and
  whether `show-sidebar` or `collapsed` changes alter the toplevel's allocated
  width.
- **Minimum width.** How the set of breakpoints affects the window's minimum
  width.

Each entry SHALL be pinned by an isolated headless probe in the
`gtk-lush-axioms` crate, a minimal Adwaita fixture that uses no LushText or GTK
Lush widget, with a `gtk-lush-axioms` catalogue entry and sample. If an entry
cannot be pinned, it SHALL record why. Each entry's statement SHALL be written from what its probe
observes, not from belief.

#### Scenario: Breakpoint axioms gain probes
- **WHEN** this change completes
- **THEN** every axiom cited by the breakpoint-loop model has a ledger row with an isolated probe or a recorded reason it is not pinned
- **AND** each probe passes against the platform floor recorded in the ledger

#### Scenario: The breakpoint model cites its envelope
- **WHEN** the breakpoint-loop harness restricts Adwaita's behaviour
- **THEN** each `kani::assume` clause names the ledger id that justifies it, and any narrower assumption is recorded in the model and the programme record

### Requirement: Long-line text behaviours are ledger axioms
Every GtkTextView, GtkTextBuffer, Pango, input-method, accessibility and GtkSourceView behaviour that the long-line designs rely on SHALL be a ledger entry with a new stable id, starting at A23. Existing ids SHALL NOT be reused. The ledger SHALL cover at least the following behaviours:

| Id | Behaviour |
|---|---|
| A23 | One PangoLayout per paragraph, over its visible text, with no partial layout |
| A24 | Invisible chunks are left out of the layout text |
| A25 | Toggle gravity: text inserted at a tagged run's start or end boundary is untagged, while text inserted strictly inside the run is tagged |
| A26 | Log attrs are computed over the whole logical paragraph, hidden text included, and are invalidated by any character change |
| A27 | Tag apply and remove emit no `changed`, leave `modified` unchanged, and record no undo history |
| A28 | Pango's glyph-width int32 overflow |
| A29 | The GtkSourceView context engine analyses a line atomically, stops at a 2000 ms cut-off, then disables highlighting, and reads hidden text |
| A30 | Input-method spot and surrounding-text work grows with the cursor's paragraph and is armed by caret moves, keys, clicks and scroll |
| A31 | Validation runs in 2000 px batches |
| A32 | `gtk_source_map_set_view(NULL)` with a pending update tick causes a CRITICAL (a defect axiom) |
| A33 | `GtkSourceSearchContext` and invisible text: with `SearchSettings:visible-only` (default true) both paths skip hidden text; with it off, the non-regex path sees hidden text and the regex path still sees visible text only |
| A34 | `move-cursor` with an invisible cursor moves only the viewport, unless `gtk-keynav-use-caret` is set |
| A35 | AT-SPI text contents and offsets over invisible text, and `TextChanged` payloads on insert |
| A36 | The btree copies both strings when it merges adjacent character segments |
| A37 | A line invalidation made inside a frame (or just before one that input started) is validated inside `gtk_text_view_paint` unless the view is allocated in that frame's layout, so the scroll adjustments change after the scrollbars were allocated |

Each entry's statement SHALL be written from what its probe or measurement observes, not from the source reading that first suggested it. Each entry SHALL list the long-line changes that depend on it.

#### Scenario: A long-line design cites a toolkit behaviour
- **WHEN** the document-scale, protected-view or sliced-install change relies on one of these behaviours
- **THEN** it cites the ledger id, and the ledger row lists that change among its dependent designs

#### Scenario: An entry cannot be pinned structurally
- **WHEN** an axiom has no structural observation in an isolated fixture, as with the AT-SPI behaviour that needs an accessibility bus or the cost-only segment merge
- **THEN** its row records why it cannot be pinned, and names the responsiveness-lane evidence that stands in for a probe

#### Scenario: A defect axiom stops holding
- **WHEN** a toolkit update fixes the behaviour that a defect axiom such as A32 records, and its probe reports violated
- **THEN** the change is reviewed as an axiom change against every dependent design before the probe is edited

### Requirement: GSettings backend behaviours the shell depends on are ledger axioms
The ledger SHALL record, with the next free ids, the GIO GSettings backend
behaviours that the window-construction and settings-writer designs depend
on: that a keyfile-backend write rewrites and fsyncs the settings file
synchronously on the calling thread once per write; that `delay()` and
`apply()` perform one backend write for many pending keys; whether a write on
one thread's settings instance blocks reads and writes on another thread's
instance sharing the default backend for the duration of the fsync; and when
`changed` is emitted on a second instance relative to the backend write. Each
entry SHALL be pinned by an isolated `gtk-lush-axioms` probe with a sample
that has an interactive and a `--check` mode, running against a private
keyfile directory, and the settings writer's flush path SHALL be chosen from
the recorded outcomes.

#### Scenario: Backend axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** each GSettings backend axiom's probe asserts its recorded behaviour in isolation and `make check-gtk-axioms` finds the ledger, catalogue, probes, and samples in agreement

#### Scenario: A GLib update changes backend behaviour
- **WHEN** a probe for a GSettings backend axiom fails after a GLib update
- **THEN** the change is reviewed against the settings writer's flush path before the update is accepted

### Requirement: Toplevel presentation, visibility, and late-composition behaviours are ledger axioms
The ledger SHALL record, with the next free ids (A46–A50 at the time of writing; A42–A45 belong to
`document-scale-policy`), the GTK and Libadwaita behaviours that the staged window composition
depends on:
- that the first realize of a toplevel in a process carries a one-time cost that later toplevels
  do not pay, recording the measured magnitudes of both;
- that `gtk_window_show` validates CSS only for visible nodes and realizes the surface, and that
  the toplevel's size computation and first measure run in the first frame's layout phase, not
  inside `present`;
- that a subtree made visible or parented in an already mapped window is styled and measured in
  the next frame's layout phase, not at the call;
- that a concealed vertical-slide `GtkRevealer` measures a visible child horizontally at full
  scale and returns zero for an invisible child;
- that `AdwBreakpointBin` applies a breakpoint matching on its first allocation after `map` before
  allocating its child, while a breakpoint that starts matching on a later allocation, including
  one added after `map`, swaps through a one-frame snapshot of the old content.

Each entry SHALL be pinned by an isolated `gtk-lush-axioms` probe with a sample that has an
interactive and a `--check` mode. The composition design SHALL cite the ids.

#### Scenario: Presentation axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** each presentation axiom's probe asserts its recorded behaviour in isolation, and `make check-gtk-axioms` finds the ledger, catalogue, probes, and samples in agreement

#### Scenario: A toolkit update removes the one-time realize cost
- **WHEN** the first-realize probe reports that a first and a later toplevel realize cost the same after a GTK update
- **THEN** the change is reviewed against the composition stages before the probe is edited, and the realize stage is reconsidered

#### Scenario: A toolkit update changes the first-allocation breakpoint path
- **WHEN** the breakpoint probe reports that a breakpoint matching on the first allocation after map no longer applies before the child is allocated
- **THEN** the composition's first-frame geometry guarantee and the breakpoint-loop model are reviewed before the probe is edited

### Requirement: The realize frame and the frame clock's pacing after an unpresented frame are a ledger axiom
The ledger SHALL record, with the next free id (A51 at the time of writing; A42–A45 belong to
`document-scale-policy` and A46–A50 to `first-frame-window-shell`), that:
- realizing a toplevel schedules a frame on its frame clock whose paint idle runs ahead of
  `G_PRIORITY_DEFAULT_IDLE` sources;
- when the surface is not yet mapped, that frame runs its layout phase and emits `after-paint`
  but never receives presentation timings;
- the frame clock then starts the next frame no earlier than one `FRAME_INTERVAL` (16,667 µs)
  after that frame, because without presentation timings its refresh interval falls back to
  `FRAME_INTERVAL`;
- realizing and presenting in one dispatch runs no frame on the unmapped surface.

The entry SHALL be pinned by an isolated `gtk-lush-axioms` probe, with a sample that has an
interactive and a `--check` mode, recording each `after-paint`'s mapped state, frame counter,
frame time, and presentation time. The window-composition design SHALL cite the id.

#### Scenario: The realize-frame axiom is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the probe observes an unmapped `after-paint` before map when realize and present run in separate default-idle dispatches, and none when they run in one dispatch
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe, and sample in agreement

#### Scenario: A toolkit update stops running the realize frame
- **WHEN** the probe reports no unmapped `after-paint` with realize and present in separate dispatches after a GTK update
- **THEN** the composition's merged realize-and-present dispatch is reviewed before the probe is edited, and the ledger entry is revised

### Requirement: Protected View's toolkit assumptions are ledger axioms
Every GTK or GtkSourceView behaviour that Protected View relies on SHALL be a ledger axiom, unless C0 or C1 already pins it. Each axiom SHALL have an isolated headless probe and a `gtk-lush-axioms` sample, except where a probe cannot observe it in-crate; that axiom SHALL be pinned by the named lane case instead, and its ledger row SHALL say so, as C0 does for A35. The axioms are numbered after the last applied ledger entry (provisionally P1–P8, which `sliced-protected-install` cites; applied as A59–A66). The entries SHALL cover:
- P1: a capture-phase gesture that claims a press sequence on a GtkTextView prevents GtkTextView's click gesture from moving the insert mark, resetting the IM context, pasting the primary selection on middle click, opening its default context menu, or starting a drag; an unclaimed touchscreen drag scrolls the view without moving a mark;
- P2: a non-editable GtkTextView never calls `gtk_im_context_focus_in`, and its key controller carries no IM context;
- P3: a handled and stopped `move-cursor` emission with `cursor-visible=false` moves only the viewport, including under `gtk-keynav-use-caret`, extending C0's A34;
- P4: for a paragraph whose tail is invisible, the AT-SPI text contents and offsets; the payload and cost of text queries at character, word, sentence and line granularity; the `TextChanged` payload when the hidden tail is inserted; and the cost of an AT-initiated caret or selection write into it, extending C0's A35. Like A35 it is pinned by the lane's AT case, not by an in-crate probe;
- P5: disabling the text view's DnD source and target prevents drop insertion on a non-editable view;
- P6: GtkTextView recomputes the per-instance enablement of its `clipboard.*` and `selection.*` actions, so it is not a guard; and a key binding pre-empted before its default handler never reaches `select-all`, `toggle-cursor-visible`, `menu.popup`, `change-number`, `change-case`, `join-lines` or `move-to-matching-bracket`;
- P7: `insert_with_tags` of an invisible run never lays out the run, and its cost grows linearly with the run (recorded per MiB, the input to C3's gate);
- P8: the cost of reading one fixed-size chunk of a single-segment paragraph at increasing depths, positioned by character offset (`iter_at_mark` and `forward_chars`) and by line and byte index; cost evidence like A36, recorded with the probe's figures.

A probe that fails SHALL block the dependent Protected View task until the design adds the missing interception or the view is made non-focusable.

#### Scenario: Axiom gate before relying on an assumption
- **WHEN** `make gtk-axioms` runs on the toolkit versions in the ledger
- **THEN** every Protected View axiom probe passes, or the ledger records the axiom as falsified and the corresponding Protected View task stays open

#### Scenario: Lane-pinned axiom
- **WHEN** the ledger lists P4
- **THEN** its row names the lane AT case that pins it and states that no in-crate probe exists

#### Scenario: The snapshot-depth cost is recorded before the source design relies on it
- **WHEN** the P8 probe reads chunks at increasing depths of a two-million-character single-line buffer
- **THEN** the ledger records the per-chunk time for both positionings, and the edit-growth source capture uses a positioning whose recorded time does not grow with depth, or is declared an envelope

### Requirement: The post-open toolkit costs are ledger axioms
The ledger SHALL record seven axioms, provisionally S1–S7, with the next free ids at the time they are applied. A42–A45 belong to `document-scale-policy`, A46–A51 are applied, and `long-line-protected-view` reserves P1–P8.

- **S1.** A GtkSourceView language whose definition embeds others (HTML) parses its definitions when it is first attached to a buffer, and the cost of that parse, recorded per language. Whether the parsed definitions are shared through the language object across buffers. Whether a private language manager and a buffer created, attached and used on a worker thread, and then handed to the GTK thread and used only there, attach without any GDK, style-manager, style-scheme-manager or main-context access on the worker. Whether a language object from that second manager attaches and highlights on the GTK thread exactly as one from the application's own manager.
- **S2.** The cost of `set_highlight_syntax(true)` against buffer size, with a language attached. The cost of attaching a language to an empty buffer followed by sliced inserts, and the highlight idles those inserts schedule.
- **S3.** The context engine's background analysis runs in dispatches of a fixed slice that the application cannot configure, with the slice constant cited from the GtkSourceView source of the verified version. The priority of those dispatches, and whether the engine reschedules through an idle or a timeout, together with the gap between slices.
- **S4.** GtkTextView's validation dispatch after an insert lays out only the inserted, invalid lines, and its cost grows with their visible characters, in either wrap mode. When the next insert waits for the frame that lays out the previous one, each validation dispatch holds one insert's lines. This extends A31 to inserted lines.
- **S5.** Whether the first write of a keyfile-backend settings file that does not exist yet makes the backend's file monitor reload it and emit `changed` on other instances, including GTK's own settings. This extends A41 to a fresh profile.
- **S6.** Whether the style carried by a lowest-priority tag, through `tabs`, `family` and a relative `scale`, lays out like the view-wide properties. Whether re-tagging a range invalidates only that range. Whether a view outside the `.monospace` zoom and font CSS keeps its gutter usable. What a second view of the same buffer (a `GtkSourceMap`) and the print compositor show under that tag, and whether character-width consumers (right margin, readable-column margins) are right from the tag's metrics. And, for the fallback, whether hiding the paragraphs outside the visible window and revealing them in ranges keeps the scroll anchor and insert mark, and what search, AT-SPI and the minimap see meanwhile.
- **S7.** Whether a `pixels-below-lines` tag can carry the end-of-document overscroll: on an empty last paragraph, for the current-line highlight, the gutter, a `GtkSourceMap` and printing. And the cost of changing `bottom-margin` on a dense document, in both wrap modes.

Each entry SHALL be pinned by an isolated `gtk-lush-axioms` probe, with a sample that has an interactive and a `--check` mode. A probe that records costs SHALL record magnitudes, never a verdict. The post-open settle design SHALL cite the ids.

#### Scenario: The post-open axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the S1–S7 probes run under the private headless session, and any CRITICAL or WARNING they provoke fails the lane unless the probe captures it on purpose
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probes and samples in agreement

#### Scenario: The language parse cannot leave the GTK thread
- **WHEN** the S1 probe shows that the worker handoff touches GDK, the style manager or a main context, or that the parsed definitions are not shared through the language object
- **THEN** the language attach stays a GTK dispatch, deferred to a low-priority idle after input-quiet, and the ledger entry records it as a toolkit-floor exception with its measured cost and the drafted upstream report

#### Scenario: The dense overscroll cannot be a tag
- **WHEN** the S7 probe shows that the tag fails on an empty last paragraph or changes what another consumer of the last line's geometry shows
- **THEN** a dense document's overscroll is a margin set before the install, and a later resize-driven change over `B_SETTLE_US` is recorded as a toolkit-floor exception against S7

#### Scenario: Neither restyle mechanism passes
- **WHEN** the S6 probe shows that neither the tag-carried style nor hide-and-reveal meets its conditions
- **THEN** the user-started invalidation on dense documents is recorded as a toolkit-floor exception against S6, with its measured batch cost and a drafted upstream report

### Requirement: Sliced protected installation behaviours are ledger axioms
Every GTK text-buffer behaviour that the sliced protected install relies on SHALL be a ledger entry with a new stable id, allocated after the ids taken by the long-line probe and protected-view changes. Existing ids SHALL NOT be reused. At minimum the ledger SHALL cover:

- **Segment separation** (A76). Two character segments separated by a zero-byte left-gravity mark segment are not merged by the line cleanup. Text inserted at a left-gravity mark's iterator is placed after the mark, while a right-gravity mark moves after it; text inserted at a left-gravity mark at the start of a tagged run lands before the run, untagged.
- **Hidden append re-validation** (A77). Re-laying out a paragraph after text carrying an invisible tag is appended to it costs time bounded by the paragraph's visible text and its segment count, not by its hidden bytes; with no visible text in the paragraph yet, it costs microseconds.
- **A point below the layout** (A79). Mapping a point below a text view's layout to an iterator costs time proportional to the hidden bytes of the buffer's last line (the visible-index walk steps one byte at the end iterator), and is flat when a delimiter follows the hidden run.
- **Accessible change payloads** (A78), extending the existing accessibility-payload axiom. With the accessibility bridge on and a listener attached, the text-change event for an insertion or deletion is built from the inserted or deleted range only, so its size and cost are bounded by that run and do not depend on the text that remains in the paragraph. The insert event carries the inserted text as it was before the tag is applied; the delete event of a hidden range carries no hidden characters.

The existing segment-merge axiom, that merging adjacent character segments copies both strings, remains the axiom these entries mitigate. The segment-separation and hidden-append entries SHALL each be pinned by an isolated headless probe in the `gtk-lush-axioms` crate, using a minimal GtkTextBuffer or GtkTextView fixture that uses no LushText or GTK Lush widget, with a catalogue entry and a sample. Because segment structure is not public, the segment-separation probe SHALL observe mark gravity structurally and the absent merge by per-append cost, and its ledger row SHALL say so. The accessible-payload entry SHALL be pinned the way the existing accessibility-payload axiom is, by the responsiveness lane's accessibility case, because the probe runner starts no AT-SPI bus, and its ledger row SHALL say so. Each entry's statement SHALL be written from what its probe or lane case observes. A failing probe SHALL block adoption of the sliced protected install, or switch it to the recorded fallback.

#### Scenario: The segment-separation probe runs
- **WHEN** `make gtk-axioms` runs on the pinned toolkit
- **THEN** the segment-separation probe inserts tagged runs at the end of a paragraph across left-gravity marks, observes that each run lands after the previous mark, and records a per-append time that does not grow with the installed tail, while the same appends without marks show a growing per-append time

#### Scenario: A toolkit update breaks a sliced-install axiom
- **WHEN** a sliced-install axiom probe fails after a GTK update
- **THEN** the sliced protected install and its fallback are re-reviewed against the ledger before the failure is resolved

### Requirement: Text validation's hold on lower main-loop priorities is a ledger axiom
The ledger SHALL record A80. While a `GtkTextView` has lines left to validate,
its incremental validation idle is ready again after every batch. So a
`G_PRIORITY_DEFAULT_IDLE` source does not dispatch until the whole buffer is
validated, whether the view is on screen or on the hidden page of a
`GtkStack`. A source at `GDK_PRIORITY_REDRAW + 4` dispatches before the first
incremental batch completes. A80 SHALL be pinned by a `gtk-lush-axioms` probe,
with a sample that has an interactive and a `--check` mode, and with a control
showing that the lower-priority source dispatches at once when nothing is left
to validate. The probe SHALL record the iterations and microseconds each source
waited as magnitudes, never as a verdict.

#### Scenario: A80 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A80 probe runs under the private headless session, in both the
  presented and the hidden-page fixture
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GTK stops holding lower priorities back
- **WHEN** a GTK update makes a `G_PRIORITY_DEFAULT_IDLE` source dispatch
  before validation completes, or makes validation outrank priority 124
- **THEN** the probe fails
- **AND** the completion priority of `gtk-lush-tasks` and LushText's admission
  hand-offs are revisited before the probe is changed

### Requirement: Equal-priority co-dispatch is a ledger axiom
The ledger SHALL record A81: GLib dispatches every ready source of the top
priority in one main-loop iteration, so that iteration lasts the sum of their
callbacks, while a ready source one step lower, or one of the same priority
added during the dispatch, runs in a later iteration. Designs that rely on
equal-priority co-dispatch SHALL cite A81. A81 SHALL be pinned by a
`gtk-lush-axioms` probe and a sample with an interactive and a `--check` mode.

#### Scenario: A81 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A81 probe arms three busy idles at one priority, one a step
  lower and one added from a callback, and records which ran in each iteration
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

### Requirement: A hidden tab page's text view lays out its buffer
The ledger SHALL record A82: an `AdwTabView` page never selected is neither
mapped nor realized, yet a `GtkSourceView` on it has a text layout and validates
text loaded into its buffer in full while hidden, as does a page selected once
and hidden again; only a buffer no view shows is not laid out; and the first
page appended to an empty tab view is selected by the tab view. A82 SHALL be
pinned by a probe and a sample.

#### Scenario: GTK stops laying out never-shown pages
- **WHEN** a GTK or Libadwaita update makes a never-selected page's view skip
  validation
- **THEN** the A82 probe fails
- **AND** the placeholder detach of `defer-background-tab-layout` is revisited
  before the probe is changed

### Requirement: An empty buffer stops a hidden view's layout
The ledger SHALL record A83: `gtk_text_view_set_buffer` to an empty buffer stops
a hidden view's validation at once and leaves the document buffer's text,
modified flag, undo state and caret unchanged; setting the document back lays
it out again from nothing and brings the view back at the buffer start; a
scroll queued right after the reattach lands at another line once the lines
above get heights, while scrolling to the target again on every `upper` change
keeps the top near it and one more scroll once laid out lands
on it; an unmapped validating view notifies its vertical adjustment's `upper`.
A83 SHALL be pinned by a probe and a sample; its accessibility consequence
(`set_buffer`'s whole-content text change) is recorded from source.

#### Scenario: A83 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A83 probe detaches a hidden validating view, reattaches it hidden
  and shown, and records the iterations and top lines

### Requirement: The context engine's reported coverage is a ledger axiom
The ledger SHALL record A84: after an insert at the end of a GtkSourceView
buffer whose language is attached, or an attach to a buffer already holding
text, the largest `highlight-updated` end reported since the change reaches
the end of the last line's text once the context engine has analysed the whole
buffer, a `G_PRIORITY_LOW + 1` idle queued after the change always finds it
there, and a chunk the `G_PRIORITY_LOW` worker still analyses is not reported
at the end by a `G_PRIORITY_DEFAULT_IDLE` idle. Designs that pace inserts by
the engine's progress SHALL cite A84. A84 SHALL be pinned by a
`gtk-lush-axioms` probe and a sample with an interactive and a `--check` mode;
the sample SHALL also measure the paced chunked install per language.

#### Scenario: A84 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A84 probe installs short-line HTML chunk by chunk into an
  attached buffer, with `\n`, CRLF and unterminated text, attaches a language
  to a buffer holding one chunk, and checks the reported coverage at a
  `G_PRIORITY_LOW + 1` idle after each change
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GtkSourceView changes what highlight-updated reports
- **WHEN** a GtkSourceView update reports less than the analysed range, or
  reports it after a `G_PRIORITY_LOW + 1` idle
- **THEN** the A84 probe fails
- **AND** the chunked highlighted install of `chunked-highlight-attach` is
  revisited before the probe is changed

### Requirement: An axiom sample never opens its window for help or an unknown argument
Every `gtk-lush-axioms` sample SHALL print its usage and exit `0` on `--help`
or `-h`, and print its usage and exit `2` on any argument it does not know or
on a measurement-mode option given without its mode, before GTK is
initialized and without creating a window. A sample with measurement modes
SHALL declare them and SHALL check its command line first. `make
check-gtk-axioms` SHALL fail a sample that reads its own flags without that
check.

#### Scenario: Help on a live desktop
- **WHEN** a sample is started with `--help` on a desktop session
- **THEN** it prints its usage and exits 0, and no window appears

#### Scenario: A mistyped mode
- **WHEN** a sample is started with an argument it does not know
- **THEN** it prints its usage and exits 2, and no window appears

### Requirement: An edit's re-analysis and its pacing are a ledger axiom
The ledger SHALL record A85: after an edit in a GtkSourceView buffer whose
language is attached, an idle queued at `G_PRIORITY_HIGH_IDLE` before the edit
dispatches immediately before the context engine's first update, whose
`highlight-updated` comes from a `G_PRIORITY_HIGH_IDLE` source; an idle queued
at `G_PRIORITY_LOW - 1` after the edit dispatches after it and before the first
worker slice; `gtk_source_buffer_ensure_highlight` on an empty range at a
bound, from an idle at `G_PRIORITY_HIGH_IDLE - 1`, re-analyses the edit to the
line after the bound, applies no style tag (only the context-class tags every
analysis maintains), and reports the range in one `highlight-updated`, whose
end is short of the bound exactly when the analysis merged, after which the
first update and the worker find nothing; and removing the language from such
an idle removes the pending first update and worker and changes no text.
Designs that pace an edit's re-analysis SHALL cite A85. A85 SHALL be pinned by
a `gtk-lush-axioms` probe and a sample with an interactive and a `--check`
mode; the sample SHALL also measure ordinary and context-flipping keys on any
document, plain and paced.

#### Scenario: A85 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A85 probe types an ordinary key into a small C document and
  `/*` at the start of 512 KiB of C with no closer, checks the idle order
  around the first update, that a pending worker is removed with the language
  from a `G_PRIORITY_LOW - 1` idle, that paced steps leave the first update
  and the worker nothing, that each step makes exactly one `highlight-updated`
  and no style-tag change (against a control in which `ensure_highlight` over
  the whole document does tag it), and that removing the language between steps
  removes both
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GtkSourceView changes how an edit is re-analysed
- **WHEN** a GtkSourceView update makes `ensure_highlight` apply style tags
  for an empty range, report more than one range per call, or leave the first
  update work after a merged step
- **THEN** the A85 probe fails
- **AND** the paced re-analysis of `bound-context-flip-keystrokes` is
  revisited before the probe is changed

### Requirement: Provider reloads and class swaps are a ledger axiom
The ledger SHALL record A86: after `gtk_css_provider_load_from_string` on a
provider added for the display, even with unchanged text, the next frame
revalidates the CSS of every widget, and its cost grows with the number of
widgets, while a load whose rules match no widget's classes changes no
computed font and re-lays out no text; after a CSS class is swapped on a set of widgets whose font rules are
keyed by that class, those widgets compute the new font size, and the next
frame's cost does not grow with widgets outside the set. Designs that change
the editor font or zoom SHALL cite A86. A86 SHALL be pinned by a
`gtk-lush-axioms` probe and a sample with an interactive and a `--check` mode.

#### Scenario: A86 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A86 probe measures a reload with unchanged text and a class
  swap on a source view and `.monospace` labels, each in a window without and
  with 1,500 unrelated rows, checks that the reload's frame grows with the rows
  and the class swap's does not, that the view's and labels' Pango font size
  follows the class, and that loading rules keyed by classes no widget carries
  leaves the view's Pango font size and its layout unchanged
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GTK changes how a class swap restyles
- **WHEN** a GTK update makes a class swap on a widget revalidate widgets
  outside its subtree, or a provider reload stop revalidating every widget
- **THEN** the A86 probe fails
- **AND** the zoom classes of `scope-font-zoom-restyle` are revisited before
  the probe is changed

### Requirement: An axiom whose verdict differs between supported toolkits is version-qualified
When a ledger axiom holds on one supported toolkit version and not on another, the ledger entry SHALL state the behaviour for each supported version range, with the version bound and the upstream change that introduced the difference. The supported versions are the GNOME source floor and the runtime the Flatpak ships. The entry's probe and sample SHALL read the running toolkit's version, assert the statement for that range, and print which range they asserted. They SHALL fail if the behaviour on either range changes again. "Verified against" SHALL record a run on each supported range and where each run happened. An entry SHALL be restated only after the toolkit change is attributed and every dependent design and citing verification envelope is reviewed; a probe SHALL be edited only after its entry is restated. A defect axiom fixed upstream SHALL be restated the same way while any supported toolkit still has the defect.

#### Scenario: GTK 4.24 changes the list's anchor behaviour
- **WHEN** `make gtk-axioms` runs on GTK 4.24 and the A7 and A20 probes observe that a host `set_value` no longer leaves a stray anchor
- **THEN** A7 and A20 state the GTK < 4.24 behaviour and the measured GTK ≥ 4.24 behaviour, attributed to the GTK change that introduced it
- **AND** the probes hold on both the GNOME 50 floor and GTK 4.24, asserting the range they run on

#### Scenario: A defect axiom is fixed upstream
- **WHEN** the A32 probe runs on GtkSourceView 5.22 or later, where detaching a map with a pending tick logs no CRITICAL
- **THEN** A32 states that the defect is present below 5.22.0 and fixed from 5.22.0 (`8d679ccd`)
- **AND** on 5.22 the probe holds only if no CRITICAL is logged, so a return of the defect fails it

#### Scenario: Restating before reviewing
- **WHEN** a probe fails after a toolkit update and its dependent designs or citing envelopes have not yet been reviewed
- **THEN** neither the entry nor the probe is edited

### Requirement: The ledger is verified on the shipped toolkit and on the floor
Every ledger row SHALL record in "Verified against" a run on the toolkit the shipped Flatpak runtime carries. Every row with a version-qualified statement SHALL also record a run on the GNOME source floor. After a runtime change, no row SHALL cite only the previous runtime's toolkit.

#### Scenario: Moving the Flatpak to GNOME 51
- **WHEN** the Flatpak runtime moves from GNOME 50 to GNOME 51
- **THEN** every probed row's "Verified against" names GTK 4.24.x / Adw 1.10.x, copied from a `make gtk-axioms` observation with the place it ran
- **AND** the version-qualified rows also name the GNOME 50 floor run

### Requirement: Nightly re-test results are recorded without restating an axiom
When a ledger axiom is re-tested against a GNOME nightly SDK, its "Verified against" cell SHALL gain a note naming the following, each copied from the run:
- the nightly SDK build commit and date;
- the toolkit versions the sample printed, with the GTK, Pango and GtkSourceView source commits from the SDK's manifest;
- where the sample ran;
- the verdict and the recorded values that the re-test was about.

The note SHALL be labelled as a preview build. It SHALL NOT be counted as a run on a supported toolkit. An axiom that changes verdict only on a nightly SDK SHALL NOT be restated or version-qualified until the change reaches a toolkit the project supports and is attributed. The ledger or the programme record SHALL list such an axiom as an unattributed nightly preview, owned by the next GNOME adoption change. A ledger cell that names a probe SHALL name the probe that the catalogue entry with the same id registers, and `make check-gtk-axioms` SHALL reject a "Pinned by" cell that names another axiom's probe.

#### Scenario: A defect still reproduces on the nightly
- **WHEN** the A79 sample holds on the nightly SDK that contains GTK !10651
- **THEN** A79's "Verified against" gains a nightly note with the build commit, GTK `4.25.x` and its source commit, and the 1, 2 and 4 MiB lookup times
- **AND** the axiom statement is unchanged

#### Scenario: An axiom fails only on the nightly
- **WHEN** the A85 sample fails on the nightly SDK and holds on GNOME 51 and on the GNOME 50 floor
- **THEN** neither A85's statement nor its probe is changed
- **AND** A85 is listed as an unattributed nightly preview for the next GNOME adoption, with the failed check and the build commit

#### Scenario: A ledger cell names another axiom's probe
- **WHEN** the A67 row's "Pinned by" cell names `gtk_lush_axioms::probe_a52`
- **THEN** `make check-gtk-axioms` reports that A67's cell names another axiom's probe
- **AND** the cell is corrected to `probe_a67`, which the A67 catalogue entry registers, as are the A68–A73 cells that name `probe_a53` … `probe_a58`

