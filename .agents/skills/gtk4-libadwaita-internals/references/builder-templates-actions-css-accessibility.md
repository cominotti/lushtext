# Builder, Templates, Actions, CSS, and Accessibility

## Table of Contents

- `GtkBuildable` child semantics
- Composite templates
- Builder-time layout and accessibility metadata
- Actions and focus
- CSS nodes and wrapper semantics
- Accessibility contracts that affect debugging
- Rust implications

## `GtkBuildable` Child Semantics

GTK and Libadwaita widgets often accept typed children, not just arbitrary descendants.

Examples from official docs and source:

- `GtkPaned` expects `start-child` and `end-child`
- `AdwToolbarView` expects `top`, `bottom`, and `content`
- `AdwNavigationSplitView` expects `sidebar` and `content`
- `AdwBreakpoint` accepts `<condition>` and `<setter>` elements

If the child type or property name is wrong, Builder warnings are usually precise:

- wrong child type
- unknown layout property
- unknown accessible property, relation, or state
- invalid value conversion for a property

Do not patch around these with ad hoc code. Fix the XML or the object type to match the widget's `GtkBuildable` contract.

## Composite Templates

The `GtkWidget` docs define the template contract:

- the XML lives under `<interface>`
- the composite widget uses a `<template>` element
- the `class` attribute must name the widget type
- the template is associated during class initialization
- the template is initialized during instance initialization
- template children are disposed during dispose

The source in `gtkwidget.c` warns when template precompilation fails:

```text
Failed to precompile template for class ...
```

That is a structural template problem, not a transient runtime issue.

## Builder-Time Layout And Accessibility Metadata

`GtkWidget`'s buildable implementation supports:

- `<layout>` for layout-manager child properties
- `<style>` for CSS classes
- `<accessibility>` for properties, relations, and states

Upstream warnings in `gtkwidget.c` cover:

- unknown layout properties for the layout manager
- failed property conversion for layout children
- unknown accessible properties or relations or states
- invalid accessible values
- missing referenced accessible objects

These are especially easy to miss in large template files because the app may still render partially while silently dropping the invalid metadata.

## Actions And Focus

Many GTK and Libadwaita widgets define built-in actions.

Examples from official docs:

- `GtkListView` defines `list.activate-item`
- `AdwNavigationSplitView` forwards the same navigation actions as `AdwNavigationView`

When actions do not fire, verify:

- the action exists on the widget class
- the parameter type matches
- the focused widget and ancestor chain are what you think they are

Focus bugs often look like action bugs because actions and mnemonics travel through the widget ancestry and current focus context.

## CSS Nodes And Wrapper Semantics

CSS is part of measurement, not just painting.

Examples from official docs:

- `GtkBox` uses CSS node `box`
- `GtkListView` uses node `listview`, rows use node `row`
- `GtkRevealer` uses node `revealer`
- `AdwToolbarView` uses toolbar and undershoot style classes as part of its edge presentation

The `GtkRevealer` docs make an especially important point:

- it hides its contents, not itself
- margins, padding, and borders on the revealer remain visible and measurable even when `reveal-child` is `FALSE`

So if a hidden panel still influences geometry, inspect the wrapper's CSS and allocation, not only the child visibility.

### Fixed-Height Rows And Flat Icon Button Hover Bounds

In fixed-height header or selector rows, a flat icon button's hover or active highlight is still drawn from the button widget's own allocation and margins. If the button sits directly inside the row with no vertical constraint, the highlight can look like it bleeds past an adjacent dropdown, label, or other centered control even when the row height itself is correct.

For this class of bug, fix the button widget itself instead of pushing sibling content away:

- keep the button `valign="center"`
- add explicit `margin-top` and `margin-bottom` on the button
- do not fake the spacing by adding top margin to the list or scroller below when the visible problem is the button's own hover pill

This matters in Builder XML as much as in code-built widgets. The row can keep the same `height-request` while the button gets its own breathing room inside that row.

## Accessibility Contracts That Affect Debugging

Accessibility is not an afterthought when debugging widget behavior.

Examples from official docs:

- `GtkListView` uses role `LIST`, rows use role `LIST_ITEM`
- `GtkRevealer` keeps its child in the accessibility tree regardless of reveal state
- `AdwToolbarView` uses role `GROUP`
- `GtkBox` changed from `GROUP` to `GENERIC` in newer GTK versions, so always verify role changes against the target version

This matters when a widget appears hidden or inert visually but still participates in accessibility, focus traversal, or semantics.

**Hover Actions and HIG Accessibility:** Hover-only interactions (`GtkEventControllerMotion`) are inherently invisible to keyboard-only users and screen readers. The GNOME HIG requires that any UI action triggered exclusively by pointer hover MUST have an accessible alternative. Usually, this means putting the identical action into the element's right-click context menu (`GtkPopoverMenu`).

## Rust Implications

- In Rust composite widgets, respect the same template child types, property names, and lifecycle phases that the C docs describe.
- If Builder warns about layout or accessibility metadata, fix the XML or target object type first. Do not assume the binding layer translated it incorrectly.
- Remember that CSS classes on wrapper widgets affect measurement in Rust exactly as they do in C.
