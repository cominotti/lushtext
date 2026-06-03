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

The shared runner and widget harness default `GSK_RENDERER` to `cairo`. This
keeps CI on GTK's CPU fallback renderer so a headless Fedora container does not
emit Mesa/EGL GPU-probe warnings just because no render device is available.
Override `GSK_RENDERER` explicitly only when the test run is meant to chase a
renderer-specific GTK bug.

## Waiting for Async UI State

Many widget changes land through idle callbacks, timeouts, or `spawn_blocking_then`. Use the **shared** wait helpers from `crates/lushtext/tests/widget/common.rs` — do not paste a private copy into your module:

```rust
use crate::common::{flush_events, wait_until};

// `wait_until` polls, then drains ALL ready main-loop sources. The drain is
// required: `spawn_blocking_then` delivers completion via a low-priority
// idle_add_once source, which only runs once nothing higher-priority is pending.
wait_until(Duration::from_secs(5), || window.imp().sidebar.section_count() == 1);
```

Why shared, not local: there were once five copy-pasted `wait_until`s, so fixing one missed the rest. The canonical version lives once in `common.rs`; everything imports it. Do **not** rewrite it as a single blocking `MainContext::iteration(true)` with a timeout source — the timeout (higher priority) starves the idle completion and every `spawn_blocking_then`-backed wait then times out (verified failing 5/5 in isolation). The flake to fix is the *budget*, not the poll mechanism.

**Budget the wait to what it waits on.** Window realization and `spawn_blocking_then`/file-I/O completion are async and scheduling-dependent — give them a generous budget (≥5–10s). A 2s budget for async work flakes under load. The predicate returns the moment the work lands, so a larger ceiling never slows the fast path and only matters when a loaded machine delays the thread. Keep short budgets for synchronous UI-state flips.

If a widget test fails then passes (or the harness prints `FLAKY:`), that is a blocker, not noise: read the real panic, classify the wait, fix the cause, and rerun to confirm. See the skill's Flake Discipline section.

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

## `AdwDialog` Geometry Assertions

`AdwDialog` can be tricky to test geometrically because the dialog widget itself
is an overlay participant inside the parent window, while the user-visible card
or sheet is usually the dialog's child content.

For LushText widget tests:

- use `dialog.content_width()` / `dialog.content_height()` to assert the
  configured target size
- use the dialog child widget's `width()` / `height()` to assert the rendered
  floating surface the user actually sees
- do **not** assume `dialog.width()` / `dialog.height()` matches the visible
  card size

This matters for regressions where the dialog looks unchanged on screen even
though the `content-width` property was updated.

## Edit/Render Dialog Geometry Regressions

Dialogs with `GtkStack` Edit/Render modes need tests for the first activation, not only repeated toggles. The first click can perform extra work such as rendering Markdown, replacing an `AdwStatusPage` placeholder, or mounting a scrolled text surface; that is exactly when small dialog-size regressions appear.

For this class of test:

- compare the content widget's `measure()` natural sizes before and after the first Render activation
- use `dialog.content_width()` / `content_height()` or the dialog child's measured size instead of `dialog.width()` / `height()`
- assert inner text-surface margins before and after mode switches so Edit and Render do not drift
- assert that the Render page has already mounted its final scrolled text surface before first activation when the implementation supports that contract
- cover existing non-empty content and initially-empty dialogs where the user types text before first Render; those paths fail in different ways

If allocation is unavailable or unstable in the harness, measuring the dialog child is still useful. The important invariant is that the first Edit -> Render switch does not change the natural size contract that the parent dialog follows.

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
