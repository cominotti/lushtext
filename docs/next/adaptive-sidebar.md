# Adaptive Sidebar

## Status: Superseded

This earlier note proposed preserving the current `GtkPaned`-based left sidebar
and layering adaptive behavior on top of it.

That is no longer the recommended direction.

The current canonical architecture proposal is:

- [Dual Sidebars: Workspace + Properties](./dual-sidebars.md)

Why this document was superseded:

- the `GtkPaned`-based sidebar animation has proven fragile across widths
- the project now explicitly wants both a left workspace sidebar and a right
  info/properties sidebar at the same time
- nested split views provide one coherent path for both requirements together
