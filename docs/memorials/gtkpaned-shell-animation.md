# The retired GtkPaned shell

LushText's window shell, Markdown preview pane and sidebar used `GtkPaned`
until the shell moved to `AdwOverlaySplitView`, `AdwMultiLayoutView` and
`AdwBottomSheet`. No `GtkPaned` remains in `crates/` or `resources/ui/`. This
memorial keeps the rules that shell needed and the animation regression that
produced today's allocation-path rules in
[`.agents/rules/widget-wiring.md`](../../.agents/rules/widget-wiring.md),
"Size-Dependent Constraints". The toolkit contracts below (measure before
allocate, the opposite-axis floor, `max-position`, revealer-wrapped panes) are
also covered by the `gtk4-libadwaita-internals` skill's geometry reference,
which is where to read them before adding a `GtkPaned` again
(`docs/next/split-panes.md` plans one).

## The installed-Flatpak animation regression

**Known Flatpak animation regression:** sidebar and document-properties open/close animations looked like they were running below the monitor refresh rate even though no obvious blocking I/O was present. The root cause was allocation-frame churn: `size_allocate()` repeatedly synchronized split-view widths, the properties fraction notify path rewrote GSettings, and the adaptive properties breakpoint was reparsed/reinstalled for each animated frame. The durable fix pattern is:

- `size_allocate()` compares the new allocated width against a cached width before doing split-view work.
- allocation and programmatic notify paths clamp runtime geometry only; they do not persist sidebar fractions to GSettings.
- the derived properties breakpoint threshold is cached, and `AdwBreakpoint::set_condition()` runs only when that integer threshold changes.
- persistence remains tied to explicit user intent, restore, or animation completion, not to every layout tick.

## GtkPaned position constraints (as they stood when the shell was retired)

Any code that sets a `GtkPaned` position must ensure it's valid for the current allocation width. GTK4's `measure()` phase runs BEFORE `size_allocate()` — if a paned position is stale from a previous frame, GTK warns "Trying to measure ... for width of X, but needs at least Y."

**Position restore from GSettings**: Always pre-clamp in `constructed()` immediately after `set_position()`. Use the restored window width from GSettings as the `for_width` parameter. Store the original unclamped value in `saved_*_pos` for animations that target the preferred position at wider widths.

**Hidden restore state**: If the pane starts hidden, do **not** leave the live `GtkPaned::position` at the expanded saved width. Restore the live position to the same collapsed endpoint the hide animation uses, while `saved_*_pos` keeps the preferred visible width.

**Animation targets**: When computing an animation target for a paned position (e.g., sidebar show), start from the saved preferred width but clamp the target against the current allocation before writing it. Clamp per-frame animation writes too — do not rely on `size_allocate` / `notify::position` to sanitize an already-invalid animation tick. If async child population can change the budget (for example restored workspaces adding sidebar sections), refresh the measured budget immediately before the animation starts.

**Real-session proof beats harness-only proof**: Presented widget tests can verify action state, wrapper visibility, and persistence guards, but they can still miss live `GtkBox ... needs at least ...` warnings. Any sidebar/paned animation fix must also be exercised through `make run` against restored workspaces while watching stderr. Treat widget green + live warning as a failed fix, not a partial success.

**Animation persistence**: Programmatic paned animations may still trigger `notify::position` and `size_allocate` on every frame. Those paths may clamp, but they must not enqueue debounced GSettings writes on every tick. Persist the preferred visible width once from `connect_done` (or the immediate-completion test path) after the animation settles.

**Clamp against the real end-child**: If the warning references the end-child container (for example `GtkBox`), budget against that container's measured minimum, not a nested child that usually dominates it. One-pixel mismatches often come from clamping against the inner stack while GTK is actually measuring the wrapper box.

**Use GTK's legal opposite-axis floor, not only the live-height floor**: `gtk_widget_measure()` validates a supplied `for_width` or `for_height` against the opposite orientation measured with `-1` before it continues. For paned budgeting, resolve each child's width floor as `max(measure(Horizontal, -1), measure(Horizontal, current_height))`. If you only use the height-adjusted floor, the app can still emit `Trying to measure ... needs at least ...` warnings even though the clamp looked correct in app code.

**Prefer GTK's runtime paned budget**: Once a `GtkPaned` is allocated, prefer `max-position` / `min-position` over reverse-engineering the legal range from child widths. Those properties already include the current handle width and realized child constraints.

**But know where `max-position` comes from**: GTK source (`gtkpaned.c`) computes positions with the handle widget's measured natural size and the current child minimums. During animated `GtkRevealer` transitions those minimums can round up by one pixel. If a live warning persists, inspect GTK's own `gtk_paned_compute_position` / `gtk_revealer_measure` behavior before adding more local clamp churn.

**Wrap zero-width pane animations**: If a pane must fully disappear, wrap the real child widget in a `GtkRevealer` (or equivalent clipping wrapper) and animate the paned against the wrapper, not the raw child. Keep the wrapper revealed while the paned is shrinking, then drop `reveal-child` and `visible` together when the animation finishes so GTK stops reserving handle width while hidden without changing the layout budget too early.

**Heavy child optimization**: If the wrapped pane contains a large tree/list that causes visible stutter, consider swapping the live child out for a frozen snapshot during the animation. But do not assume an intermediate container stops affecting layout; validate that the real child is truly out of the paned's measurement path in the live app.

**Hide-time clamps stay live until the wrapper is hidden**: A toggle action may set a logical `*-visible` flag to `false` before the `GtkRevealer` has actually left layout. Clamp/budget helpers must keep treating the pane as layout-active while the wrapper's own `visible` property is still true, or a stale restored position can slip into the first hide frame.

**New paned children**: When adding a new child to a `GtkPaned` with `shrink-end-child=false`, set `width-request` on the end-child to match its measured minimum. This makes the paned's minimum constraint explicit and prevents GTK from even attempting to measure the child at below its minimum during layout negotiation. Re-sync that width-request after map or async child restoration if the realized minimum changes.


## The paned bullets of "Size-Dependent Constraints"

- For child-initiated changes (e.g., user drags a `GtkPaned` divider), also connect `notify::position` on the child.
- If `notify::position` also persists state, suppress that persistence while a programmatic paned animation is in flight. Clamp can stay live every frame; debounced settings writes should run once from the animation completion path.
- Treat `notify::position` primarily as the **user-drag** path. If a timed animation is already driving valid paned positions directly, short-circuit the `notify::position` handler while that animation is active so the same frame is not reprocessed as if it were a manual drag.
