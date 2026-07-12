# GObject Subclassing and Hexagonal Architecture

## Table of Contents

- [The Widget as Driving Adapter](#the-widget-as-driving-adapter)
- [Keep Business Rules Out of Widget Internals](#keep-business-rules-out-of-widget-internals)
- [Place Decisions at the Narrowest Owning Boundary](#place-decisions-at-the-narrowest-owning-boundary)
- [The CompositeTemplate Boundary](#the-compositetemplate-boundary)
- [`ensure_type()` and Registration](#ensure_type-and-the-registration-order)
- [Thread Safety](#thread-safety-and-the-adapter-boundary)
- [Boundary Summary](#summary-where-boundaries-live)

Where the architectural boundary lives in GTK4/Rust widgets, and how GObject patterns relate to hex arch principles.

## The Widget as Driving Adapter

Many substantial GTK4 widgets in LushText use this two-module convention, while
small widgets or workflow modules may use a different cohesive layout:

```
ui/widget_name/
├── mod.rs   # Public wrapper: glib::wrapper!, public API methods
└── imp.rs   # Private implementation: ObjectSubclass, CompositeTemplate, signals
```

In hexagonal terms, the widget as a whole is a driving adapter. Conventionally,
`mod.rs` exposes its Rust wrapper and caller-facing API, while `imp.rs` owns the
GObject subclass hooks, template children, and instance state. File names do not
create an architectural boundary by themselves; dependency direction and
responsibility do.

## Keep Business Rules Out of Widget Internals

### In `imp.rs`: favor subclass mechanics and local lifecycle

An `imp.rs` commonly contains:
- `ObjectSubclass` trait implementation (type registration)
- `ObjectImpl::constructed()` for signal wiring and initial setup
- `WidgetImpl`, `WindowImpl`, etc. — trait chain required by GTK
- `#[template_child]` field declarations
- `RefCell<T>` fields for runtime state
- Property getters/setters if using GObject properties
- small widget-local lifecycle helpers when keeping them beside the state makes
  ownership clearer

Do not flag control flow merely because it lives in `imp.rs`. When that control
flow encodes reusable domain/application policy rather than widget lifecycle,
extract it to either:
1. A caller-facing widget or workflow method (often exposed from `mod.rs`) if it coordinates UI elements
2. A service function (if it's a business rule)

### In signal closures: keep durable decisions visible

Keep signal closures thin when practical, especially when they would otherwise
hide blocking work, persistence, or reusable decisions:

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

## Place Decisions at the Narrowest Owning Boundary

### In caller-facing widget methods: UI orchestration

Caller-facing widget methods often coordinate multi-step UI operations. They may
live in `mod.rs` or a cohesive workflow module:

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

### In domain or service functions: reusable rules

GTK-free code is eligible for the domain or service layer, but placement still
depends on ownership. A pure invariant over one domain value belongs on that
value; orchestration or infrastructure belongs in services.

```rust
// After an explicit cleanup workflow has produced confirmed retention evidence:
session.retain_tabs_by_path(&retained_paths);
```

The active-index rebasing rule belongs to `SessionData`. The retained-path set
must be precomputed by an explicit cleanup or reconciliation workflow; startup
restore must not drop tabs from future snapshots merely because a path is
temporarily unavailable. Direct existence probes are both an infrastructure
concern and unsafe evidence for destructive restore-time filtering.

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
| Widget public ↔ private | Caller-facing wrapper API / subclass internals, often split across `mod.rs` and `imp.rs` | Rust visibility and module ownership |
| Rust types ↔ GObject types | `glib::wrapper!` macro + `ObjectSubclass` | Type system |
