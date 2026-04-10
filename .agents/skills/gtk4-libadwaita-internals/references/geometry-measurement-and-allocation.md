# Geometry, Measurement, and Allocation

## Table of Contents

- The request and allocation model
- `gtk_widget_measure()` and why illegal `for_size` values warn
- Wrapper versus virtual method semantics
- `GtkBox` and why it is often named in warnings
- `GtkPaned` handle math and child slot math
- `GtkRevealer` transition scaling and integer rounding
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
- Validate complex geometry fixes in a live app session, not only in widget tests. The rounding and handle math happens at runtime with real allocations.
