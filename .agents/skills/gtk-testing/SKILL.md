---
name: gtk-testing
description: "Guide testing strategy for GTK4/Libadwaita Rust applications, especially LushText's current unit, integration, and custom widget-test harness. Use when adding or fixing tests, deciding the right test level, debugging headless GTK failures, modifying `crates/lushtext/tests/**`, or reasoning about widget visibility, animation, focus, and lifecycle assertions in tests. Trigger on mutter headless, widget harness, `TestContext`, flaky GTK tests, CI test configuration, or any request for GTK test coverage."
---

Guide testing for LushText as it exists today. The repo already has solid coverage layers and a custom GTK widget harness; use them before inventing new structure.

The testing approach is pragmatic:

- Keep pure logic in unit and service tests where no display server is needed.
- Use the existing integration helpers for cross-service filesystem workflows.
- Use the custom widget harness for real window and widget behavior.
- Reach for a new compositor-level or E2E harness only when the current widget target cannot express the behavior.

## Current Test Map

| Level | What | Location | Display | Command |
|-------|------|----------|---------|---------|
| Unit | Domain invariants and small pure helpers | crate-local `#[cfg(test)]` modules | No | `make test-unit` |
| Service | Filesystem and persistence logic in lib tests | crate-local `#[cfg(test)]` modules | No | `make test-unit` |
| Integration | Cross-service workflows with real temp directories | `crates/lushtext/tests/integration.rs` and `crates/lushtext/tests/integration/*.rs` | No | `make test-int` |
| Widget | Widget and real-window behavior, including workflow-level UI regressions | `crates/lushtext/tests/widget.rs` and `crates/lushtext/tests/widget/*.rs` | Yes | `make test-widget` |

There is **no standalone `make test-e2e` target today**. Many user workflows already belong in the widget harness, especially when they can be expressed by constructing a real `LushtextWindow` and waiting for observable state changes.

## Boundary with gtk4-libadwaita-internals

Use this skill to choose the test level, structure assertions, and run the harness.

Use `gtk4-libadwaita-internals` when the tricky part is the GTK contract behind the assertion:

- `WidgetExt::is_visible()` versus the widget's own `visible` property
- `GtkRevealer`, `GtkPaned`, or layout-measurement behavior in unpresented windows
- `GtkSignalListItemFactory` lifecycle, row reuse, or `GtkTreeListModel` behavior
- focus, actions, builder templates, CSS nodes, or accessibility-tree assumptions

If a test is flaky because the assertion is built on the wrong GTK mental model, fix that model first.

## Repo-Specific Harness Facts

- `crates/lushtext/tests/widget.rs` is a custom single-threaded harness. GTK widgets stay on one stable thread, and each test runs in its own child process.
- `build.rs` generates the widget registry from `crates/lushtext/tests/widget/*.rs`.
- `make test` may use `cargo nextest` for non-widget tests across the workspace, but widget tests still run through `scripts/run-widget-tests.sh`, which owns the native and headless `cargo test --test widget` paths.
- The widget harness supports `--list --format terse`, which matters for CI and nextest-style discovery.

## Default Workflow

1. Pick the lowest level that can prove the behavior.
2. Reuse the existing helpers:
   - `crates/lushtext/tests/integration/common.rs` for `TestContext`
   - `crates/lushtext/tests/widget/common.rs` for `ensure_gtk_init()`, `test_application()`, and `test_window()`
3. For widget tests, prefer small local helpers like `flush_events()` or `wait_until(...)` over fixed sleeps.
4. If the code under test uses async GTK callbacks or `spawn_blocking_then`, wait on a visible predicate with a timeout.
5. If the behavior depends on real frame-clock progression, do **not** just sleep for the nominal animation duration. Assert on stable invariants or use the repo's narrow test-only knobs when they already exist.
6. Run the smallest relevant command before broadening to the full suite.
7. When refactoring a large widget or window into sibling modules without changing behavior, still run the widget target for that surface and its adjacent orchestration surface. Visibility and wiring regressions often show up only from the external `crates/lushtext` test crate, not from `lushtext-core` alone.

## Headless GTK Runs

Prefer `make test-widget-headless` or `scripts/run-widget-tests.sh --headless` over hand-copying the mutter command. The shared runner owns the CI path, including retries for transient compositor failures.

The underlying headless invocation is:

```bash
export XDG_RUNTIME_DIR="$(mktemp -d)"
export GDK_BACKEND=wayland
dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 2560x1600 -- \
    cargo test --test widget
```

Use `make test-widget` locally when a display server is already available.

## Assertion Rules That Save Time

- For parented widgets inside an unpresented window, prefer `widget.property::<bool>("visible")` when you need the widget's own visibility flag. `is_visible()` answers a different question and walks the parent chain.
- For `GtkListView` keyboard shortcuts, do not assume the focused widget is the list itself. In real sessions focus usually sits on a realized row descendant, so synthetic key delivery should target the focused widget and, if needed, walk up its parent chain until it reaches the ancestor that owns the `EventControllerKey`. Emitting the key directly on the list view can produce false-green tests for shortcuts that do nothing in the live app.
- For adaptive surfaces that animate, especially `AdwBottomSheet` used by the document-properties pane, do not close the animated surface as a cleanup step in headless widget tests unless the test is specifically proving the close animation. Assert the open/compact state, then explicitly destroy the presented test window and flush once. Closing the sheet right before process teardown can make Mutter report `gdk-frame-clock: layout continuously requested` and `Trying to snapshot LushtextWindow ... without a current allocation`, even when the production UI state is correct.
- Test behavior, not GTK implementation details. Avoid pixel assertions, CSS rendering expectations, or proving that GTK's own containers work.
- Keep widget tests narrowly scoped. A real window is fine; an enormous end-to-end script is usually not.
- If a failure only reproduces in a live desktop session with compositor or portal behavior, switch to `gtk-agentic-debugging`.

## What To Add After a Change

- New domain or model logic: unit tests.
- New service or persistence behavior: service or integration tests with `TestContext`.
- New widget state, signal wiring, or window orchestration: widget tests.
- Bug fix: add the lowest-level regression test that reproduces the bug reliably.
- Workflow that truly needs compositor behavior beyond the current widget harness: discuss whether a new dedicated target is justified before creating one.
- Large adapter refactor with no intended behavior change: rerun the widget suites for the touched widget plus neighboring window/sidebar/search orchestration so extracted helper visibility and callback wiring stay covered.

## References

- [references/widget-testing.md](references/widget-testing.md): current harness behavior, headless commands, wait helpers, and contract-sensitive assertions
- [references/test-recipes.md](references/test-recipes.md): concrete service, integration, and widget recipes aligned with the repo's current helpers
- [../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md): layout and visibility contracts that affect widget assertions
- [../gtk4-libadwaita-internals/references/containers-lists-and-factories.md](../gtk4-libadwaita-internals/references/containers-lists-and-factories.md): `GtkListView` and `GtkTreeListModel` lifecycle rules for test reasoning
- [../gtk4-libadwaita-internals/references/builder-templates-actions-css-accessibility.md](../gtk4-libadwaita-internals/references/builder-templates-actions-css-accessibility.md): focus, builder-template, CSS, and accessibility guidance for test assertions
