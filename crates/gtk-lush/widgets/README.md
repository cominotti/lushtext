# gtk-lush-widgets

`gtk-lush-widgets` is a `0.0.0` GTK Lush family crate for small reusable GTK
widgets and render-hold helpers.

## Pre-Publication Status

This is the first functional in-tree implementation. It is not a Phase 5 publication-ready
crate and is not a crates.io release candidate. The current
API exists so LushText can prove the extraction boundary before publishing any
GTK Lush crate.

Follow the roadmap in `docs/next/gtk-lush.md`.

## Scope

`ClipBin` is a single-child widget that reports zero minimum size while still
delegating natural size to its child and clipping snapshots to its allocation.

`RenderHoldOverlay` captures already-rendered child pixels into a non-targetable
cover picture, hides the live child, lets callers warm the live child beneath
the cover, and clears the cover with paired opacity restoration.

The crate does not schedule reflow timing, own readiness predicates, or encode
application-specific minimap behavior.
