# gtk-lush-viewport

`gtk-lush-viewport` is a `0.0.0` GTK Lush family crate for observing gtk-rs
scroll adjustment geometry.

## Internal Platform Status

This is a functional in-tree `0.0.0` implementation for LushText's internal
platform. It is not a stable external dependency and is not a crates.io release
candidate. The current API exists so LushText can keep viewport observation
small, local, and reviewable.

Follow the current posture in `docs/next/gtk-lush.md`. Baseline adoption
evidence for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

Use this crate when viewport geometry is best represented by a `GtkAdjustment`
page-size or value change. This is especially useful for widgets whose class
uses a layout manager: overriding `size_allocate` on those widgets is the wrong
trap, because GTK will not call the subclass vfunc.

The crate reports observation events and rest state. Callers still own repair
policy such as scroll clamps, minimap refreshes, focus-mode geometry, and visual
readiness.
