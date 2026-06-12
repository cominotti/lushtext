# gtk-lush-viewport

`gtk-lush-viewport` is a `0.0.0` GTK Lush family crate for observing gtk-rs
scroll adjustment geometry.

## Pre-Publication Status

This is the first functional in-tree implementation. It is not a Phase 5b publication-ready
crate and is not a crates.io release candidate. The current
API exists so LushText can prove the extraction boundary before publishing any
GTK Lush crate.

Follow the roadmap in `docs/next/gtk-lush.md`. Adoption-validation evidence
for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

Use this crate when viewport geometry is best represented by a `GtkAdjustment`
page-size or value change. This is especially useful for widgets whose class
uses a layout manager: overriding `size_allocate` on those widgets is the wrong
trap, because GTK will not call the subclass vfunc.

The crate reports observation events and rest state. Callers still own repair
policy such as scroll clamps, minimap refreshes, focus-mode geometry, and visual
readiness.
