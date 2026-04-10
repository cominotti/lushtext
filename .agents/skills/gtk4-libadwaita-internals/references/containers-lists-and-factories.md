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
