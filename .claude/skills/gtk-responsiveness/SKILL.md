---
name: gtk-responsiveness
description: "Guide and review Rust code for GTK4/Libadwaita responsiveness and performance. Auto-invoked when writing or modifying UI code, async patterns, signal handlers, file I/O, TreeListModel usage, or any code that could block the main thread. Use whenever the user writes code in ui/, touches spawn_blocking_then or async_task, works with GLib signals, handles file operations, implements ListView/TreeListModel patterns, or discusses performance, responsiveness, threading, or 'app not responding' issues. Also trigger when reviewing UI code in pull requests."
---

Guide and review Rust code for keeping LushText buttery smooth — no "Waiting for application to respond" dialogs, no UI freezes, no janky scrolling. The GTK main loop runs on a single thread; any blocking call on that thread freezes the entire UI. This skill ensures every I/O operation, heavy computation, and signal handler follows patterns that keep the main loop free.

## The Golden Rule

> **Never block the GTK main thread.**

The main thread runs the GLib main loop, which processes:
- User input events (keyboard, mouse, touch)
- Widget drawing and layout
- Signal emissions and handler dispatch
- Timer callbacks and idle handlers
- D-Bus messages

If your code takes >16ms on the main thread (~60fps frame budget), the UI stutters. If it takes >5 seconds, the desktop environment shows "Application Not Responding." There is no exception to this rule.

## Decision Matrix: Sync vs Async

| Operation | Time | Pattern | Where |
|-----------|------|---------|-------|
| Read small config file (<1KB) | <1ms | Sync on main thread | OK in `constructed()` or startup |
| Read user file (any size) | Variable | `spawn_blocking_then` | Always async |
| Write/save file | Variable | `spawn_blocking_then` | Always async |
| Scan directory listing | Variable | `spawn_blocking_then` | Always async (already done in `file_tree.rs`) |
| JSON parse small config | <1ms | Sync after async read | Parse in `then` callback |
| JSON parse large file | >10ms | Parse in background | Parse in `work` closure |
| Syntax highlighting | Handled by GtkSourceView | N/A | Natural port — don't reimplement |
| Search/replace in buffer | Handled by GtkSourceView | N/A | Use `SearchContext` API |
| Tree model population | Per-directory | `spawn_blocking_then` | Return empty store, populate async |

### The 1ms Rule of Thumb

If an operation can exceed 1ms in the worst case (large file, slow disk, network mount, many directory entries), it must run off the main thread. When in doubt, make it async — the overhead of `spawn_blocking_then` is negligible compared to a UI freeze.

## Pattern 1: `spawn_blocking_then` — The Primary Async Pattern

This project uses a custom async primitive instead of Tokio/async-std. It bridges background threads with the GLib main loop:

```rust
use crate::services::async_task;

// state: non-Send GTK object (auto-wrapped in ThreadGuard)
// work: runs on background thread (must be Send)
// then: runs on main thread with the result
async_task::spawn_blocking_then(
    self.clone(),           // state: the widget that needs updating
    move || {               // work: blocking I/O on background thread
        std::fs::read_to_string(&path)
    },
    |editor, result| {     // then: update UI on main thread
        match result {
            Ok(content) => editor.apply_loaded_content(&content),
            Err(e) => tracing::error!("Failed to load: {e}"),
        }
    },
);
```

### Why Not Tokio?

Tokio is designed for managing hundreds of concurrent network connections. LushText does occasional file I/O on a single machine. The overhead of an async runtime (executor, task scheduler, I/O driver) adds complexity without benefit. The `std::thread::spawn` + `glib::idle_add_once` pattern is:
- Simpler (30 lines of code vs a runtime dependency)
- Sufficient (file I/O is the only blocking operation)
- GTK-native (uses GLib's own main loop scheduling)

Only recommend Tokio if the app needs to manage concurrent network connections (e.g., cloud sync, LSP client with multiple servers).

### Common Mistakes

**Mistake 1: Forgetting to clone the path before the closure**
```rust
// BAD: path is borrowed, can't move into Send closure
async_task::spawn_blocking_then(
    self.clone(),
    || std::fs::read_to_string(&path),  // ERROR: path doesn't live long enough
    |editor, result| { ... },
);

// GOOD: clone/move the path
let path = path.to_path_buf();
async_task::spawn_blocking_then(
    self.clone(),
    move || std::fs::read_to_string(&path),
    |editor, result| { ... },
);
```

**Mistake 2: Doing heavy work in the `then` callback**
```rust
// BAD: parsing a large JSON on the main thread
async_task::spawn_blocking_then(
    self.clone(),
    move || std::fs::read_to_string(&path),
    |editor, content| {
        let data: LargeStruct = serde_json::from_str(&content.unwrap()).unwrap(); // BLOCKS
        editor.apply(data);
    },
);

// GOOD: parse in the background, deliver the result
async_task::spawn_blocking_then(
    self.clone(),
    move || -> Result<LargeStruct> {
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    },
    |editor, result| {
        match result {
            Ok(data) => editor.apply(data),
            Err(e) => tracing::error!("Failed: {e}"),
        }
    },
);
```

## Pattern 2: Thread Safety — `ThreadGuard` vs `SendWeakRef`

GTK objects contain raw pointers and are NOT `Send`/`Sync`. Two mechanisms exist to safely reference them across threads:

### `ThreadGuard` (used by `spawn_blocking_then`)

Wraps a non-Send value to make it `Send`. Panics if accessed on the wrong thread. Used when you need to pass the object through a thread boundary but only access it on the original thread.

```rust
let guard = glib::thread_guard::ThreadGuard::new(widget);
std::thread::spawn(move || {
    // guard is Send, but we don't access the widget here
    let result = do_work();
    glib::idle_add_once(move || {
        let widget = guard.into_inner(); // OK: back on main thread
        widget.update(result);
    });
});
```

### `SendWeakRef` (for long-lived references)

A weak reference that is `Send`. Use when you need a reference that outlives a single function call — e.g., storing a callback that fires later.

```rust
let weak: glib::SendWeakRef<LushtextWindow> = window.downgrade().into();
std::thread::spawn(move || {
    let result = do_work();
    glib::idle_add_once(move || {
        if let Some(window) = weak.upgrade() {
            window.update(result);
        }
        // If window was destroyed, silently skip — no panic
    });
});
```

### When to Use Which

| Situation | Use | Reason |
|-----------|-----|--------|
| `spawn_blocking_then` (short-lived) | `ThreadGuard` (automatic) | Object guaranteed to exist for the duration |
| Periodic background check | `SendWeakRef` | Widget may be destroyed between checks |
| Signal handler cleanup | `glib::signal::SignalHandlerId` + `disconnect()` | Prevent leaked handlers |
| Timer that updates UI | `SendWeakRef` in `glib::timeout_add_local` | Timer may outlive the widget |

## Pattern 3: TreeListModel Performance

`GtkTreeListModel` calls a `create_model_func` callback when expanding a node. This callback runs on the main thread.

### Critical Rule: NEVER Set `autoexpand = true`

```rust
// CATASTROPHIC: recursively calls create_model_func for EVERY directory
let tree_model = gtk4::TreeListModel::new(root_model, false, true, ...);
//                                                          ^^^^
//                                                    autoexpand = true

// CORRECT: manual expansion via user interaction only
let tree_model = gtk4::TreeListModel::new(root_model, false, false, ...);
```

With `autoexpand = true`, the tree tries to expand every node recursively. Combined with `spawn_blocking_then` for directory scanning, this spawns an unbounded number of threads (one per directory). With synchronous I/O, it freezes the UI traversing the entire filesystem.

### Lazy Population Pattern (Current Approach)

The current `file_tree.rs` pattern is correct:
1. `build_children_model` returns an **empty** `ListStore` immediately
2. `spawn_blocking_then` scans the directory on a background thread
3. The `then` callback appends `FileTreeItem` objects to the store
4. `TreeListModel` reacts to `items-changed` signal automatically

This keeps the main thread free during directory scanning.

## Pattern 4: ListView Factory Performance

`GtkListView` uses a `SignalListItemFactory` to create, bind, and recycle list item widgets. For smooth scrolling:

### Bind/Unbind Pattern

```rust
factory.connect_setup(|_, item| {
    // SETUP: create the widget structure once (recycled across items)
    let expander = gtk4::TreeExpander::new();
    let label = gtk4::Label::new(None);
    expander.set_child(Some(&label));
    item.set_child(Some(&expander));
});

factory.connect_bind(|_, item| {
    // BIND: update content for the current data item (called on scroll)
    // Keep this FAST — no I/O, no allocations, no complex logic
    let row = item.item().and_downcast::<gtk4::TreeListRow>().unwrap();
    let file_item = row.item().and_downcast::<FileTreeItem>().unwrap();
    
    let expander = item.child().and_downcast::<gtk4::TreeExpander>().unwrap();
    expander.set_list_row(Some(&row));
    
    let label = expander.child().and_downcast::<gtk4::Label>().unwrap();
    label.set_text(&file_item.name());
});

factory.connect_unbind(|_, item| {
    // UNBIND: clean up bindings (optional but good practice)
    let expander = item.child().and_downcast::<gtk4::TreeExpander>().unwrap();
    expander.set_list_row(None::<&gtk4::TreeListRow>);
});
```

### Performance Rules for Factories

- **`setup`**: Allocate widgets. Called infrequently (pool size).
- **`bind`**: Set properties only. Called on every scroll. Must be <1ms.
- **`unbind`**: Reset bindings. Called on every scroll. Must be <1ms.
- **Never** do I/O in `bind` — if you need an icon from disk, load it async and cache.
- **Never** create new widgets in `bind` — reuse the ones from `setup`.
- **Avoid** signal connections in `bind` (disconnect in `unbind` if you must).

## Pattern 5: Signal Handling Performance

### Use `connect_*_local` for Closures Capturing GTK Objects

```rust
// WRONG: connect_notify requires Send closure (GTK objects aren't Send)
buffer.connect_notify(Some("modified"), move |buf, _| {
    page.set_title(...); // page is not Send — COMPILE ERROR
});

// CORRECT: connect_notify_local allows non-Send closures
buffer.connect_notify_local(Some("modified"), move |buf, _| {
    page.set_title(...); // OK: guaranteed to run on main thread
});
```

### Debouncing: Delay Rapid Signal Bursts

For search-as-you-type or filter operations, debounce to avoid processing every keystroke:

```rust
use std::cell::Cell;

let timeout_id: Rc<Cell<Option<glib::SourceId>>> = Rc::new(Cell::new(None));

search_entry.connect_changed(clone!(@weak timeout_id, @weak self as sidebar => move |entry| {
    // Cancel previous pending search
    if let Some(id) = timeout_id.take() {
        id.remove();
    }
    
    let query = entry.text().to_string();
    // Schedule new search after 150ms of inactivity
    let id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(150),
        clone!(@weak sidebar => move || {
            sidebar.execute_search(&query);
            timeout_id.set(None);
        }),
    );
    timeout_id.set(Some(id));
}));
```

### Batch Property Updates with `freeze_notify`

When updating multiple GObject properties at once, prevent intermediate signal emissions:

```rust
widget.freeze_notify();
widget.set_property("title", &new_title);
widget.set_property("subtitle", &new_subtitle);
widget.set_property("icon-name", &new_icon);
widget.thaw_notify(); // All three changes emit ONE notification
```

## Pattern 6: Memory Management

### Prevent Signal Handler Leaks

Every `connect_*` call returns a `SignalHandlerId`. If you connect signals in a loop or conditionally, store and disconnect them:

```rust
// In imp.rs
pub struct MyWidget {
    handler_ids: RefCell<Vec<glib::SignalHandlerId>>,
}

// When connecting
let id = some_object.connect_changed(|_| { ... });
self.handler_ids.borrow_mut().push(id);

// When cleaning up (e.g., switching documents)
for id in self.handler_ids.borrow_mut().drain(..) {
    some_object.disconnect(id);
}
```

### Use Weak References in Long-Lived Closures

Signal closures that capture widget references should use `@weak` to prevent circular references:

```rust
// GOOD: weak reference — closure becomes no-op if widget is destroyed
button.connect_clicked(clone!(@weak self as window => move |_| {
    window.do_something();
}));

// BAD: strong reference — keeps widget alive forever
let window_clone = self.clone();
button.connect_clicked(move |_| {
    window_clone.do_something(); // window can never be freed
});
```

## Pattern 7: Large File Handling

For files larger than a few MB, consider:

### Streaming Read with Progress

```rust
let path = path.to_path_buf();
async_task::spawn_blocking_then(
    self.clone(),
    move || {
        let metadata = std::fs::metadata(&path)?;
        let size = metadata.len();
        
        if size > 10_000_000 {
            // For very large files, read in chunks for cancellability
            // (GtkSourceView handles the display — we just need to get text into the buffer)
            tracing::info!("Loading large file ({} bytes): {}", size, path.display());
        }
        
        std::fs::read_to_string(&path)
    },
    |editor, result| {
        match result {
            Ok(content) => editor.apply_loaded_content(&content),
            Err(e) => tracing::error!("Failed to load: {e}"),
        }
    },
);
```

### GtkSourceView Buffer Size Limits

GtkSourceView handles large files reasonably well, but syntax highlighting becomes expensive above ~10MB. For very large files, consider disabling highlighting:

```rust
if file_size > 10_000_000 {
    buffer.set_language(None::<&sourceview5::Language>);
    buffer.set_highlight_syntax(false);
}
```

## Anti-Patterns to Flag

### [FLAG] — Blocking I/O on the Main Thread

```rust
// Any of these in ui/ code (outside spawn_blocking_then) is a flag:
std::fs::read_to_string(&path)     // File read
std::fs::write(&path, content)     // File write
std::fs::read_dir(&path)           // Directory scan
std::fs::metadata(&path)           // Stat call (usually fast, but NFS/FUSE can block)
std::process::Command::new(...)    // Subprocess
```

### [FLAG] — `autoexpand = true` on TreeListModel

Causes unbounded recursive expansion. Always `false`.

### [FLAG] — `connect_notify` Instead of `connect_notify_local`

When the closure captures non-Send GTK objects. The non-local variant requires `Send`, which GTK objects are not.

### [RECOMMEND] — Missing Debounce on User Input

Search entries, filter inputs, and other rapid-fire text inputs should debounce to avoid redundant work.

### [RECOMMEND] — Signal Connection Without Cleanup Strategy

Signals connected in a loop or conditionally should have a disconnect strategy. Otherwise, handlers accumulate and slow down signal emission.

### [CONSIDER] — Synchronous Small Config Reads

Reading a <1KB JSON config synchronously at startup is acceptable. Flag only if it's in a hot path (e.g., called per-keystroke) or if the file could be on a slow filesystem.

## Tone

Performance advice should be specific and measurable. Instead of "this might be slow," say "this blocks the main thread for ~50ms on a directory with 1000 entries." Acknowledge existing good patterns (like the current `spawn_blocking_then` usage in `file_tree.rs` and `editor_page`) before suggesting improvements.
