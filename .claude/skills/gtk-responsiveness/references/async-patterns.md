# Async Pattern Catalog for GTK4/Rust

Detailed code examples for every async pattern used in LushText.

## Table of Contents

1. [Fire-and-Forget Background Work](#1-fire-and-forget)
2. [Background Work with UI Update](#2-background-with-ui-update)
3. [Cancellable Background Work](#3-cancellable)
4. [Periodic Background Check](#4-periodic)
5. [Debounced User Input](#5-debounced)
6. [Sequential Async Pipeline](#6-pipeline)
7. [Multiple Concurrent Loads](#7-concurrent)

---

## 1. Fire-and-Forget Background Work {#1-fire-and-forget}

For operations where you don't need the result (e.g., auto-save, telemetry, cleanup):

```rust
std::thread::spawn(move || {
    if let Err(e) = save_session_data(&data_dir, &session) {
        tracing::error!("Auto-save failed: {e}");
    }
});
```

No `ThreadGuard` or `idle_add_once` needed — the result is logged, not displayed.

**When to use**: Writes that can fail silently (auto-save, cleanup). NOT for saves triggered by the user (they need success/error feedback).

## 2. Background Work with UI Update {#2-background-with-ui-update}

The standard `spawn_blocking_then` pattern. Use for any I/O that needs to update the UI afterward.

```rust
// Loading a file into the editor
let path = path.to_path_buf();
async_task::spawn_blocking_then(
    self.clone(),
    move || -> anyhow::Result<String> {
        Ok(std::fs::read_to_string(&path)?)
    },
    |editor, result| {
        match result {
            Ok(content) => {
                let buffer = editor.buffer();
                buffer.set_text(&content);
                buffer.set_modified(false);
            }
            Err(e) => {
                tracing::error!("Load failed: {e}");
                // TODO: show error in UI (toast or status bar)
            }
        }
    },
);
```

**When to use**: File loads, directory scans, any I/O that feeds into UI state.

**RAM note**: The content `String` lives on the background thread until `glib::idle_add_once` delivers it to the main thread. During the handoff, there are briefly two references to the same allocation (background closure + idle closure), but Rust's move semantics ensure only one owner at a time — no duplication. Peak memory = 1x file size (the String) + whatever GtkTextBuffer allocates during `set_text()`.

## 3. Cancellable Background Work {#3-cancellable}

For operations the user might want to cancel (e.g., loading a large file, then switching tabs):

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

let cancelled = Arc::new(AtomicBool::new(false));
let cancel_token = cancelled.clone();

// Store cancel_token so we can trigger it later
*self.imp().cancel_token.borrow_mut() = Some(cancel_token);

let path = path.to_path_buf();
async_task::spawn_blocking_then(
    self.clone(),
    move || -> anyhow::Result<Option<String>> {
        // Check cancellation before expensive work
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        // Check again after I/O completes
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        Ok(Some(content))
    },
    |editor, result| {
        match result {
            Ok(Some(content)) => editor.apply_loaded_content(&content),
            Ok(None) => {} // Cancelled — do nothing
            Err(e) => tracing::error!("Load failed: {e}"),
        }
    },
);
```

To cancel:
```rust
if let Some(token) = self.imp().cancel_token.borrow().as_ref() {
    token.store(true, Ordering::Relaxed);
}
```

**When to use**: Large file loads, operations that become stale when the user navigates away.

**RAM note**: When cancelled, the `Ok(None)` return drops the read content immediately on the background thread — the String is never transferred to the main thread. This is important for large files: cancelling a 100MB load reclaims ~100MB on the background thread instead of keeping it alive through the idle_add_once handoff. The chunked reading variant is even better — if cancelled mid-read, only the partially-read content (~file_size * progress) is allocated, not the full file.

## 4. Periodic Background Check {#4-periodic}

For polling operations (file change detection, auto-save):

```rust
use glib::ControlFlow;

let weak_window: glib::SendWeakRef<LushtextWindow> = window.downgrade().into();

// Auto-save every 30 seconds
glib::timeout_add_local(
    std::time::Duration::from_secs(30),
    move || {
        if let Some(window) = weak_window.upgrade() {
            window.auto_save_session();
            ControlFlow::Continue
        } else {
            ControlFlow::Break // Window destroyed, stop timer
        }
    },
);
```

**Key**: Use `SendWeakRef` so the timer doesn't keep the window alive. Return `ControlFlow::Break` when the widget is gone.

## 5. Debounced User Input {#5-debounced}

For search-as-you-type and filter operations:

```rust
use std::cell::Cell;
use std::rc::Rc;

fn setup_search_debounce(entry: &gtk4::SearchEntry, callback: impl Fn(&str) + 'static) {
    let pending_id: Rc<Cell<Option<glib::SourceId>>> = Rc::new(Cell::new(None));
    
    entry.connect_search_changed(move |entry| {
        // Cancel previous pending callback
        if let Some(id) = pending_id.take() {
            id.remove();
        }
        
        let query = entry.text().to_string();
        let pending = pending_id.clone();
        
        // Schedule callback after 150ms of inactivity
        let id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(150),
            move || {
                callback(&query);
                pending.set(None);
            },
        );
        pending_id.set(Some(id));
    });
}
```

**Tuning**: 100-200ms is typical for search. Shorter feels snappier but triggers more work. Longer feels sluggish.

## 6. Sequential Async Pipeline {#6-pipeline}

When you need to chain async operations (load config → process → save):

```rust
// Step 1: Load workspace config
let data_dir = data_dir.to_path_buf();
async_task::spawn_blocking_then(
    self.clone(),
    move || workspace_manager::load(&data_dir),
    |window, result| {
        match result {
            Ok(workspaces) => {
                // Step 2: Process on main thread (fast, uses GTK state)
                let active = workspaces.active_workspace.as_ref();
                let workspace_id = active.map(|id| id.clone());
                
                // Step 3: Load session (another background operation)
                if let Some(id) = workspace_id {
                    let data_dir = window.data_dir().to_path_buf();
                    async_task::spawn_blocking_then(
                        window.clone(),
                        move || session_service::load(&data_dir, &id),
                        |window, result| {
                            if let Ok(session) = result {
                                window.restore_session(&session);
                            }
                        },
                    );
                }
            }
            Err(e) => tracing::error!("Failed to load workspaces: {e}"),
        }
    },
);
```

**Note**: Nested `spawn_blocking_then` calls are fine for 2-3 steps. For longer chains, consider combining the I/O into a single background operation that returns a tuple of results.

## 7. Multiple Concurrent Loads {#7-concurrent}

When you need to load several things in parallel (e.g., restoring multiple tabs):

```rust
// Each tab restoration is independent — spawn all at once
for tab in session.tabs.iter() {
    let path = tab.path.clone();
    let cursor_line = tab.cursor_line;
    
    // Create the page immediately (shows loading state)
    let editor = LushtextEditorPage::new();
    let page = self.imp().tab_view.append(&editor);
    page.set_title(&path.file_name().unwrap_or_default().to_string_lossy());
    
    // Load content in parallel
    async_task::spawn_blocking_then(
        editor.clone(),
        move || std::fs::read_to_string(&path),
        move |editor, result| {
            if let Ok(content) = result {
                editor.apply_loaded_content(&content);
                // Restore cursor position after content is loaded
                let buffer = editor.buffer();
                let iter = buffer.iter_at_line(cursor_line as i32);
                if let Ok(iter) = iter {
                    buffer.place_cursor(&iter);
                }
            }
        },
    );
}
```

**Key**: Each `spawn_blocking_then` spawns its own `std::thread`, but the global concurrency guard (`MAX_CONCURRENT_SPAWNS = 8`) automatically defers excess spawns via `timeout_add_local_once(50ms)`. No manual batching is needed — the guard caps peak thread memory at 8 * max_file_size.

**RAM note**: With the spawn guard, at most 8 file reads are concurrent. For session restore with 50 tabs averaging 500KB each, peak memory is ~4MB (8 * 500KB), not 25MB. Excess spawns queue up and execute as threads complete.

---

## Thread Safety Quick Reference

| Type | Send? | Sync? | Cross-thread pattern |
|------|-------|-------|---------------------|
| All GTK4 widgets | No | No | `ThreadGuard` or `SendWeakRef` |
| `gio::ListStore` | No | No | `ThreadGuard` |
| `glib::Object` subclasses | No | No | `ThreadGuard` or `SendWeakRef` |
| `sourceview5::Buffer` | No | No | `ThreadGuard` |
| `PathBuf`, `String`, `Vec<T>` | Yes | Yes | Direct move into closure |
| `Arc<AtomicBool>` | Yes | Yes | Direct clone + move |
| Domain types (`WorkspacesFile`, etc.) | Yes | Yes | Direct move (they're plain Rust structs) |

## GLib Main Loop Scheduling Primitives

| Function | Thread | Repeating? | Use case |
|----------|--------|------------|----------|
| `glib::idle_add_once(closure)` | Main | Once | Background → UI result delivery |
| `glib::idle_add_local_once(closure)` | Main | Once | Same, but closure doesn't need Send |
| `glib::timeout_add_local_once(dur, closure)` | Main | Once | Debouncing, delayed actions |
| `glib::timeout_add_local(dur, closure)` | Main | Repeating | Periodic checks, auto-save |
| `glib::idle_add(closure)` | Main | Until Break | Incremental work (use sparingly) |
