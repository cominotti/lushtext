# Widget Testing Patterns for GTK4 and Rust

Use this reference for the current LushText widget harness and the failure modes that make GTK widget tests confusing.

## Current Harness

Widget tests live under:

- `crates/lushtext/tests/widget.rs`
- `crates/lushtext/tests/widget/*.rs`
- `crates/lushtext/tests/widget/common.rs`

Important repo facts:

- `crates/lushtext/tests/widget.rs` is a custom single-threaded harness, not the default libtest runner.
- Each widget test runs in its own child process via `LUSHTEXT_WIDGET_CHILD`.
- The harness supports `--list --format terse`, which matters for nextest-style discovery.
- `crates/lushtext/tests/widget/common.rs` already sets up GTK, GResources, in-memory GSettings, isolated data dirs, and unique application IDs.

Prefer the existing helpers:

```rust
use crate::common::{ensure_gtk_init, test_application, test_window};
```

## Headless Runs

Prefer `mutter --headless` for GTK4 and Libadwaita:

Use `scripts/run-widget-tests.sh --headless` or `make test-widget-headless` when possible so CI and local repro share the same wrapper, retry policy, and monitor size.

```bash
export XDG_RUNTIME_DIR="$(mktemp -d)"
export GDK_BACKEND=wayland
dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 2560x1600 -- \
    cargo test --test widget
```

Use `make test-widget` when a display server is already available.

## Waiting for Async UI State

Many widget changes land through idle callbacks, timeouts, or `spawn_blocking_then`. Use a local wait helper with a timeout instead of hard-coded sleeps:

```rust
use std::time::{Duration, Instant};

fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        flush_events();
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(predicate(), "timed out waiting for widget state");
}
```

Use predicates tied to visible behavior:

- tab count changed
- selected page changed
- a label or title updated
- a `Cell` flag in the widget `imp()` flipped

## Visibility and Realization Traps

The most common bad assertion in GTK widget tests is using `is_visible()` when you really mean the widget's own `visible` property.

In an unpresented window:

- `widget.is_visible()` walks the parent chain and often returns `false`
- `widget.property::<bool>("visible")` reads the widget's own flag

So prefer:

```rust
assert!(widget.property::<bool>("visible"));
```

If the question becomes "what does GTK consider visible, mapped, realized, or accessible here?", stop and read:

- [../../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md](../../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md)
- [../../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md)

## Fixed Row Chrome Regressions

When a bug is really "the hover pill or active highlight of this button no longer fits inside its fixed row", prefer a property-level widget regression over screenshot-only verification.

Good assertions for this class of bug:

- the row still keeps its expected fixed height
- the button uses `valign == gtk4::Align::Center`
- the button carries the expected `margin_top` and `margin_bottom`

This is especially important for flat icon buttons living beside a dropdown or label in a fixed top row. If the bug is the button chrome itself, do not lock in a wrong fix by asserting extra margin on the list or scroller below.

## Animations and Frame Clocks

The custom harness does not guarantee that spinning the GLib main loop advances `AdwTimedAnimation` the way a live desktop session does.

Rules:

- Do **not** sleep for `150ms` or `250ms` and assume the animation finished.
- Prefer assertions on stable pre/post invariants.
- If the code already exposes a narrow test-only knob for animation completion, use it.
- If the behavior only reproduces with real frame-clock progression, run under `mutter --headless` or switch to `gtk-agentic-debugging`.

## ListView, TreeListModel, and Row Reuse

When testing list or tree widgets:

- assert on model or selection state, not on assumptions about fresh row instances
- remember rows are recycled
- keep factory lifecycle questions separate from app-level expectations

If the confusing part is `connect_setup`, `connect_bind`, `connect_unbind`, row reuse, or `GtkTreeListModel` behavior, read:

- [../../gtk4-libadwaita-internals/references/containers-lists-and-factories.md](../../gtk4-libadwaita-internals/references/containers-lists-and-factories.md)

## When Widget Tests Are the Wrong Tool

Switch tools when:

- the bug depends on portal behavior, compositor state, or desktop focus handoff
- you need live stderr or journal evidence
- the failure only reproduces in a running app session

That is a `gtk-agentic-debugging` problem, not a better assertion problem.
