# Lifecycle and Ownership

## Table of Contents

- Widget state ladder
- Parent and child ownership
- Visibility, child visibility, mapping, and realization
- Allocation and snapshot timing
- Disposal and cleanup
- Rust implications

## Widget State Ladder

Think in phases, not in a single "widget exists" state:

1. Constructed
2. Parented into a widget tree
3. Rooted under a `GtkRoot`
4. Realized
5. Mapped
6. Measured and allocated
7. Snapshotted
8. Unmapped
9. Unrealized
10. Disposed and eventually finalized

The main failure mode in GTK debugging is mixing these phases together. A widget can be visible but not yet allocated. A widget can still exist after it has been unparented. A widget can have a child in the accessibility tree even while the child is visually hidden.

## Parent And Child Ownership

GTK widgets are single-parent objects.

- A widget may only have one parent at a time.
- Reparenting requires removing it from the old parent first.
- Container APIs are not cosmetic wrappers; they establish ownership and lifecycle.

The canonical warning is emitted from `gtk/gtkwidget.c`:

```text
Can't set new parent ... which already has parent ...
```

That warning means a widget is still owned by another container. The fix is not to clone a Rust wrapper or drop a Rust variable. The fix is to remove the widget from the old container or to create a fresh widget instance.

## Visibility, Child Visibility, Mapping, And Realization

These flags mean different things:

- `visible`: the widget wants to be shown
- `child_visible`: the parent currently allows the child to participate
- `mapped`: the widget is part of the current on-screen widget tree
- `realized`: the widget has completed surface-related realization work

Important consequences:

- Hiding a child does not erase its CSS node or necessarily its accessibility presence.
- A wrapper widget can remain visible and measurable even while its content is hidden.
- `GtkRevealer` documents this explicitly: it hides its contents, not itself, and its child stays in the accessibility tree.

## Allocation And Snapshot Timing

GTK assumes measurement happens before allocation, and allocation happens before snapshot.

Core warnings in `gtk/gtkwidget.c` reflect broken ordering:

- `Allocating size to ... without calling gtk_widget_measure()`
- `... called gtk_widget_queue_resize() during size_allocate()`
- `Trying to snapshot ... without a current allocation`

These are not random logs. They mean the widget tree is mutating or being asked to render while the layout contract is incomplete.

Practical reading:

- If allocation happens without a fresh measure, some parent computed sizes from stale information.
- If `queue_resize()` happens during `size_allocate()`, code is trying to invalidate geometry while GTK is still settling the current geometry pass.
- If snapshot happens without current allocation, rendering got ahead of layout.

## Disposal And Cleanup

Template children, signal handlers, timers, and async callbacks must stop keeping a widget alive when its container drops it.

GTK's template guidance in `GtkWidget` docs is explicit:

- associate the template at class init time
- initialize it during instance init
- dispose template children during dispose

Common lifecycle bugs are not memory leaks in the allocator sense; they are ownership leaks:

- signal closures retaining widgets forever
- periodic callbacks continuing after the widget is gone
- async completions trying to touch objects after disposal
- template children not being released when the widget is torn down

## Rust Implications

- Dropping a Rust binding handle does not necessarily destroy the underlying GTK object if GTK still holds references.
- Use container APIs like `set_child`, `append`, `remove`, `set_content`, and `set_sidebar` to express ownership changes. Do not treat Rust variable reassignment as a UI tree mutation.
- Use weak references or equivalent closure helpers for long-lived callbacks so disposal can complete cleanly.
- Treat `dispose`-time cleanup as real logic, not boilerplate. If a callback, monitor, or timer outlives the widget, GTK lifecycle warnings usually follow later.
