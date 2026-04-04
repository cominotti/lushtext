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

## Mutable State on GObject Structs

- Use `Cell<T>` for `Copy` types (e.g., `Cell<Option<u64>>`, `Cell<u32>`). No borrow overhead, no panic risk from overlapping borrows.
- Use `RefCell<T>` for non-`Copy` types (e.g., `RefCell<Option<PathBuf>>`, `RefCell<Option<String>>`).
- Both default correctly via `#[derive(Default)]` on the imp struct (`Cell<Option<T>>` defaults to `Cell::new(None)`).

## Error Handling

- Services return `anyhow::Result`.
- For file I/O: try the operation and handle errors (no TOCTOU `exists()` checks).
- In GTK signal handlers: log errors with `tracing::error!`, don't panic.

## File Size Limit

**Hard limit: 1000 lines of production code per `.rs` file.** `#[cfg(test)]` modules are excluded from this count — co-located tests are encouraged and should not trigger file splits.

When production code approaches 1000 lines:
1. **Split by responsibility.** Extract cohesive groups of functions into new modules. For UI widgets, the `mod.rs` / `imp.rs` split already helps — if `mod.rs` grows, extract helpers (e.g., `actions.rs`, `dialogs.rs`). For services, split by sub-domain.
2. **Never split mid-impl block.** Keep all trait impls for a type in one file. Split by extracting private helper functions into sibling modules, then calling them from the main impl.
3. **Prefer vertical, not horizontal splitting.** A 900-line file with one clear responsibility is better than 3 files that constantly cross-reference each other.

## Testing

- Unit tests: `#[cfg(test)]` inside service modules, no GTK dependency.
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split pattern.
- Widget tests: `crates/lushtext/tests/widget.rs` with `#[path]` split pattern; require display server.
- Use `TestContext` for filesystem isolation (tempdir + simulated XDG dirs).
- Run `cargo hakari generate` after adding/removing dependencies.
