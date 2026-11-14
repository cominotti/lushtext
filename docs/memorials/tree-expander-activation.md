# TreeExpander file-row activation

Why workspace file rows disable `GtkTreeExpander`'s internal click gesture. The
rules are in [`.agents/rules/widget-wiring.md`](../../.agents/rules/widget-wiring.md),
"GTK4 Signal Delivery Pitfalls", and [`.agents/rules/ui.md`](../../.agents/rules/ui.md),
"File Tree".

## Three attempts

The sidebar file-activation code was correctly wired (`connect_activate` → `open_document` → `tab_view.append`), and `open_document` always creates tabs. But `GtkTreeExpander`'s internal `GtkGestureClick` intercepted mouse clicks at BUBBLE phase, preventing `GtkListView::activate` from firing. Fix attempt 1 (`single-click-activate=true`) changed the UX to single-click. Fix attempt 2 (CAPTURE-phase gesture) was fragile and failed for the first file due to `SingleSelection::selected()` timing. The correct fix: disable the expander's gesture for file rows by setting `propagation_phase = None` via `observe_controllers()` in `connect_bind`. This lets `GtkListView`'s built-in double-click activation work for files while preserving directory expand/collapse. When a child widget's internal gesture blocks parent behavior, **disable the gesture at the source** rather than trying to race it from a higher phase.
