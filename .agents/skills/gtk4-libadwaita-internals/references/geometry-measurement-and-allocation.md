# Geometry, Measurement, and Allocation

## Table of Contents

- The request and allocation model
- `gtk_widget_measure()` and why illegal `for_size` values warn
- Wrapper versus virtual method semantics
- `GtkBox` and why it is often named in warnings
- `GtkPaned` handle math and child slot math
- `GtkRevealer` transition scaling and integer rounding
- Snapshot surfaces used as paned children
- Libadwaita adaptive containers in the same pipeline
- Related warnings from the same invariant family
- Rust implications

## The Request And Allocation Model

GTK uses minimum size and natural size.

- Minimum size is the least space the widget can legally accept.
- Natural size is the size the widget prefers under normal conditions.
- Parents may allocate more than natural size, or less than natural size, but not less than minimum size.

GTK also uses height-for-width and width-for-height request modes. The `GtkWidget` docs are explicit:

- parents first request sizes in one orientation
- those results are then fed back as `for_size` in the opposite orientation
- widgets may be measured multiple times before being allocated once

That is why layout bugs are often recursive and why a single animated width can trigger a warning far deeper in the tree.

## `gtk_widget_measure()` And Why Illegal `for_size` Values Warn

The specific warning

```text
Trying to measure GtkBox ... for width of X, but it needs at least Y
```

is emitted in `gtk/gtksizerequest.c` inside `gtk_widget_measure()`.

The sequence is:

1. GTK is asked to measure a widget in one orientation with a concrete `for_size`.
2. Before it trusts that `for_size`, it measures the widget in the opposite orientation with `-1` to compute the minimum legal opposite size.
3. If the supplied `for_size` is below that minimum, GTK logs the warning and clamps `for_size` upward before continuing.

Important subtlety:

- If GTK is measuring in vertical orientation, the warning text says `width`.
- If GTK is measuring in horizontal orientation, the warning text says `height`.

That is because `for_size` always refers to the size in the opposite orientation.

So `Trying to measure ... for width of 414, but it needs at least 415` means:

- GTK was computing height
- the parent supplied width `414`
- the widget's minimum legal width was `415`
- GTK clamped the width upward and continued

## Wrapper Versus Virtual Method Semantics

The `GtkWidget` docs and `gtksizerequest.c` draw a hard line:

- use `gtk_widget_measure()` when measuring another widget, usually a child
- do not call `gtk_widget_measure()` on `self` from your own `measure` implementation
- call the class virtual method directly when a widget needs its own request internally

GTK warns if a widget re-enters the wrapper on itself during `measure`, because the wrapper applies CSS, margins, alignment, and size-group adjustments intended for external callers.

The same source file also validates:

- natural size must be `>=` minimum size
- minimum sizes must be non-negative
- baseline data must be internally consistent and inside the widget bounds

These are not optional style rules. They are the consistency checks that keep recursive layout from spiraling into nonsense.

## `GtkBox` And Why It Is Often Named In Warnings

`GtkBox` is often the widget named in measurement warnings because its minimum width or height is derived from many child requests.

`GtkBox` itself is a `GtkWidget`, but its layout work lives in `gtk/gtkboxlayout.c`. Along the main axis it:

- measures every visible child
- adds spacing between children
- accounts for homogeneous distribution and expansion
- tracks baseline-related size when horizontally aligning baselines

This makes `GtkBox` a common symptom site:

- the real cause may be a deep child whose minimum size grew
- the box's own minimum then becomes unsatisfiable
- the warning surfaces at the box because that is the widget GTK was directly measuring

Do not stop at the widget name. Inspect the box's visible children, spacing, CSS extras, and any height-for-width descendants such as wrapping labels, search entries, or custom widgets.

## `GtkPaned` Handle Math And Child Slot Math

`gtk/gtkpaned.c` makes `GtkPaned` especially sensitive to narrow widths:

- minimum and natural size on the main axis include both children and the handle size
- the handle is measured as a real widget and participates in size math
- during allocation, child minimums are remeasured against the actual opposite-axis size
- the slot available to the second child is `width - start_child_size - handle_size`

Subtle but important details from source:

- the paned subtracts the handle before computing child slots
- it still allocates children at least `MAX(1, slot)`
- if a child's minimum exceeds its slot, GTK inflates the child's allocation to its minimum
- that can shift positions or make animation endpoints temporarily illegal

This is why paned bugs often show up only during live animations or after restoring a large widget subtree. The handle is not bookkeeping; it consumes width.

## `GtkRevealer` Transition Scaling And Integer Rounding

`gtk/gtkrevealer.c` explicitly documents its precision problem in code comments.

During transitions:

- the revealer requests a scaled size for the child
- it still allocates the child at its unscaled size so the child renders correctly
- it reverse-applies scale when allocating the child
- it uses `floor()` and `ceil()` when translating between scaled and unscaled sizes

The source calls out the consequences:

- scaled requests are rounded up with `ceil()`
- reverse-mapping uses `floor()`
- tiny scales can produce disproportionate integer jumps
- min and natural sizes are preferred special cases to avoid random reversals

This is the exact class of math that creates one-pixel warnings in animation-heavy layouts. A `GtkPaned` width budget can be mathematically valid at one stage, then become invalid by one pixel after revealer scaling, handle subtraction, CSS extras, or a late child minimum remeasurement.

## Snapshot Surfaces Used As Paned Children

Replacing a heavy live child with a frozen snapshot can improve animation smoothness, but the snapshot is still part of GTK's measurement pipeline.

Important consequences:

- a `GtkPicture` or other snapshot host does not automatically inherit the live child's minimum width contract
- if the snapshot surface reports a smaller minimum than the real child, `GtkPaned` may believe the start child can shrink further than it really can
- the resulting warning may still name the end child, because that is the widget GTK was directly measuring when the illegal `for_width` finally surfaced
- a stable host such as `GtkStack` sitting directly under `GtkPaned` is itself part of the contract; preserving the inner child's width floor is not enough if the host is the real paned child

Real-world failure pattern:

- the live sidebar has a minimum width, for example 200px
- a frozen snapshot replaces it during hide or show animation
- the snapshot host reports `0` or another undersized minimum width
- `GtkPaned` reallocates more width to the end child than should be legal
- GTK later measures the end `GtkBox` with a width that is one pixel too small and warns there, even though the root cause is the start-child snapshot contract

The robust fix pattern is:

- preserve the live child's minimum width on the snapshot host, for example with `width-request` set from `live_child.measure(Horizontal, -1)`
- if the paned child is a stable host such as `GtkStack`, preserve that same width floor on the host itself as well
- treat the snapshot as a geometry participant, not only a paint optimization
- if you use a stable host such as `GtkStack` or a similar multiplexer purely to swap between live and frozen children, disable that host's own transitions unless you explicitly want a second animation system
- generate or refresh the snapshot off the direct interaction path when possible, such as idle time or a steady-state refresh, because synchronous snapshot capture on the click path can remove the warning but still cause hide-time stutter
- do not treat `paintable().is_some()` as proof that the frozen image is visually valid; `GtkWidgetPaintable` can still yield an empty or transparent current image when the observed widget has no usable render node yet
- when a fresh `GtkSnapshot::to_paintable()` capture still produces a black or empty frozen pane, prefer a persistent `GtkWidgetPaintable` observer and freeze its warmed `current_image()` instead of demanding a brand-new render snapshot at hide start
- if a frozen content pane stretches or shows a seam near the final collapsed frame, inspect `GtkPicture:content-fit` and ask whether the content pane should remain live while only the expensive opposite pane is frozen

When debugging this class of bug, compare the warned widget pointer with the actual widget pointers in the live tree. If the warned widget is the end child, still inspect the start-child wrapper or snapshot surface before assuming the end child is the root cause.

## Libadwaita Adaptive Containers In The Same Pipeline

Libadwaita widgets are not outside GTK's layout model. They are GTK widgets that implement custom measurement and allocation.

`AdwNavigationSplitView` in `src/adw-navigation-split-view.c`:

- measures sidebar and content bins separately
- clamps sidebar width against explicit minimum and maximum width properties
- computes sidebar natural width from content natural width and the configured fraction
- allocates sidebar and content side by side when uncollapsed
- switches to a navigation-view-based model when collapsed

`AdwToolbarView` in `src/adw-toolbar-view.c`:

- measures top bar, bottom bar, and content
- subtracts top and bottom bars from content height unless content is allowed to extend behind them
- allocates bars and content with translated transforms
- updates undershoot-related CSS classes based on actual edge and reveal state

So when debugging a Libadwaita layout bug, still ask the GTK questions:

- what is the minimum legal size
- which widget supplied the illegal `for_size`
- which wrapper or container changed the budget

## Related Warnings From The Same Invariant Family

These warnings usually sit adjacent to the same root cause:

- `widget tried to gtk_widget_measure inside GtkWidget::measure implementation`
- `reported min width ... natural width ... natural size must be >= min size`
- `reported baseline for only one of min/natural`
- `Allocating size to ... with stale request mode`
- `Allocating size to ... without calling gtk_widget_measure()`
- `Allocation width too small ... needs at least ...`
- `gtk_widget_size_allocate(): attempt to allocate ... with width ... and height ...`
- `... called gtk_widget_queue_resize() during size_allocate()`
- `Trying to snapshot ... without a current allocation`

Read them as a family:

- bad measurement data
- stale request mode
- illegal animation endpoints
- recursive resize during allocation
- render before allocation settled

## Rust Implications

- In Rust subclasses, the same measurement and allocation rules apply. If your widget implements `measure` or `size_allocate`, obey GTK's contracts exactly.
- When reviewing application code, clamp animated widths before they reach paned or split-view allocation, not after warnings appear.
- Treat one-pixel warnings as real. They often mean a layout budget is only accidentally valid on one frame or one monitor scale.
- If a snapshot surface replaces a live paned child, preserve the live child's minimum width on the snapshot widget or host container. Otherwise GTK can under-measure the opposite child while still naming that opposite child in the warning.
- If the actual paned child is a wrapper host, preserve the legal width floor on that host too. A descendant `width-request` does not automatically satisfy GTK when the host itself is what `GtkPaned` measures.
- Do not capture heavyweight snapshots synchronously on the click path unless you have measured that cost. Moving snapshot refresh to idle or another steady-state moment can preserve smoothness without reintroducing geometry bugs.
- Freeze only the pane that benefits from it. Keeping the content pane live while freezing the heavy sidebar can be the correct fix when content snapshots introduce distortion or end-of-animation artifacts.
- Validate complex geometry fixes in a live app session, not only in widget tests. The rounding and handle math happens at runtime with real allocations.
