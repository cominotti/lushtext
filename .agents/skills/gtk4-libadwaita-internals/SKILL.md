---
name: gtk4-libadwaita-internals
description: "Deep operational guide to GTK4, Libadwaita, and GtkSourceView internals for Rust applications using gtk4-rs, libadwaita-rs, and sourceview5-rs. Use when investigating widget lifecycle, measurement and allocation, focus and actions, builder templates, CSS nodes, list virtualization, adaptive Libadwaita containers, GtkSourceView editor features, GTK Lush widget/viewport/signal/settle contracts, or GTK and Adwaita warnings and criticals. Trigger on warnings such as `Trying to measure ... needs at least ...`, allocation or snapshot or parenting errors, Paned or Revealer or Box or ListView or TreeListModel behavior, or when implementing or reviewing custom GTK widgets and editor projections in Rust and you need the toolkit contract rather than app-level heuristics."
---

# GTK4 Libadwaita Internals

## Overview

Use this skill when the real question is "what contract is GTK, Libadwaita, or GtkSourceView enforcing here?" rather than "what quick patch makes my app stop complaining?" It is built for Rust codebases, but every conclusion must come from official GNOME docs and official GTK, Libadwaita, or GtkSourceView source.

## Source Discipline

- Treat only official GNOME docs and official GTK, Libadwaita, or GtkSourceView source as authoritative.
- Start with docs for the public contract. Escalate to source for warning emitters, allocation math, child-type parsing, signal ordering, or behavior that the docs describe only loosely.
- Ignore blog posts, forum answers, StackOverflow snippets, and binding docs when settling disputed behavior.
- Use supporting GNOME platform docs on `docs.gtk.org` for `GObject`, `Gio`, and `GLib` only when GTK or Libadwaita docs point into them.

## Version Discipline

- This repo targets `gtk4 = 0.11` with feature `gnome_50`, `libadwaita = 0.9` with feature `v1_9`, and `sourceview5 = 0.11` with feature `v5_18`.
- Treat GTK 4.22.x, Libadwaita 1.9.x, and GtkSourceView 5.20.x as the valid source families.
- The docs sites may render newer library versions. Honor `since` markers and do not recommend APIs added after GTK 4.22, Libadwaita 1.9, or the enabled GtkSourceView binding feature floor unless the user explicitly asks for forward-looking guidance.
- Micro releases inside the same stable family are acceptable for source lookup because the invariants and warning paths are the same class of behavior this skill is documenting.

## Quick Start

1. If the task contains a warning or critical, read [references/warnings-and-criticals.md](references/warnings-and-criticals.md) first.
2. If it is about size, animation, clipping, or a one-pixel layout warning, read [references/geometry-measurement-and-allocation.md](references/geometry-measurement-and-allocation.md).
3. If it involves `GtkListView`, factories, models, row reuse, or `GtkTreeListModel`, read [references/containers-lists-and-factories.md](references/containers-lists-and-factories.md).
4. If it involves templates, builder XML, actions, focus, CSS, or accessibility metadata, read [references/builder-templates-actions-css-accessibility.md](references/builder-templates-actions-css-accessibility.md).
5. If it involves adaptive layouts, split views, header bars, toolbars, breakpoints, or page navigation, read [references/libadwaita-adaptive-surfaces.md](references/libadwaita-adaptive-surfaces.md).
6. If it involves GtkSourceView editor features such as marks, gutters, annotations, hover providers, completion, style schemes, or text-buffer projections, use [references/official-sources.md](references/official-sources.md) to confirm both the GtkSourceView source contract and the Rust binding feature gate.
7. If it involves parentage, mapping, visibility, disposal, or object ownership, read [references/lifecycle-and-ownership.md](references/lifecycle-and-ownership.md).
8. When the docs are too high-level, use [references/official-sources.md](references/official-sources.md) to jump to the exact upstream source file and function.

## GTK Lush Mapping

After confirming the GTK/Libadwaita contract, map fitting app work to existing
GTK Lush primitives instead of hand-rolling local helper patterns:

- Signal, binding, and controller lifetimes -> `gtk-lush-signals`.
- UI debounce, superseding timers, and readiness-linked settle bursts ->
  `gtk-lush-settle`.
- Adjustment observation, rest-state, and lower-edge geometry ->
  `gtk-lush-viewport`.
- Zero-min clipping and render-hold/capture overlay behavior ->
  `gtk-lush-widgets`.

This skill establishes toolkit truth; it does not approve new GTK Lush APIs.
When the existing primitives do not fit and the change proposes a new or
reshaped GTK Lush contract, also use `gtk-lush-stewardship`.

## Operating Rules

- Separate these phases in your reasoning: measure, allocate, snapshot, map or unmap, realize or unrealize, dispose, finalize.
- Distinguish the widget named in a warning from the widget that caused the invariant to fail. Containers often surface child problems.
- Trace both directions through the widget tree. Parents distribute constraints downward; children push minimums upward.
- Account for CSS margin, border, padding, and wrapper style classes. They participate in measurement even when visible content seems unchanged.
- When the symptom is a few pixels of rendered effect drift, identify whether
  the visible pixels are toolkit-owned (for example a private CSS node) or
  app-owned drawing. If tests need a stable invariant, preserve the native or
  app-rendered effect and verify it with screenshot-derived pixel anchors from a
  broad crop. Automation1 geometry may choose the crop and explain diagnostics;
  do not prove only a computed rectangle while the visible pixels come from a
  separate toolkit or CSS path.
- For collection, browser, picker, and status-page surfaces, reason through the
  measurement contract at both ends of the data shape: zero children or empty
  model, representative content, many rows, long labels, and constrained
  allocation. GTK can produce a valid tree that is still human-unusable if the
  empty state follows a narrow natural child size or the dense state lets the
  row area expand instead of scrolling.
- Treat animated containers as layout participants, not visual sugar. `GtkRevealer`, `GtkPaned`, `AdwNavigationSplitView`, and `AdwToolbarView` all feed real numbers into GTK's request and allocation pipeline.
- Translate every conclusion back into Rust terms before finishing. The invariant comes from GTK; the Rust binding changes syntax and ownership ergonomics, not toolkit semantics.

## Rust Framing

- In Rust, the semantic contract matches the C docs and source exactly: custom widget `measure`, `size_allocate`, `snapshot`, buildable child types, action or focus behavior, and GtkSourceView editor projections are toolkit rules, not binding-specific inventions.
- A `WidgetImpl::size_allocate` override compiling does not prove GTK will call it for the path you need. If a subclass inherits or installs a layout manager (for example a `GtkBox`/`GtkBoxLayout` path), verify the vfunc empirically before hanging repair logic on it; for text-view viewport reflow, scroll-adjustment `page-size` changes can be the live allocation signal.
- When subclassing, never re-enter the measurement wrapper on `self` from your own `measure` implementation. Measuring child widgets via the wrapper is correct.
- Keep GTK objects on the main thread. Background threads may compute data, parse files, or search the filesystem, but they must hand plain data back to the UI thread.
- Use weak references or equivalent patterns for long-lived closures so disposal can complete and widgets can actually die.

## References

- [references/official-sources.md](references/official-sources.md): allowed sources, version bounds, source-file map, and repeatable upstream lookup commands
- [references/lifecycle-and-ownership.md](references/lifecycle-and-ownership.md): widget lifecycle, parentage, visibility, mapping, and ownership invariants
- [references/geometry-measurement-and-allocation.md](references/geometry-measurement-and-allocation.md): request modes, min and natural sizes, baselines, Box or Paned or Revealer math, and why `Trying to measure ...` happens
- [references/containers-lists-and-factories.md](references/containers-lists-and-factories.md): `GtkListView`, `GtkSignalListItemFactory`, `GtkTreeListModel`, selection, reuse, and scroll integration
- [references/builder-templates-actions-css-accessibility.md](references/builder-templates-actions-css-accessibility.md): Builder XML, composite templates, actions, focus, CSS nodes, and accessibility metadata
- [references/libadwaita-adaptive-surfaces.md](references/libadwaita-adaptive-surfaces.md): `AdwBreakpoint`, `AdwNavigationSplitView`, `AdwToolbarView`, `AdwViewStack`, and adaptive chrome behavior
- [references/warnings-and-criticals.md](references/warnings-and-criticals.md): high-signal warning atlas with upstream source paths and likely invariant violations
