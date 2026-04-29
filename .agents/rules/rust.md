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
- For GtkSourceView extension methods on Buffer, View, annotation providers, annotations, and other source-view objects: import `sourceview5::prelude::*`.
- Prefer `gtk4::prelude::*` for general GTK widget methods.
- In `imp.rs` files: use `libadwaita::subclass::prelude::*` (re-exports the full chain).

## Version Alignment

All gtk-rs crates must be from the same release series. Current versions:
- `gtk4 = 0.11`, `gdk4 = 0.11`, `sourceview5 = 0.11`
- `libadwaita = 0.9`
- `glib = 0.22`, `gio = 0.22`, `pango = 0.22`
- `glib-build-tools = 0.22`
- Platform feature floor: GNOME 50 (`gtk4` `gnome_50` / GTK 4.22, Libadwaita 1.9, GLib/GIO 2.88, GtkSourceView feature `v5_18`).

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

When save-time policy rewrites buffer text (for example EditorConfig
`trim_trailing_whitespace` or `insert_final_newline`), the saved bytes and live
buffer must agree before the buffer is marked clean. Either mirror the saved
text back into the buffer after a successful write or keep the buffer modified;
do not show a clean tab whose visible text differs from disk.

## Mutable State on GObject Structs

- Use `Cell<T>` for `Copy` types (e.g., `Cell<Option<u64>>`, `Cell<u32>`). No borrow overhead, no panic risk from overlapping borrows.
- Use `RefCell<T>` for non-`Copy` types (e.g., `RefCell<Option<PathBuf>>`, `RefCell<Option<String>>`).
- Both default correctly via `#[derive(Default)]` on the imp struct (`Cell<Option<T>>` defaults to `Cell::new(None)`).

## Lint Suppression

- Prefer `#[expect(lint)]` over `#[allow(lint)]` when suppressing a lint for a known reason (e.g., using a deprecated API that has no replacement yet). `#[expect]` is self-policing: it causes a compile error if the lint no longer fires, so stale suppressions are caught automatically.
- Reserve `#[allow(lint)]` only for cases where the lint may or may not fire depending on configuration or feature flags.

## Modern Rust Idioms

Use `if let` chains with `&&` / `&& let` (Edition 2024) as the preferred pattern for multi-condition guards that combine option unwrapping with boolean checks:

```rust
// Preferred: if-let chain
if let Some(obj) = obj_weak.upgrade()
    && let Some(ref cb) = *obj.imp().callback.borrow()
{
    cb();
}

// Avoid: nested if-let
if let Some(obj) = obj_weak.upgrade() {
    if let Some(ref cb) = *obj.imp().callback.borrow() {
        cb();
    }
}
```

With Rust 1.95.0, also use `if let` match guards when a match arm needs both the matched value and a short fallible guard:

```rust
match encoding {
    DocumentEncoding::Utf8 if let Ok(text) = simdutf8::basic::from_utf8(bytes) => {
        text.to_string()
    }
    _ => fallback_decode(bytes),
}
```

Prefer the new standard-library helpers when they make intent clearer:

- Use `slice.array_windows::<N>()` over `slice.windows(N)` when the window width is fixed and every element is indexed.
- Use `Atomic*::try_update` / `Atomic*::update` instead of hand-written compare-exchange loops when the closure expresses the complete state transition clearly.
- Use `Peekable::next_if_eq`, `next_if`, `next_if_map`, or `next_if_map_mut` instead of manual `peek()` + `next()` pairs.
- Use `Vec::push_mut` / `insert_mut` only when the caller immediately needs a mutable reference to the inserted value; plain `push` remains clearer otherwise.
- Use `cfg_select!` for expression-level or tightly grouped cfg choices. Keep item-level `#[cfg]` when separate Unix/non-Unix implementations are already clearer.

## Error Handling

- Services return `anyhow::Result`.
- For file I/O: try the operation and handle errors (no TOCTOU `exists()` checks).
- In GTK signal handlers: log errors with `tracing::error!`, don't panic.

## File Size Limit

**Target limit: keep production `.rs` files under roughly 1000 lines when practical.** `#[cfg(test)]` modules are excluded from this count — co-located tests are encouraged and should not trigger file splits.

Existing over-limit files are accepted refactor debt. When you touch one substantially, prefer extracting helpers or sibling modules that reduce responsibility count and local complexity instead of letting the file grow unchecked.

When production code approaches 1000 lines:
1. **Split by responsibility.** Extract cohesive groups of functions into new modules. For UI widgets, the `mod.rs` / `imp.rs` split already helps — if `mod.rs` grows, extract helpers (e.g., `actions.rs`, `dialogs.rs`). For services, split by sub-domain.
2. **Never split mid-impl block.** Keep all trait impls for a type in one file. Split by extracting private helper functions into sibling modules, then calling them from the main impl.
3. **Prefer vertical, not horizontal splitting.** A 900-line file with one clear responsibility is better than 3 files that constantly cross-reference each other.
4. **Split GTK adapters by workflow before inventing new abstraction layers.** If a widget starts mixing unrelated flows (actions, notifications, persistence, search runtime, focus recovery), prefer sibling modules under the widget folder over new traits or faux-manager types.
5. **Promote repeated field bundles into named value objects or state groupings.** If multiple call sites rebuild the same shape (for example query text + toggle state), move it into `model/`. If an `imp` struct accumulates unrelated timers/counters/maps, group them into small helper structs with clear workflow ownership.

## Testing

- Unit tests: `#[cfg(test)]` inside service modules, no GTK dependency.
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split pattern.
- Widget tests: `crates/lushtext/tests/widget.rs` with `#[path]` split pattern; require display server.
- Use `TestContext` for filesystem isolation (tempdir + simulated XDG dirs).
- Run `cargo hakari generate` after adding/removing dependencies.
