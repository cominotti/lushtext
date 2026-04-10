# GTK Log Patterns

Use this file to sort live-session evidence quickly. It is a triage index, not the final authority on GTK behavior.

For any GTK or Adwaita warning or critical, prefer these source-backed references once you know which family of failure you hit:

- [../../gtk4-libadwaita-internals/references/warnings-and-criticals.md](../../gtk4-libadwaita-internals/references/warnings-and-criticals.md)
- [../../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md)
- [../../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md](../../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md)
- [../../gtk4-libadwaita-internals/references/containers-lists-and-factories.md](../../gtk4-libadwaita-internals/references/containers-lists-and-factories.md)

## Geometry and Measurement

### `Gtk-WARNING **: Trying to measure GtkBox ... for width of X, but it needs at least Y`

- Meaning: GTK attempted a width measurement smaller than the widget tree's minimum width.
- Authoritative follow-up: open the internals geometry reference above before deciding whether the problem is clamping, handle width accounting, `GtkRevealer` transition rounding, or another measurement invariant.
- Common causes:
  - paned positions clamped too late
  - revealer or animation endpoints moving through illegal widths
  - stale handle-overhead or minimum-size calculations
  - hidden widgets still participating in layout longer than expected
- Debug focus:
  - `measure()` versus `size_allocate()` timing
  - min-content widths
  - animated paned or revealer transitions
  - off-by-one handle width budgets

## Lifecycle and Object Ownership

### `GLib-GObject-CRITICAL`

- Meaning: invalid object state, signal misuse, or property access after disposal.
- Authoritative follow-up: use the internals lifecycle reference above to separate disposal, parenting, and stale-reference issues.
- Common causes:
  - upgraded weak ref after the object died
  - signal handlers left connected after widget disposal
  - stale object references captured by async callbacks

### `Gtk-CRITICAL` or `Gdk-CRITICAL`

- Meaning: GTK or GDK invariant was violated.
- Authoritative follow-up: use the internals warning atlas or official source to identify the exact invariant instead of guessing from the class name alone.
- Common causes:
  - main-thread-only API called from the wrong thread
  - widget tree mutations during invalid lifecycle phases
  - incorrect event, focus, or surface assumptions

## Rust and GLib Bridge Signals

### `thread 'main' panicked`

- Meaning: Rust panic crossed into the GTK app.
- Debug focus:
  - panic site
  - `unwrap()` or `expect()` near UI events
  - callback boundaries where panic leaves inconsistent UI state

### `RUST_BACKTRACE=1`

- Use when the panic site is unclear or the warning seems downstream from an earlier panic.

## D-Bus Signals

### `org.gnome.Shell.Introspect.WindowsChanged`

- Meaning: GNOME Shell thinks the visible or focused window set changed.
- Useful for:
  - correlating focus, map, and workspace changes with GTK warnings
  - spotting visual churn during animations or dialog display

### Portal request and response traffic

- Meaning: the desktop is processing screenshot or screencast requests.
- Useful for:
  - explaining why a screenshot helper timed out
  - distinguishing “portal denied” from “helper never asked”

## Practical Reading Order

1. Repeated GTK or GLib warnings
2. Rust panics or errors
3. D-Bus bursts near the same timestamps
4. Journal messages from related services
