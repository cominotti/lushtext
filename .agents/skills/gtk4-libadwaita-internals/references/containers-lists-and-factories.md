# Containers, Lists, and Factories

## Table of Contents

- `GtkListView` virtualization
- `GtkSignalListItemFactory` lifecycle
- `GtkTreeListModel` on-demand child models
- Model identity and duplicate-item warnings
- Scroll integration and search-bar wiring
- Rust implications

## `GtkListView` Virtualization

`GtkListView` does not create one persistent row widget per item in the full model. The docs state that it uses its factory to generate one row widget for each visible item.

That means:

- row widgets are recycled
- row state must be reset when items change
- per-item signal connections cannot live forever on the row
- the data model and the row widget lifetime are intentionally decoupled

### The realized-widget cap

`GtkListView` realizes at most `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200, plus two extra items per tracker; `gtk/gtklistview.c`) row widgets for one visible range. "Visible" is decided by the list view's own vertical adjustment, so
whatever provides that adjustment defines the viewport. A `GtkScrolledWindow`
with `vscrollbar-policy: never` and `propagate-natural-height: true` gives the
list view its **entire content** as viewport: GTK allocates the full height but
populates only the first ~205 rows, and the rest is blank space that an outer
scroller happily scrolls through. The model is complete, the presentation is not.

Symptom signature: `tree_model.n_items()` is right, the list view's allocated
height equals `rows × row height`, and realized row widgets stop at ~205 with
the last rendered label far from the last model item.

Fix pattern: keep the list virtualized. Either let a real scroller own the list,
or, when the list must sit inside an outer scroller next to other content, host
it in `gtk_lush_widgets::ViewportSliceBin`, which advertises the full content
height to the outer scroller but allocates the child only the visible band and
owns its adjustments. Two GTK facts shape that bin: a `GtkListView` outside a
scroller reports its whole content as **minimum** height (so the host must own
that decision, as `GtkScrolledWindow` does), and `GtkViewport` allocates a
non-scrollable child its **minimum** in the scroll direction (so the host must
advertise the full content as minimum while inside the outer scroller, or the
outer range collapses to one page). The list applies `scroll_to` inside its own
allocation, so a host that owns the adjustment must read it back **after**
allocating the child and forward the request to the outer scroller against the
unclamped viewport position, from an idle rather than inside the layout pass.

Three more facts, each learned by shipping a regression (v0.8.1 → v0.8.2):

- **A `GtkScrollable` works in its CSS content box.** `GtkListView` measures
  its page and content *inside* its padding and border, so a host that hands it
  border-box `upper`/`page_size` sees both rewritten every allocation (the
  Libadwaita `navigation-sidebar` style pads 6px top and 4px bottom, hence a
  "mysterious 10px" reconfigure). The list then re-derives its value from its
  scroll anchor against a page the host's value was not chosen for and lands a
  few pixels off on every anchor change — a focus move is one — and renders
  there while the host's transform stays put. Publish `upper`/`page_size` in
  the child's frame; learn the inset from the page the child reports after its
  first allocation (`allocated_height − page_size`; `StyleContext::padding` is
  deprecated). Do **not** offset the value: a row at child content `y` draws at
  `transform + inset_top + (y − value)`, and the host's own measured frame
  already contains the inset, so `value = slice_offset` is right as it is.
- **`GtkListBase` drops a pending `scroll_to` on any `value-changed`.** It
  treats every value change as a user scroll and re-anchors on it. So a host
  honouring a child request must land the outer scroller where its own re-slice
  republishes *exactly* the value the child chose (`child_value −
  viewport_top`); configuring a different value emits, the anchor is wiped, and
  a request issued between the outer move and the re-slice is never made. The
  gentler "move only by what the child asked" (`child_value − published`) was
  implemented and lost keyboard-traversal requests. The cost is that an honoured
  request below chrome scrolls that chrome away.
- **Rows around the selection and focus stay realized but unmapped.** They are
  real children with `compute_bounds`, no pixels, and `child_visible = false`;
  any probe of what is drawn must filter `is_mapped()`, or it will read the
  bounds of a row that is not on screen.

The list view also carries presentation-level CSS classes such as `.rich-list`, `.navigation-sidebar`, and `.data-table`. Those are style decisions, not model decisions.

## `GtkSignalListItemFactory` Lifecycle

The official `GtkSignalListItemFactory` docs describe a strict order:

1. `setup`
2. `bind`
3. `unbind`
4. more `bind` and `unbind` cycles as the row is reused
5. `teardown`

Use those phases literally:

- `setup`
  Create the permanent widget structure for the row.
- `bind`
  Attach the current item to that structure and connect item-specific signals.
- `unbind`
  Undo item-specific wiring and clear row state that should not leak into the next binding.
- `teardown`
  Undo permanent setup-time wiring and let the row die.

The docs also note that listitem notifications are frozen during these signals. That means relying on property notify from the listitem itself during factory callbacks is the wrong mental model.

## `GtkTreeListModel` On-Demand Child Models

`GtkTreeListModel` is a list model that creates child models on demand.

Important consequences:

- the child-model creation callback is part of the live expansion path
- `autoexpand` changes behavior drastically because new rows expand by default
- `passthrough` changes whether callers receive original items or `GtkTreeListRow` wrappers

If your child-model callback performs I/O, model creation, or deep tree expansion work, `autoexpand` can multiply that cost immediately. In Rust apps this often shows up as a tree that "works" for tiny directories but explodes when pointed at a real project.

## Model Identity And Duplicate-Item Warnings

`gtk/gtklistitemmanager.c` contains:

```text
Duplicate item detected in list. Picking one randomly.
```

Treat that warning as a model-identity problem, not a rendering glitch.

Typical causes:

- the model produced inconsistent remove or add semantics
- the same object identity appeared in conflicting positions during an update
- list-item recycling found an impossible mapping between dead and live rows

When you see it, inspect the model update sequence before touching the row factory.

## Scroll Integration And Search-Bar Wiring

Two small but high-signal reminders from official source:

- `GtkListView` implements `GtkScrollable`, so it expects to live in a scrolling context that provides adjustments and viewport behavior.
- `gtk/gtksearchbar.c` warns if a `GtkSearchBar` is used without connecting an entry. The expected contract is to connect an entry so key capture can redirect correctly.

If a list or search surface behaves strangely, confirm the structural contract first:

- is the list inside the intended scroll container
- is the search bar wired to an entry
- is the row factory respecting reuse

## Rust Implications

- Build row widgets once in `setup`, not on every `bind`.
- Disconnect item-specific signals in `unbind`, not only in `teardown`.
- Treat `gio::ListModel` item identity as meaningful. Reusing the same object incorrectly can confuse GTK's recycling machinery.
- Be careful with `GtkTreeListModel::autoexpand` in Rust apps that create child models from filesystem data or any expensive source.
- **Deep Tree Nesting & Inline Actions**: When placing fixed action buttons (like a hover button) in a `GtkTreeListModel` row, DO NOT put the button inside the `GtkTreeExpander`'s content box. Deep nesting indentation will eventually push the button off-screen. Instead, wrap the `GtkTreeExpander` in a `GtkOverlay` and add the button as an overlay widget anchored to the right edge (`halign=End`). This guarantees the action remains visible regardless of tree depth.
