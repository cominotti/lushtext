---
project_name: 'lushtext'
user_name: 'Danilo'
date: '2026-04-04'
sections_completed: ['technology_stack', 'language_rules', 'framework_rules', 'testing_rules', 'code_quality', 'workflow_rules', 'critical_rules']
status: 'complete'
rule_count: 79
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss._

---

## Technology Stack & Versions

- **Rust** MSRV 1.94.1, Edition 2024
- **GTK4** 0.11 (`v4_20`), **Libadwaita** 0.9 (`v1_8`), **GtkSourceView 5** 0.11 (`v5_16`)
- **GLib/GIO/Pango** 0.22, **GDK4** 0.11
- **Serialization:** serde 1.0 + serde_json 1.0
- **Errors:** anyhow 1.0 (services), thiserror 2.0 (user-facing typed errors)
- **SIMD:** nucleo-matcher 0.3 (fuzzy search), simdutf8 0.1 (UTF-8 validation)
- **EditorConfig:** editorconfig-parser 0.0.3
- **Logging:** tracing 0.1
- **Benchmarks:** Criterion 0.5
- **Build:** Cargo workspace (2 crates + workspace-hack) + Makefile (dev) + Meson (Flatpak/installed)
- **Linker:** rust-lld (Rust 1.90+ default on x86_64-linux)
- **Test runner:** cargo-nextest (auto-detected)
- **Dep unification:** cargo-hakari (workspace-hack crate)

**CRITICAL version constraint:** All gtk-rs crates must be from the same release series (0.11/0.22 cycle). Mixing versions (e.g., glib 0.21 with gtk4 0.11) causes "multiple versions of crate glib" compile errors.

---

## Critical Implementation Rules

### Rust Language Rules

- **SPDX header required:** Every `.rs` file starts with `// SPDX-License-Identifier: GPL-3.0-or-later` as the first line, before module docstrings.
- **Workspace dependencies:** All deps declared in root `[workspace.dependencies]`, consumed with `{ workspace = true }` in crate `Cargo.toml`. Every crate depends on `workspace-hack`.
- **Import ordering:** (1) `mod` declarations, (2) `pub use` re-exports, (3) `use crate::...` grouped by layer, (4) external crates alphabetically, (5) `std` last. No `use super::*` or `extern crate`.
- **GObject mutable state:** `Cell<T>` for `Copy` types (no borrow overhead), `RefCell<T>` for non-`Copy` types. Both default correctly via `#[derive(Default)]`.
- **Error handling split:** `thiserror` enums for user-facing I/O errors (e.g., `LoadError`, `SaveError`); `anyhow::Result` for internal persistence. In GTK signal handlers: `tracing::error!()`, never panic.
- **No GTK deps in model layer:** `src/model/` contains pure domain types with no GTK/GLib imports.
- **Module re-exports:** Flat `pub mod` in layer boundaries. No `pub use` flattening — callers write full paths like `crate::services::json_store::load(...)`.
- **File size limit:** Hard limit of 1000 lines of production code per `.rs` file. `#[cfg(test)]` modules excluded.
- **Import aliases:** Use `libadwaita` (not `adw`). Use `gtk4::prelude::*` for general widget methods. In `imp.rs`: `libadwaita::subclass::prelude::*` re-exports the full trait chain.

### GTK4 / Libadwaita Framework Rules

**GObject Subclassing:**
- Every custom widget: `mod.rs` (public wrapper + `glib::wrapper!`) + `imp.rs` (private struct + trait impls).
- Required trait chain for AdwApplicationWindow: `ObjectSubclass` -> `ObjectImpl` -> `WidgetImpl` -> `WindowImpl` -> `ApplicationWindowImpl` -> `AdwApplicationWindowImpl`.
- Register child widget types via `ensure_type()` in `class_init()` BEFORE `klass.bind_template()`.
- Signal disconnect/cleanup in `ObjectImpl::dispose()`, not Rust `Drop` — GTK4's `dispose()` runs first and clears `TemplateChild` fields.

**Threading Model (CRITICAL):**
- GTK objects are NOT `Send`/`Sync` (raw pointers inside). Never pass across threads.
- Use `spawn_blocking_then(state, work, then)` for all blocking I/O: `state` is GTK object (auto-wrapped in `ThreadGuard`), `work` runs on background thread, `then` runs on main thread.
- Concurrency limited to 8 threads via `AtomicUsize` guard. Overflow deferred via `timeout_add_local_once(50ms)`.
- Use `connect_notify_local()` (not `connect_notify()`) for closures capturing GTK widgets.

**Widget Wiring:**
- Never set `autoexpand = true` on `GtkTreeListModel` — spawns unbounded threads or freezes UI.
- `GtkTreeExpander` installs an internal `GtkGestureClick` that intercepts ALL rows. For file rows: disable via `observe_controllers()` + `propagation_phase = None` in `connect_bind`.
- Use `size_allocate()` override (not `notify::default-width`) for size-dependent constraints — property notifications fire BEFORE the new allocation is applied.
- Focus restoration after overlay close: explicitly save/restore focus. GTK4's default traversal after `set_reveal_child(false)` walks to first focusable widget (usually sidebar button).

**GSettings:**
- `gio::Settings::bind()` for direct property bindings. Manual `connect_changed()` for type conversions (e.g., bool -> WrapMode).
- `EditorPage` stores `SignalHandlerId` for GSettings handlers and disconnects in `Drop` — without this, handlers accumulate on tab cycles.

**CSS & Theming:**
- CSS loading must happen in `startup()` callback, not before `app.run()`.
- Use Adwaita CSS custom properties (`@window_bg_color`, `@headerbar_bg_color`, etc.) — no hardcoded colors.
- GtkSourceView has its own style scheme system separate from GTK CSS. Query `StyleManager::is_dark()` and connect `connect_dark_notify()` for runtime switching.

### Testing Rules

**Three-tier test structure:**
- **Unit tests:** `#[cfg(test)]` modules co-located inside service files. No GTK dependency. Run via `make test-unit`.
- **Integration tests:** `crates/lushtext/tests/integration.rs` with `#[path]` split binary pattern. Uses `TestContext` (tempdir + simulated XDG dirs). Run via `make test-int`.
- **Widget tests:** `crates/lushtext/tests/widget.rs` with `#[path]` split pattern. Require display server (`xvfb-run make test-widget`). Use `GSETTINGS_BACKEND=memory` for isolation.

**TestContext isolation:** Wraps `TempDir` with `path()`, `data_dir()`, `write_file()`, `mkdir()` helpers. Every test gets its own isolated filesystem.

**Widget test initialization:** `ensure_gtk_init()` (via `std::sync::Once`) sets `GSETTINGS_BACKEND=memory`, `LUSHTEXT_DATA_DIR` to PID-named temp dir, calls `gtk4::init()`, `sourceview5::init()`, `lushtext_core::register_resources()`.

**Async result assertions:** Tests depending on `spawn_blocking_then` must use `spin_until(|| predicate())` to poll the main loop — `flush_events()` alone is insufficient because the background thread may not have posted its callback yet.

**Visibility checks:** `WidgetExt::is_visible()` checks the entire parent chain (returns `false` in headless tests). Use `widget.property::<bool>("visible")` for the widget's own visibility property.

**CI enforcement:** `cargo clippy --all-targets -- -D warnings` — CI fails on any clippy lint. All tests run under `xvfb-run`.

### Code Quality & Style Rules

**Naming conventions:**
- Rust source files: `snake_case.rs`
- GResource XML paths: `kebab-case` (GTK convention, e.g., `editor-page.ui`)
- CSS classes: `kebab-case` (e.g., `.search-bar-container`, `.status-bar`)
- GSettings keys: `kebab-case` (e.g., `insert-spaces-instead-of-tabs`), also exposed as `pub const` in `src/config.rs` under `pub mod keys { ... }`.

**Code organization:**
- Two-crate workspace: `lushtext-core` (all logic) + `lushtext` (thin binary + integration tests).
- Three-layer architecture: `model/` (pure domain types) -> `services/` (business logic, no GTK) -> `ui/` (GTK widgets).
- UI widgets are directories with `mod.rs` + `imp.rs`. Additional helpers extracted when needed (e.g., `window/dialogs.rs`).

**Async I/O patterns:**
- `spawn_blocking_then` for all blocking I/O with main-thread callback.
- Fire-and-forget `std::thread::spawn` only for tiny non-critical cleanup (temp file deletion).
- Atomic JSON writes via `json_store::save` (write-to-temp + `rename`).
- Generation-counter debounce for timed operations (status bar 5s, search 150ms, file index 300ms, file monitor 500ms, sidebar persist 150ms).

**Batch updates:**
- `gio::ListStore::splice()` for batch model updates (single `items-changed` signal) instead of per-item `append()` loops.

**Caps and guards:**
- Directory entry cap: 10,000 per directory with placeholder row on truncation.
- File index cap: 100,000 files with warning log.
- Concurrent thread cap: 8 via `MAX_CONCURRENT_SPAWNS`.
- Buffer memory budget: 256MB — evicts unmodified background tabs on tab switch.

### Development Workflow Rules

**Git conventions:**
- Conventional commits: `type(scope): short description` (types: feat, fix, refactor, perf, test, docs, chore; scopes: ui, sidebar, editor, workspace, session, build, flatpak).
- All commits must be signed (SSH signing). Never skip with `--no-gpg-sign` or `--no-verify`.
- Never force-push to `main`. Create new commits rather than amending unless explicitly asked.
- Stage specific files by name, not `git add -A` or `git add .`.

**Build workflow:**
- Use `make` targets for development (`make run`, `make test`, `make check`).
- Dependency addition chain: (1) root `[workspace.dependencies]`, (2) `{ workspace = true }` in crate, (3) `cargo hakari generate`, (4) `make cargo-sources` for Flatpak.
- `make check` = `clippy --all-targets -- -D warnings` + `cargo fmt --all -- --check`.

**Documentation maintenance:**
- `README.md` must always be in sync with code (features, build, architecture).
- `.claude/CLAUDE.md` updated when modules/patterns/design decisions change.
- `.claude/rules/*.md` updated when conventions are refined.

**Runtime warning policy:**
- GTK/pixman warnings are bugs, not noise. Development is not finished if any warnings appear.
- Before considering a UI change complete, run the app and exercise the feature while watching stderr.

**Pre-existing blocker policy (CRITICAL):**
- If implementation or verification reveals a pre-existing blocker, fix it in the same work stream instead of deferring around it or treating it as out of scope.
- Do not close work with known failing checks or broken test harnesses on the grounds that the failure was already present.
- This rule is mandatory and has no exceptions.

### Critical Don't-Miss Rules

**Anti-patterns to NEVER do:**
- NEVER pass GTK objects across threads — use `ThreadGuard` or `SendWeakRef`.
- NEVER set `autoexpand = true` on `GtkTreeListModel`.
- NEVER use deprecated `GtkTreeView` — use `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander`.
- NEVER load CSS before `app.run()` — display access requires GTK initialization in `startup()`.
- NEVER use `notify::default-width` / `notify::maximized` for size-dependent constraints — they fire before the new allocation is applied.
- NEVER use `idle_add_local_once` for deferred concurrency overflow — it busy-wait spins. Use `timeout_add_local_once(50ms)`.
- NEVER animate sidebar paned position to 0px — zero-width allocations trigger pixman warnings. Use 1px minimum.
- NEVER use `single-click-activate=true` on file tree ListViews — changes expected UX.

**Edge cases agents must handle:**
- `GtkTreeExpander`'s internal gesture intercepts clicks for file rows. Disable via `observe_controllers()` in `connect_bind`.
- `connect_bind` must clean up stale rename `GtkEntry` widgets from ListItem recycling.
- `build_children_model` must deduplicate items already in the store (prevents duplicates from async scan + new-file flow).
- Inline rename guard: check `entry.parent().is_none()` to prevent double-fire from focus-out after confirm/cancel.
- Window close: `Propagation::Stop` + deferred `window.destroy()` — not direct close.
- Session restore: `restoring_session` flag suppresses redundant session saves.
- Buffer eviction: set `evicted=true` BEFORE clearing text to prevent `modified-changed` signal flash.

**Performance gotchas:**
- Large files >10MB: use `simdutf8` for SIMD UTF-8 validation, skip redundant scalar validation.
- Large files >50MB: keep `begin_irreversible_action()` permanently open (no undo stack).
- Very large buffers >=10MB: snapshot in 64k-char main-loop slices with view read-only during save.
- `clamp_sidebar_position` fires ~60Hz during resize — cache `sidebar_visible` in `Cell<bool>` to avoid GObject property lookups.

---

## Usage Guidelines

**For AI Agents:**
- Read this file before implementing any code
- Follow ALL rules exactly as documented
- When in doubt, prefer the more restrictive option
- Cross-reference with `.claude/CLAUDE.md` for architectural details and `.claude/rules/*.md` for full conventions

**For Humans:**
- Keep this file lean and focused on agent needs
- Update when technology stack or patterns change
- Review periodically for outdated rules
- Remove rules that become obvious over time

Last Updated: 2026-04-04
