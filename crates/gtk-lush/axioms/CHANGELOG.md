# Changelog

## 0.0.0

- First functional in-tree pre-publication implementation: the axiom
  catalogue (`AxiomId`, `Axiom`, `catalogue()`, `find()`), the `Observation`
  / `Verdict` result with a dependency-free one-line JSON encoder, the shared
  pure-GTK fixtures (`fixtures::FixedHost`, `HostedList`, the per-axiom
  fixtures, and the values each probe drives them with under
  `fixtures::aNN`), and probes for A1, A2, A3, A4, A5, A6, A7, A9, A11, and A13.
- The A5, A9, A11, and A13 probes moved here from LushText's widget-test
  binary with their steps unchanged; their measured values are identical on
  GTK 4.22.5 / Libadwaita 1.9.3. The fixture GType is renamed
  `GtkLushAxiomsFixedHost`, and the probes present in a plain `AdwWindow`
  with no application.
- One sample per probe under `examples/`, interactive by default and a
  headless verdict with `--check` (exit 0 / 1 / 2, and 77 outside a private
  headless session).
- The probe runner is the `axiom_probes` test of `gtk-lush-adoption-lab`, run
  by `make gtk-axioms`, because a family crate may not depend on
  `gtk-lush-proof-harness`.
- Probes and samples for the Adwaita behaviours the adaptive-shell breakpoint
  loop relies on (`extend-closed-loop-geometry-verification`), pure Libadwaita
  fixtures only: A14 (`max-width: N sp` matches `px ≤ N × text scale`,
  inclusive), A15 (`set_condition` applies a frame later, at an allocation it
  schedules itself), A16 (setters change after the bin allocated its child, and
  an unapply restores the value from `add_setter` time), A17
  (`AdwOverlaySplitView` sidebar width is a clamped fraction in sp; toggling it
  never changes the window width), and A18 (a breakpoint makes the window's
  minimum its `width-request`; only the last-added matching breakpoint
  applies). New shared fixtures: `fixtures::BreakpointFixture` (an
  `AdwBreakpointBin` allocated at an exact width), `fixtures::SplitViewFixture`,
  `fixtures::set_text_scale`, and `FixedHost::set_child_width`; the values each
  probe drives them with are under `fixtures::a14` … `fixtures::a18`.
- A10 rewritten from measurement and given a probe and sample: a
  `GtkListView` maps rows wholly outside its own allocation (one past the
  bottom at rest, one past each edge at a row-aligned value, after
  `scroll_to`, and after a focus scroll), so `mapped` does not mean drawn.
  Its catalogue name is now `a10_mapped_rows_are_not_necessarily_on_screen`.
- A19 (`GtkViewport` emits `notify::page-size` / `notify::value` only after
  allocating its child, while a clamp's `value-changed` precedes it, so a
  `queue_allocate` from the notify is not served in that frame) and A20 (a
  host `set_value` leaves the list's scroll anchor outside `[0, 1]`; the
  value re-announced after allocation anchors it at the view's edge), with
  probes and samples. New fixtures: `fixtures::AllocationProbe` and
  `fixtures::ViewportOrder` (with `ViewportEvent`, `QueuedFromNotify`),
  `fixtures::a20::reannounce_value`, and the values under `fixtures::a10`,
  `fixtures::a19`, and `fixtures::a20`.
- `fixtures::RowPlacement` and `fixtures::row_placement` classify a mapped
  row against its list's allocation, shared by the A10 and A20 probes and the
  A10 sample; `APPLIED_LABEL` is one constant re-exported by `fixtures::a14`,
  `a15`, and `a16`.
- A21 (`gtk_window_destroy` on a window of a registered `GtkApplication`
  emits `window-removed` before it returns and drops the window from
  `windows()`, but disposes it only when the last strong reference drops,
  once, just before finalization), with a probe and sample. New fixtures:
  `fixtures::DisposeCountingWindow` and `fixtures::DisposalLog`.
- A22 (`TreeListRow::set_expanded(true)` in a `GtkTreeListModel` with
  `passthrough` and `autoexpand` off runs the create function before it
  returns, so the expanded state, `children()`, and the flattened item count
  are final on return; a `None` leaves the row collapsed and marks it empty;
  later appends to the returned store appear at `parent + 1 + index`; and
  `set_expanded(false)` removes the children synchronously; `is_expandable()`
  on a row not yet probed also runs the create function and discards the
  model), the first tree-model axiom, with a probe that needs no window and a
  sample. New fixture: `fixtures::LazyTree`, with its values under
  `fixtures::a22`.
- The long-line text axioms (`probe-long-line-toolkit-costs`), the first
  text-layout, text-buffer, input-method and GtkSourceView axioms, with probes
  and samples for A23 (a paragraph is one `PangoLayout`, never partly laid
  out), A24 (invisible text is left out of the layout), A25 (insertions at a
  tagged run's start or end are untagged, inside it tagged), A26 (log attrs
  cover the whole paragraph, hidden text included; invalidation recorded as
  cost), A27 (tag changes emit no `changed`, leave `modified` unset, record
  no undo), A28 (Pango's `int32` line-width overflow), A29 (the context
  engine's one-call 2000 ms cut-off disables highlighting; it reads hidden
  text), A30 (an input method's surrounding text is the cursor's whole
  paragraph), A31 (2000 px validation batches), A32 (the `GtkSourceMap`
  detach CRITICAL, a defect axiom), A33 (`GtkSourceSearchContext` skips hidden
  text unless `visible-only` is off, and its regex path always does), and A34
  (`move-cursor` with an invisible cursor only scrolls, unless caret
  browsing). A35 (AT-SPI text over invisible text) and A36 (btree segment
  merge cost) are catalogued without probes; the responsiveness lane records
  them. Verdicts stay structural; every duration is a recorded value.
- A23 compares display-line counts, not pixel heights, against an independent
  Pango layout at the view's wrap width (the text width less GtkTextView's
  one-pixel cursor space). At the full text width the CI container's font
  wrapped one line more in the view than in Pango, and the view's summed
  per-line pixel heights can differ from Pango's once-rounded height by a
  pixel when the lines agree.
- The crate now depends on the toolkit crates `pango` and `sourceview5`. New
  text fixtures: `fixtures::paragraph` with `ParagraphKind` (deterministic
  ASCII, multibyte and minified-markup paragraphs), `TextViewFixture`,
  `SourceViewFixture`, `invisible_tag`, `LogCapture` (keeps warnings and
  criticals while alive, then restores the previous log handling), the probe
  input method `ProbeImContext` registered as `PROBE_IM_ID` by
  `register_probe_im` and read back with `take_im_requests`,
  `text_toolkit_versions` (text probes record the Pango and GtkSourceView
  versions), `time_us` and `offset`; each probe's values and helpers are
  under `fixtures::a23` … `fixtures::a34`.
- Probe and sample for A37 (a line invalidation made inside a frame is
  validated inside `gtk_text_view_paint` unless the view is allocated in that
  frame's layout, so the scroll adjustments change after the scrollbars were
  allocated), with `fixtures::a37::{LongLineView, PaintPhaseChanges,
  FrameChange}`: the paint phase is bracketed by a one-pixel `GtkDrawingArea`
  snapshotted before the view and the frame clock's `after-paint`. Added for
  LushText's fix of the scrollbar `GtkGizmo` snapshot warning.
- A37 also covers default-style properties: `FrameChange::Margin` grows the
  view's left margin per frame (`MARGIN_STEP`) and redraws the view, and
  `LongLineView::queue_allocation_before_paint` queues the allocation on the
  `notify` of every `DEFAULT_STYLE_PROPERTIES` property (their setters
  invalidate every line and queue no resize, allocation or redraw). Added for
  LushText's fix of the warning's residual route (the overscroll bottom
  margin).
- Probes and samples for A38–A41, the first GIO GSettings axioms (a keyfile
  write replaces the file on the calling thread once per `set_*`;
  `delay()`/`apply()` writes many keys once; a second keyfile backend over one
  file stays stale until its monitor reloads; `changed` timing across
  instances). New fixtures: `KeyfileSettingsFixture` (a keyfile backend over a
  private file in the temp directory, removed on drop, on GTK 4's
  `org.gtk.gtk4.Settings.FileChooser` schema), `WriteWatch` (counts renames
  onto the settings file), `write_on_a_worker`, `drain_main_context`.
- A slow-probe marker: `Axiom::slow`, with `INCLUDE_SLOW_ENV`
  (`GTK_LUSH_AXIOMS_INCLUDE_SLOW`), `include_slow_probes` and
  `slow_skip_line`. The probe runner and a sample's `--check` skip a slow
  probe unless the switch is `1`, printing `skipped (slow): <id>`. No probe
  is slow yet.
- Probes and samples for A42 (`GtkTextBuffer` lines end exactly at Pango's
  paragraph delimiters: `\n`, `\r`, `\r\n`, U+2029, never U+2028 or
  U+0085), A44 (a language cleared in an `insert-text` after-handler leaves the
  inserted text untagged) and A45 (a widget unparented with a tick callback
  armed never runs it; unparenting and dropping a `GtkSourceMap` with its
  update tick armed logs no CRITICAL), and a catalogue entry for A43
  (`chars_in_line` walks segments, verified by source, no in-crate probe), for
  `document-scale-policy`. New fixtures under `fixtures::a42`, `fixtures::a44`
  and `fixtures::a45`.
- Probe and sample for A51 (`shorten-window-content-fill-in`): realizing a
  toplevel schedules a frame that runs ahead of default-idle sources; on the
  unmapped surface it emits `after-paint`, is never presented, and paces the
  next frame one `FRAME_INTERVAL` (16,667 µs) later; realizing and presenting
  in one dispatch runs no unmapped frame. New fixture: `fixtures::a51`
  (`Composed`, `Order`, `AfterPaint`, `FRAME_INTERVAL_US`,
  `PACED_GAP_FLOOR_US`).
- Probes and samples for A46–A50 (`first-frame-window-shell`): the first
  toplevel realize of a process has a one-time cost a second one does not pay
  (recorded, not judged); neither realize nor present measures, the first
  measure runs in the first frame's layout phase, and an invisible subtree is
  neither measured, allocated nor mapped; a subtree made visible or parented
  in a mapped window is measured in a later frame's layout phase; a concealed
  vertical-slide revealer measures a visible child horizontally at full width
  and an invisible one not at all; `AdwBreakpointBin` applies a breakpoint
  matching on its first allocation after map before allocating the child, and
  swaps a later match through a one-frame snapshot. New fixtures:
  `CountingWidget` (counts measures, allocations and snapshots),
  `PhaseRecorder` and `PhaseStamp` (stamp each frame-clock phase with a
  measure count), `first_phase_measuring`, `fixture_window`, `timed_realize`
  and `concealed_revealer`; their values are under `fixtures::a47`,
  `fixtures::a49` and `fixtures::a50`.
