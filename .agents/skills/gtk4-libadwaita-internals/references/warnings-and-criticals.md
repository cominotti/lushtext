# Warnings and Criticals

## Table of Contents

- How to read a GTK or Adwaita warning
- Geometry and allocation warnings
- Lifecycle and parenting warnings
- List and model warnings
- Builder and template warnings
- Libadwaita adaptive warnings
- Generic criticals

## How To Read A GTK Or Adwaita Warning

Treat the message as the name of a violated invariant.

- The widget named in the warning is the place GTK detected the problem, not always the original cause.
- The source file that emits the warning is often more informative than the widget class alone.
- Related warnings often come in clusters. Do not fix the second message while ignoring the first.

Also keep one nuance in mind:

- GTK itself can hit these warnings too. `gtk-4.20.4/NEWS` includes a fixed bug about a `GtkPopover` measurement warning.

So the right posture is:

- assume your app violated an invariant until proven otherwise
- confirm against official source
- only then consider an upstream bug hypothesis

## Geometry And Allocation Warnings

### `Trying to measure ... for width or height of X, but it needs at least Y`

- Emitted from `gtk/gtksizerequest.c:gtk_widget_measure`
- Means the supplied opposite-axis size was smaller than the widget's minimum legal opposite-axis size
- GTK clamps upward and continues
- First checks:
  - parent width or height budget
  - wrapper widgets that still participate in layout
  - `GtkPaned` handle subtraction
  - `GtkRevealer` transition rounding
  - CSS margin or border or padding

### `widget tried to gtk_widget_measure inside GtkWidget::measure implementation`

- Emitted from `gtk/gtksizerequest.c`
- Means a widget re-entered the public measurement wrapper on itself from inside its own `measure` implementation
- First checks:
  - custom widget subclass code
  - layout manager code measuring the owner instead of children

### `reported min width or height ... natural size must be >= min size`

- Emitted from `gtk/gtksizerequest.c`
- Means a widget reported inconsistent measure data
- First checks:
  - custom measure implementation
  - baseline and size adjustments

### `Allocating size to ... with stale request mode`

- Emitted from `gtk/gtkwidget.c`
- Means request-mode assumptions changed between cached measurement and allocation
- First checks:
  - widgets whose request mode depends on child state
  - child additions or removals during layout-sensitive transitions

### `Allocating size to ... without calling gtk_widget_measure()`

- Emitted from `gtk/gtkwidget.c`
- Means GTK saw allocation happen while resize was still queued
- First checks:
  - allocation code that skipped measurement
  - stale geometry caches
  - mutation during allocation

### `Allocation width or height too small ... needs at least ...`

- Emitted from `gtk/gtkwidget.c`
- Means allocation fell below minimum size in debug builds
- First checks:
  - same root causes as the `Trying to measure` warning
  - usually later in the same failure chain

### `... called gtk_widget_queue_resize() during size_allocate()`

- Emitted from `gtk/gtkwidget.c`
- Means code invalidated geometry during an active allocation pass
- First checks:
  - setters or notifications triggered from `size_allocate`
  - callbacks mutating child structure mid-allocation

### `Trying to snapshot ... without a current allocation`

- Emitted from `gtk/gtkwidget.c`
- Means rendering reached a widget whose layout is not current
- First checks:
  - allocation timing
  - visibility toggles racing with render
  - custom paint or snapshot logic

## Lifecycle And Parenting Warnings

### `Can't set new parent ... which already has parent ...`

- Emitted from `gtk/gtkwidget.c`
- Means a widget is still owned by another container
- First checks:
  - container removal path
  - reusing the same widget instance in multiple places

### `A window is shown after it has been destroyed`

- Emitted from `gtk/gtkwindow.c`
- Means code is re-showing a dead toplevel
- First checks:
  - async callbacks that outlive the window
  - action handlers using stale references

### `gtk_window_set_titlebar() called on a realized window`

- Emitted from `gtk/gtkwindow.c`
- Means titlebar structure changed too late in lifecycle
- First checks:
  - late setup work
  - trying to treat titlebar as a dynamic child without understanding window lifecycle

## List And Model Warnings

### `Duplicate item detected in list. Picking one randomly.`

- Emitted from `gtk/gtklistitemmanager.c`
- Means list item identity or update sequencing is inconsistent
- First checks:
  - model update semantics
  - reuse of the same item object in conflicting positions

### `The search bar does not have an entry connected to it`

- Emitted from `gtk/gtksearchbar.c`
- Means the search bar cannot route key capture correctly
- First checks:
  - `gtk_search_bar_connect_entry()` or equivalent wiring

### `GtkPaned only accepts two widgets as children`

- Emitted from `gtk/gtkpaned.c`
- Means Builder or code attempted an illegal third child
- First checks:
  - buildable child types
  - container structure assumptions

## Builder And Template Warnings

### `Failed to precompile template for class ...`

- Emitted from `gtk/gtkwidget.c`
- Means the template XML is structurally invalid for template precompilation
- First checks:
  - `<template>` structure
  - class name
  - signal handler names
  - resource path and syntax

### `Unable to find layout property ...`

- Emitted from `gtk/gtkwidget.c`
- Means a `<layout>` property does not exist for the current layout-manager child object
- First checks:
  - layout manager type
  - child property names

### `Failed to set accessible property or relation or state ...`

- Emitted from `gtk/gtkwidget.c`
- Means Builder accessibility metadata failed to parse or target resolution failed
- First checks:
  - property or relation or state name
  - value syntax
  - referenced object IDs

## Libadwaita Adaptive Warnings

### `Unable to parse condition: ...`

- Emitted from `src/adw-breakpoint.c`
- Means a breakpoint condition string is invalid
- First checks:
  - condition syntax
  - units and operators

### `Type '...' does not have a property named '...'`

- Emitted from `src/adw-breakpoint.c`
- Means a breakpoint setter targeted a nonexistent property
- First checks:
  - target object type
  - property spelling

### `Cannot add an object of type ... to AdwNavigationSplitView`

- Emitted from `src/adw-navigation-split-view.c`
- Means child type is wrong for the split view's buildable contract
- First checks:
  - use `AdwNavigationPage`
  - use `sidebar` or `content` child types

### `Trying to add sidebar or content with the tag ... but ... already has the same tag`

- Emitted from `src/adw-navigation-split-view.c`
- Means routing identifiers collided
- First checks:
  - unique tags for sidebar and content pages

### `While adding page: duplicate child name in AdwViewStack`

- Emitted from `src/adw-view-stack.c`
- Means page names are not unique
- First checks:
  - page `name` property
  - any view-switcher or page-selection logic that assumes unique names

## Generic Criticals

`GLib-GObject-CRITICAL`, `Gtk-CRITICAL`, and `Gdk-CRITICAL` often come from:

- `g_return_if_fail`
- `g_return_val_if_fail`
- explicit `g_critical`

Interpret them as contract failures, not mere log noise.

In Rust code, do not suppress or ignore them. Find the invariant first:

- wrong object type
- wrong thread
- wrong lifecycle phase
- wrong child type
- wrong property name or value
- impossible size or allocation ordering
