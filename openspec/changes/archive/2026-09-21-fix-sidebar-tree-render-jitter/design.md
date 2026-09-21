## Context

`ViewportSliceBin` owns its child's vertical adjustment, hosts the child at full
content height, and allocates only the visible band. The child (`GtkListView`
styled `navigation-sidebar`) is a `GtkScrollable` that treats the adjustment as
its own.

| fact | provenance |
|---|---|
| `.navigation-sidebar { padding-top: 6px; padding-bottom: 4px }` | verified, `strings libadwaita-1.so.0` on the host |
| child overwrites `upper` 15248→15238, `page_size` 665→655 every allocation | measured, v0.8.1 |
| child `value` settles 1–7px from the published offset, varying between allocations with no content change | measured, v0.8.1 |
| GTK4 `gtk_widget_allocate` insets to the CSS content box before the `size_allocate` vfunc and translates the transform by the content-box origin | toolkit contract |
| the bin's `measure` returns the child's border-box natural height (`imp.rs` `measure`, `natural.max(minimum)`) | verified |
| `Adjustment::new(0,0,0,1,1,0)` in `constructed`; `publish_slice_offset` uses `configure` and reads the clamped value back | verified `imp.rs` |
| `allocating` is cleared immediately after `child.allocate`, before the post-processing | verified `imp.rs` |
| `follow_child_request` forwards `child_value − viewport_top`, not `child_value − published` | verified `scroll_request.rs` |
| `StyleContext::padding` is `deprecated = "Since 4.10"` under the `gnome_50` feature; `gtk_widget_get_css_padding` does not exist; gtk-lush forbids `unsafe` | verified gtk4-0.11.3, `check-gtk-lush-policy.py` |
| `flush_after_delay` is `sleep` + `MainContext::iteration(false)`; it does not pump the frame clock | verified `proof-harness/src/lib.rs` |
| `GtkListBase` re-derives `value` from an anchor (item + align) at allocation; its own `value-changed` handler queues an allocation on every `set_value` | GTK knowledge, gtklistbase.c 4.10–4.20; the allocation-count test measures the consequence |

## Where a row is drawn

```
bin_y = transform_y + padding_top + (row_y − value)
```

The bin's content frame already contains the child's padding (15248 = 6 + 15238
+ 4). With `value = slice_top`, `bin_y = row_y + 6` for every slice: the value
the bin publishes today is **already correct** for row alignment. What is wrong
is only `upper` and `page_size`, which the child rewrites in its content box.
And the two ceilings coincide: child `upper − page = 15238 − (slice_h − 10) =
15248 − slice_h` = the bin's `slice_top.clamp(0, height − slice_height)`, so
publishing in the child's frame changes no reachable range.

## The invariant

> **Between two allocations with the same content height, `transform_y −
> value` is the same number.**

Not `transform == value`: mid-content, when the list revises its estimate of
unrealized rows above the viewport, `row_y` moves by δ and the anchor moves
`value` by δ so the rendered row stays put. Forcing equality would trade the
top-of-list jitter for a mid-content one. At the top nothing is above the
viewport and the compensation has nothing to compensate.

## Decision

### C2 — publish `upper` and `page_size` in the child's frame

The bin derives the child's content-box inset by observation, since the style
API is deprecated and FFI is forbidden: after `child.allocate`,
`inset = slice_height − vadjustment.page_size()` when the child changed
`page_size`, else the previously derived inset (initially 0). Stored in a
`Cell<f64>`, invalidated on child replacement and `unroot`. From the next
allocation on, `publish_slice_offset` configures `upper = content_height −
inset`, `page_size = slice_height − inset`, and `value = slice_top` unchanged.
The child then agrees with every number it is given, `reconfigure_shift` is
zero on ordinary allocations, and the anchor arithmetic is evaluated against
the page the bin intended.

The learning frame itself still sees the child rewrite the page by the inset
and settle a few pixels off; the `reconfigure_shift` bound routes that settle
away from forwarding (it would otherwise travel `child − viewport_top`, ~55px
at the top: the header scrolled away on first show), and the requeued pass
republishes corrected geometry. So the bound is not a safety net: it is the
classifier for the learning frame and for any later child `upper` revision.

The bin stays the range authority every frame, so the bound keeps its meaning
(a correction the *bin's* publish provoked). Had the
bin stopped publishing `upper`/`page_size` (the retracted C1), the shift would
have become the child's own estimate revision — thousands of pixels as rows
realize — and swallowed genuine requests; that is the rejected rule already
recorded in `scroll_request.rs`.

### Residual — unconditional re-assert under the guard

If a settle survives C2 (rounding, anchor side flip), after the request
decision has been read the bin sets the value back to `published` with
`allocating` still set, so its own handler ignores the emission. Unconditional:
a conditional form gated on `reconfigure_shift > 0` is a no-op exactly when the
child did not reconfigure. The `published_offset := value` re-baseline becomes
a no-op and is removed. `allocating` moves to after the last value the bin sets.

Cost: the list's own `value-changed` handler queues an allocation on every
`set_value`, so a correction costs one extra layout when re-derivation is
idempotent and loops at frame rate when it is not. Acceptance is therefore the
**bin's own allocation count at rest**: zero growth over N forced flushes once
settled, at most one extra per correction. The bin exposes
`allocations_for_test()` and `corrections_for_test()` behind the existing
test-utils feature; a foreign `GtkListView`'s allocations cannot be counted, but
its `queue_allocate` marks the ancestry, so the bin re-allocates whenever the
child does.

### Findings from implementation (2026-09-21)

- **Each half passes the rendered-bounds tests alone, but they are not
  symmetric.** C2 is the general mechanism: it makes a fixed point exist (the
  child agrees with what it is handed, so its anchor re-derivation lands on the
  published offset). The re-assert is the invariant guard for residuals. Alone
  it is not a fix: every allocation the child would rewrite the page, settle,
  be corrected, and its own `value-changed` handler would queue another
  allocation — the at-rest counter test is what separates the two, and it is
  why both are kept.
- **The out-of-allocation path did not fire** on selection or focus in either
  suite; the settles all arrived inside allocation, on the list's own
  reveal-focus scroll. No causal discriminator was needed.
- **The forwarded delta must stay `child_value − viewport_top`.** The gentler
  `child_value − published_offset` (move the outer by what the child moved) was
  implemented and broke focus traversal: `GtkListBase` treats every
  `value-changed` as a user scroll, re-anchors on the value, and drops any
  pending `scroll_to`. After the outer moved by the child's own delta the
  re-slice published `child_value − chrome`, the emission wiped the child's
  anchor, and a request issued between the outer move and the re-slice (row 30
  after row 21) was never made. Landing the outer where the slice offset equals
  the child's value means the re-slice republishes the value the child already
  holds and nothing is emitted. The cost is that an honoured request below
  chrome scrolls that chrome away (52px for a 5px reveal); the positive
  controls assert the row is revealed and the scroller then rests, not the
  travel. The draft's "one pixel moves the outer by one pixel" scenario was
  wrong and is replaced.
- The inset is learned as `slice_height − page_size` when the child changes the
  page during allocation, and the bin queues one more allocation so the
  corrected geometry lands the same frame. `StyleContext::padding` is
  deprecated under the lint gate and observation is more faithful anyway.

### Out-of-allocation moves — a hypothesis, not a cause

The first review proposed that `child_adjustment_moved` (which passes
`reconfigure_shift = 0`) forwards an anchor settle delivered from selection or
focus handling. The second review showed the forwarded magnitude would be
`child_value − viewport_top` ≈ 55px at the top, not `d`, and that
`assert_sidebar_rests_at_top` samples 12 times over 1.4s and is green — so this
path does not fire at rest. It stays a hypothesis owned by the instrumentation
step. If it does fire, no magnitude rule can separate a settle from a genuine
one-pixel keyboard request (both are quantized to anchor items, and C2 does not
remove item quantization); the discriminator must be causal — a value move
accompanied by a change in `child.focus_child()` is a request, the same focus
child is a settle — and the change must add the positive control the suite
lacks: Down arrow onto a row clipped by one pixel scrolls the outer by about
one pixel.

## Rounding

`viewport_top` is an f32 from `compute_point`; `whole_pixels` rounds it;
`ADJUSTMENT_EPSILON = 0.5` sits on the boundary. Instrumentation logs the
pre-rounding value. Default tolerance for the invariant is **0px**
(pixel-identical bounds); if the pre-rounding top is seen oscillating around
`.5`, the fix is to round once, consistently, and the tolerance stays 0.

## Rejected

- **C1 — stop configuring `upper`/`page_size`.** On the first allocation the
  fresh adjustment is `[0,0]`, `set_value` clamps to 0, and the bin reads the
  clamp back as `published`, so a restored scroll position or a second section
  below the fold renders misaligned by the whole `slice_top` and the bin cannot
  detect it. In steady state the clamp uses the previous frame's child `upper`,
  which the suite documents as moving mid-trip.
- **Offsetting `value` by the inset.** Wrong by derivation above: it eats the
  visible top padding and breaks the bottom edge where the offset is clamped
  away.
- **Reading the inset from `StyleContext::padding`.** Deprecated since 4.10
  under the blocking lint gate; observation is also more faithful because it
  measures what the child actually subtracted.
- **B — double allocation, `transform := settled value`.** Legal, cannot loop,
  but 2× layout every frame at rest, forces `transform == value` (disables the
  anchor compensation), and "bound at two passes then assert" contradicts
  itself.
- **Snapshot-time compensation.** GTK4 routes input through the allocation
  transform; clicks would land up to 7px off the drawn row, and
  `compute_bounds` would agree with a broken app.
- **Previous frame's value.** One visibly wrong frame is still the bug.
- **`debug_assert!` in `size_allocate`.** A panic in a vfunc unwinds through
  the glib trampoline and aborts the process; the widget harness is one process
  per binary, so one regression would kill the run without attribution.
- **Magnitude rules for out-of-allocation moves** ("smaller than one row",
  "within one frame of a reconfigure"). Both swallow a genuine one-pixel
  keyboard request.

## Proof

Rendered-bounds recipe (both suites):

- Resolve the row **by label** each sample via `rendered_row_widget`; list
  widgets are recycled and a held handle can rebind.
- Measure `compute_bounds` relative to the **section header**, not the sidebar,
  so a genuine outer move cannot masquerade as stability and vice versa.
- Force layout between samples (`queue_allocate` + flush), then sample;
  `flush_after_delay` alone waits for the compositor.
- Drive selection with `selection.set_selected(i)` and focus with
  `grab_focus()` on rows `inside_outer_viewport` already accepts; assert
  `outer.value()` unchanged per target as a precondition. `select_and_scroll_to`
  always issues a scroll request and is reserved for the positive control.
- Static fixtures (short window, two sections, refresh over an identical tree)
  assert across the event. Model-changing fixtures (expand, collapse,
  hidden-files) assert **post-settle** stability only: N samples identical
  after the event, no across-the-event comparison.
- A padded fixture in `gtk_lush_adoption.rs` (a `GtkListView` with a CSS class
  giving asymmetric padding) proves the contract where it lives; the current
  adoption fixture, the example, and the adoption lab are all unpadded and
  cannot see the defect.
- Allocation-count-at-rest test via `allocations_for_test()`.
- Positive control: a genuine one-pixel keyboard request still moves the outer.
- Live-session confirmation of the reported gesture before sign-off.
