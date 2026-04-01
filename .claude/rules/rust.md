---
description: Rust coding conventions for lushtext
globs: "**/*.rs"
---

# Rust Conventions

## Crate Structure

- All dependencies declared in workspace root `[workspace.dependencies]`, consumed with `{ workspace = true }`.
- Every crate depends on `workspace-hack` for cargo-hakari.
- License header `// SPDX-License-Identifier: GPL-3.0-or-later` on every `.rs` file.

## GTK/GLib Imports

- Use `libadwaita` (not `adw`) in all imports — there is no crate alias.
- For `imp()` method: import `glib::subclass::prelude::ObjectSubclassIsExt`.
- For GtkSourceView methods on Buffer: import `sourceview5::prelude::*`.
- Prefer `gtk4::prelude::*` for general GTK widget methods.
- In `imp.rs` files: use `libadwaita::subclass::prelude::*` (re-exports the full chain).

## Version Alignment

All gtk-rs crates must be from the same release series. Current versions:
- `gtk4 = 0.11`, `gdk4 = 0.11`, `sourceview5 = 0.11`
- `libadwaita = 0.9`
- `glib = 0.22`, `gio = 0.22`, `pango = 0.22`
- `glib-build-tools = 0.22`

Mixing versions (e.g. glib 0.21 with gtk4 0.11) causes "multiple versions of crate glib" errors.

## GObject Subclassing

Every custom widget: `mod.rs` (public wrapper + `glib::wrapper!`) + `imp.rs` (private struct + trait impls).

- Register child widget types with `ensure_type()` in `class_init()`.
- Use `connect_notify_local()` (not `connect_notify()`) for signal closures capturing GTK widgets — GTK types are not `Send`.
- GTK initialization order: CSS/Display access requires GTK to be initialized. Load CSS in `startup()`, not before `app.run()`.

## Dark Mode

GtkSourceView has its own theming separate from GTK CSS. Always:
1. Query `libadwaita::StyleManager::is_dark()` to pick `"Adwaita"` vs `"Adwaita-dark"`.
2. Connect to `connect_dark_notify()` for runtime changes.

## Background I/O

Use `services::async_task::spawn_blocking_then(state, work, then)` for any I/O that may block:
- `state`: non-Send GTK object (auto-wrapped in `ThreadGuard`)
- `work`: runs on background thread, must be `Send`
- `then`: runs on main thread with result, does NOT need to be `Send`

Never pass GTK objects directly across threads — they are not `Send`/`Sync`. Use `glib::thread_guard::ThreadGuard` or `glib::SendWeakRef`.

Never set `autoexpand = true` on `GtkTreeListModel`.

## Error Handling

- Services return `anyhow::Result`.
- For file I/O: try the operation and handle errors (no TOCTOU `exists()` checks).
- In GTK signal handlers: log errors with `tracing::error!`, don't panic.

## Testing

- Unit tests: `#[cfg(test)]` inside service modules, no GTK dependency.
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split pattern.
- Use `TestContext` for filesystem isolation (tempdir + simulated XDG dirs).
- Run `cargo hakari generate` after adding/removing dependencies.
