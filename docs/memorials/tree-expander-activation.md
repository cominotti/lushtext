# TreeExpander file-row activation

Why workspace file rows disable `GtkTreeExpander`'s internal click gesture. The
rules are in [`.agents/rules/widget-wiring.md`](../../.agents/rules/widget-wiring.md),
"GTK4 Signal Delivery Pitfalls", and [`.agents/rules/ui.md`](../../.agents/rules/ui.md),
"File Tree".

## Three attempts

The sidebar file-activation code was correctly wired (`connect_activate` → `open_document` → `tab_view.append`), and `open_document` always creates tabs. But `GtkTreeExpander`'s internal `GtkGestureClick` intercepted mouse clicks at BUBBLE phase, preventing `GtkListView::activate` from firing. Fix attempt 1 (`single-click-activate=true`) changed the UX to single-click. Fix attempt 2 (CAPTURE-phase gesture) was fragile and failed for the first file due to `SingleSelection::selected()` timing. The correct fix: disable the expander's gesture for file rows by setting `propagation_phase = None` via `observe_controllers()` in `connect_bind`. This lets `GtkListView`'s built-in double-click activation work for files while preserving directory expand/collapse. When a child widget's internal gesture blocks parent behavior, **disable the gesture at the source** rather than trying to race it from a higher phase.

## Retired on GTK 4.24.1 (`slice-large-directory-collapse`, 2026-11-14)

The bind-time loop outlived the toolkit behaviour it answered. On GTK 4.24.1
`GtkTreeExpander` creates its toggle `GtkGestureClick` on the `expander` icon
child (`update_for_list_row`), re-creates it with the icon, and carries none
itself: the A114 probe counts zero click gestures on the expander and one on
the icon. The loop therefore matched nothing, and file rows never have an icon,
so their double-click activation reaches `GtkListView` without it (the widget
test `test_expand_routes_keep_gtk_behaviour` drives a real double press). The
loop was removed when the sidebar took the arrow click over for its paced
collapse: one capture-phase gesture on the expander, installed at setup, claims
the sequence on a release inside the current icon. That is a capture-phase
gesture, which attempt 2 above rejected, but for a different job: it does not
race `GtkListView`'s activation (it lets every press through and claims only a
release inside the arrow, where GTK's own gesture claims too), so the
`SingleSelection::selected()` timing that broke attempt 2 does not arise.
