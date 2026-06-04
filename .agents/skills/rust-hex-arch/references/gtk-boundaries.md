# GObject Subclassing and Hexagonal Architecture

Where the architectural boundary lives in GTK4/Rust widgets, and how GObject patterns relate to hex arch principles.

## The Widget as Driving Adapter

Every GTK4 widget in LushText follows the two-module pattern:

```
ui/widget_name/
├── mod.rs   # Public wrapper: glib::wrapper!, public API methods
└── imp.rs   # Private implementation: ObjectSubclass, CompositeTemplate, signals
```

In hexagonal terms:
- **`mod.rs`** defines the **driving port** — the public API that other code (including other widgets) uses to interact with this widget
- **`imp.rs`** is the **adapter internals** — how the widget translates between GTK's object model and the application's service layer

## Where Business Logic Must NOT Live

### In `imp.rs`: Never

The `imp.rs` file should contain only:
- `ObjectSubclass` trait implementation (type registration)
- `ObjectImpl::constructed()` for signal wiring and initial setup
- `WidgetImpl`, `WindowImpl`, etc. — trait chain required by GTK
- `#[template_child]` field declarations
- `RefCell<T>` fields for runtime state
- Property getters/setters if using GObject properties

If you find yourself writing `if/else` chains with domain logic in `imp.rs`, extract it to either:
1. A method on `mod.rs` (if it coordinates UI elements)
2. A service function (if it's a business rule)

### In signal closures: Minimal

Signal closures in `constructed()` should be thin dispatchers:

```rust
// Good: thin dispatcher
fn constructed(&self) {
    let window = self.obj().clone();
    self.sidebar.connect_file_activated(move |path| {
        window.open_document(path);
    });
}
```

```rust
// Bad: business logic in a closure
fn constructed(&self) {
    let window = self.obj().clone();
    self.sidebar.connect_file_activated(move |path| {
        if path.extension().map_or(false, |e| SUPPORTED_EXTENSIONS.contains(&e.to_str().unwrap())) {
            let content = filesystem::read::text(&path).unwrap();
            if content.len() > MAX_FILE_SIZE {
                window.show_error("File too large");
            } else {
                window.open_document(path);
            }
        }
    });
}
```

The second example has three problems:
1. Extension validation is domain logic → belongs in a service or domain method
2. Synchronous file I/O on the main thread → use `spawn_blocking_then`
3. Size checking is a business rule → belongs in a service

## Where Business Logic SHOULD Live

### In `mod.rs` methods: UI orchestration

The widget's `mod.rs` is where multi-step UI operations live. These methods coordinate between GTK widgets and service calls:

```rust
impl LushtextWindow {
    pub fn open_document(&self, path: &Path) {
        // Step 1: Check for duplicate tab (UI concern — iterates AdwTabView)
        if let Some(page) = self.find_page_for_path(path) {
            self.imp().tab_view.set_selected_page(&page);
            return;
        }
        // Step 2: Create new editor page (UI concern — creates widget)
        let editor = LushtextEditorPage::new();
        let page = self.imp().tab_view.append(&editor);
        // Step 3: Load file content (delegates to service via async)
        editor.load_file_async(path);
    }
}
```

This is appropriate — it coordinates UI elements. The business decision "don't open duplicate tabs" is a UI policy, not a domain rule.

### In service functions: Business rules

Anything that doesn't need a GTK type to operate belongs in services:

```rust
// services/session_service.rs
pub fn filter_existing_tabs(data: &mut SessionData) {
    data.tabs.retain(|tab| filesystem::metadata::exists(&tab.path));
    if let Some(ref active) = data.active_tab {
        if !data.tabs.iter().any(|t| t.path == *active) {
            data.active_tab = None;
        }
    }
}
```

This logic uses only `std::path::Path::exists()` and domain types — it belongs in services, not in the window widget.

## The CompositeTemplate Boundary

`#[derive(CompositeTemplate)]` and `#[template_child]` are framework metadata that binds Rust fields to XML template elements. They are NOT architectural coupling — they're the mechanism by which GTK4 finds and connects UI elements defined in `.ui` files.

**Do not flag these as architectural issues:**
```rust
#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/window.ui")]
pub struct LushtextWindow {
    #[template_child]
    pub tab_view: TemplateChild<libadwaita::TabView>,
    #[template_child]
    pub sidebar: TemplateChild<LushtextSidebar>,
    // ...
}
```

This is equivalent to constructor injection in a DI framework — it's how the adapter receives its dependencies.

## `ensure_type()` and the Registration Order

`ensure_type()` in `class_init()` is a GObject registration requirement, not an architectural decision:

```rust
fn class_init(klass: &mut Self::Class) {
    LushtextSidebar::ensure_type();
    LushtextEditorPage::ensure_type();
    klass.bind_template();
}
```

This registers child widget types with the GObject type system before the template parser encounters them. Skip it and the template fails at runtime. It's infrastructure, not coupling.

## Thread Safety and the Adapter Boundary

GTK objects are not `Send`/`Sync` (raw pointers inside). This creates a hard boundary:

```
Main Thread (GTK)                    Background Thread
┌─────────────────┐                 ┌─────────────────┐
│ UI widgets       │                 │ File I/O         │
│ Signal handlers  │◄── idle_add ───│ Directory scan   │
│ Property updates │    ─────────►  │ Data processing  │
│                  │  ThreadGuard    │                  │
└─────────────────┘                 └─────────────────┘
```

`spawn_blocking_then` encodes this boundary:
- `state` (GTK object) stays on the main thread via `ThreadGuard`
- `work` runs on a background thread (must be `Send`)
- `then` runs on the main thread via `glib::idle_add_once`

This IS the adapter boundary for async operations. The background thread is a driven adapter performing I/O; the main thread callback is the driving adapter updating the UI with the result.

## Summary: Where Boundaries Live

| Boundary | Mechanism | Enforced by |
|----------|-----------|-------------|
| Domain ↔ Application | Module imports (`model/` never imports `services/`) | Code review (no compile-time enforcement within a crate) |
| Application ↔ UI | Module imports (`services/` never imports `ui/`) | Code review + this skill's [FLAG] severity |
| Main thread ↔ Background thread | `ThreadGuard` + `spawn_blocking_then` | Compile-time (`Send` bound) |
| Widget public ↔ private | `mod.rs` (public API) / `imp.rs` (internals) | `pub` visibility |
| Rust types ↔ GObject types | `glib::wrapper!` macro + `ObjectSubclass` | Type system |
