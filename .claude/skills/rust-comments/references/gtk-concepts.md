# GTK/GLib Concepts Reference

When these patterns appear in code, the **first use in each file** must have an explanatory comment. This reference provides the expected explanation for each concept.

The goal is not to teach GTK/GLib comprehensively — it's to give enough context that a developer can understand the code they're reading and know what to search for if they need more detail.

---

## Core GObject System

### `glib::wrapper!` Macro

**What it is:** Generates the public wrapper type for a GObject subclass — the Rust struct, type-safe casting, reference counting, and connection to the private implementation in `imp.rs`.

**When you see it:** Every `mod.rs` file for a custom widget.

**Expected comment:**
```rust
// This macro generates the public wrapper type for our widget. It declares
// that LushtextWindow is a GObject subclass (ObjectSubclass) whose private
// implementation lives in imp::LushtextWindow. The @extends chain declares
// the GTK class hierarchy (our type -> AdwApplicationWindow -> GtkWindow -> ...),
// and @implements lists the interfaces this type supports.
glib::wrapper! {
    pub struct LushtextWindow(ObjectSubclass<imp::LushtextWindow>)
        @extends libadwaita::ApplicationWindow, gtk4::ApplicationWindow,
                 gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}
```

### The `imp.rs` / `mod.rs` Split

**What it is:** Every custom widget in gtk-rs requires two modules. `imp.rs` holds the private struct (fields, trait implementations). `mod.rs` holds the public wrapper type and API. This is a requirement of the gtk-rs bindings, not a style choice.

**Why:** GLib's type system needs a private struct for instance data and a separate public type for the API. The Rust bindings enforce this at the module level.

**Expected comment (in mod.rs):**
```rust
// Private implementation module. In GTK's GObject system, every widget has
// two halves: a private struct (imp.rs) holding data and trait impls, and
// a public wrapper type (this file) providing the API. This split is a
// requirement of the gtk-rs bindings — it mirrors how GObject's C-level
// class/instance structs work.
mod imp;
```

### `ObjectSubclass` / `ObjectImpl`

**What it is:** The trait chain that registers a Rust struct as a GObject type. `ObjectSubclass` defines the type name, parent class, and associated types. `ObjectImpl` handles lifecycle callbacks (`constructed()`, `dispose()`).

**Expected comment:**
```rust
// ObjectSubclass registers this struct with GLib's runtime type system.
// NAME is the GType identifier (must match the `class` attribute in UI
// templates). ParentType sets which GTK widget we're extending.
impl ObjectSubclass for LushtextWindow {
    const NAME: &'static str = "LushtextWindow";
    type Type = super::LushtextWindow;
    type ParentType = libadwaita::ApplicationWindow;
```

### `CompositeTemplate`

**What it is:** A derive macro that loads a GTK UI template (XML file from the GResource bundle) and binds `#[template_child]` fields to named widgets in the template.

**Expected comment:**
```rust
// CompositeTemplate loads the UI layout from a compiled XML file (bundled
// as a GResource at build time). Each #[template_child] field is auto-bound
// to the widget with the matching `id` attribute in the XML — no manual
// widget creation or lookup needed.
#[derive(CompositeTemplate, Default)]
#[template(resource = "/dev/cominotti/lushtext/ui/window.ui")]
pub struct LushtextWindow {
    #[template_child]
    pub tab_view: TemplateChild<libadwaita::TabView>,
```

### `ensure_type()` in `class_init()`

**What it is:** Pre-registers a GObject type before template parsing. Without it, GTK won't recognize custom widget types referenced in UI templates.

**Expected comment:**
```rust
fn class_init(klass: &mut Self::Class) {
    // Register our custom widget types BEFORE the template is parsed.
    // GTK needs to know about these types when it encounters them in
    // the UI XML — without ensure_type(), template parsing fails with
    // "unknown type" errors.
    LushtextSidebar::ensure_type();
    LushtextEditorPage::ensure_type();
    klass.bind_template();
}
```

---

## Interior Mutability (GObject Context)

### `Cell<T>` and `RefCell<T>` on imp Structs

**What it is:** GObject methods always receive `&self` (shared reference), never `&mut self`, because multiple parts of the widget tree can hold references to the same object simultaneously. To store mutable state, GObject structs use interior mutability: `Cell<T>` for `Copy` types (no borrow overhead, no panic risk), `RefCell<T>` for everything else (checked borrows at runtime).

**Expected comment:**
```rust
// GObject methods always take &self because multiple widgets can hold
// references to the same object at once. To store mutable state, we use
// Cell<T> for simple Copy types (like u32 — get/set with no borrowing)
// and RefCell<T> for complex types (like HashSet — requires borrow/
// borrow_mut with runtime checks).
pub struct LushtextWindow {
    rebuild_gen: Cell<u32>,
    open_paths: RefCell<HashSet<PathBuf>>,
}
```

---

## Signal System

### `connect_*` Signal Handlers

**What it is:** GObject's observer pattern. Widgets emit named signals when events happen (button clicked, text changed, tab switched). You connect a closure to react. This is the primary communication mechanism between GTK widgets.

**Expected comment:**
```rust
// Connect to the "modified-changed" signal on the GtkSourceBuffer.
// GObject signals are the observer pattern: widgets emit named events,
// and any number of closures can listen. This closure fires whenever
// the buffer's modified state changes (user types, or file is saved).
buffer.connect_modified_changed(move |buf| {
```

### `connect_notify_local` vs `connect_notify`

**What it is:** Both connect to property-change notifications, but `_local` ensures the closure runs on the main thread only. Use `_local` whenever the closure captures GTK objects (which are not `Send`).

**Expected comment:**
```rust
// connect_notify_local (not connect_notify) because the closure captures
// GTK widgets that are not thread-safe. The _local variant guarantees
// main-thread execution — connect_notify could invoke the closure from
// a background thread, which would panic when touching GTK objects.
tab_view.connect_notify_local(Some("selected-page"), move |tv, _| {
```

### `SignalHandlerId` and Disconnection

**What it is:** Signal connections return a handle. Disconnecting prevents the closure (and everything it captures) from being called after the source object or target is no longer relevant.

**Expected comment:**
```rust
// Store the handler ID so we can disconnect in Drop. Without this, the
// closure keeps references to our widgets alive even after the tab is
// closed — causing memory leaks and stale UI updates.
self.style_handler_id.replace(Some(handler_id));
```

---

## Threading

### `ThreadGuard`

**What it is:** A wrapper that makes non-`Send` types (like GTK widgets) movable across threads by enforcing same-thread access at runtime. It implements `Send` but panics if you try to access the inner value from a different thread.

**Expected comment:**
```rust
// ThreadGuard makes this GTK widget "movable" across threads for the type
// system, but enforces at runtime that it's only accessed from the original
// (main) thread. The background thread carries the guard without touching
// the widget inside — when done, it hands the guard back to the main
// thread via idle_add_once, where into_inner() safely unwraps it.
let guard = glib::thread_guard::ThreadGuard::new(widget);
```

### `glib::idle_add_once`

**What it is:** Schedules a closure to run on the GTK main loop's next idle iteration. This is how background threads deliver results to the main thread — since GTK widgets can only be touched from the thread that created them.

**Expected comment:**
```rust
// Deliver the result to the main thread via GLib's main loop. GTK widgets
// can only be accessed from the main thread, so background threads use
// idle_add_once to schedule a closure that runs on the next main loop
// iteration after all pending events are processed.
glib::idle_add_once(move || {
    let state = guard.into_inner();
    then(state, result);
});
```

### `glib::timeout_add_local_once`

**What it is:** Schedules a closure to run after a delay on the main thread. Used for debouncing, auto-dismiss timers, and retry logic.

**Expected comment:**
```rust
// Schedule a delayed callback on the main thread. Unlike idle_add_once
// (which fires ASAP), timeout_add_local_once waits the specified duration.
// The _local suffix means the closure doesn't need to be Send — it runs
// on the main thread only.
glib::timeout_add_local_once(Duration::from_millis(50), move || {
```

---

## Widget Patterns

### `downcast_ref`

**What it is:** GObject's dynamic type casting — like Java's `instanceof` + cast, or Rust's `Any::downcast_ref`. Used because GTK containers return generic widget types that need casting to specific types.

**Expected comment:**
```rust
// Cast the generic GtkWidget from the tab view to our specific EditorPage
// type. This is GObject's dynamic type system — GTK containers store
// children as generic Widget references, so we downcast to access
// EditorPage-specific methods. Returns None if the type doesn't match.
let editor = page.child().downcast_ref::<LushtextEditorPage>()?;
```

### `GtkTreeListModel` + `GtkListView` + `GtkTreeExpander`

**What it is:** GTK4's tree view pattern. Unlike GTK3's single `GtkTreeView` widget, GTK4 composes three pieces: `GtkTreeListModel` provides hierarchical data as a flat list (handling expand/collapse), `GtkListView` renders the flat list with item recycling, and `GtkTreeExpander` adds indent and arrow UI to each row.

**Expected comment:**
```rust
// GTK4 has no dedicated tree widget. Instead, three pieces compose:
// - GtkTreeListModel: flattens hierarchical data into a list, tracking
//   which nodes are expanded/collapsed
// - GtkListView: renders the flat list with efficient item recycling
//   (only creates widgets for visible rows)
// - GtkTreeExpander: adds indentation and expand/collapse arrows per row
//
// This is more flexible than GTK3's GtkTreeView but requires understanding
// how the three pieces interact.
```

### `gio::ListStore` and `splice()`

**What it is:** GObject's observable list. `GtkListView` watches it for changes and updates the UI automatically. `splice()` replaces a range in one operation, emitting a single `items-changed` signal instead of N individual signals.

**Expected comment:**
```rust
// ListStore is GObject's observable list — GtkListView watches it and
// auto-updates when items change. splice() replaces items in a single
// operation (one items-changed signal) instead of N append()/remove()
// calls (N signals, N relayout passes).
list_store.splice(0, list_store.n_items(), &results);
```

---

## Settings and Resources

### `GSettings` and `Settings::bind()`

**What it is:** GNOME's persistent settings system (backed by dconf). `bind()` creates a live two-way sync between a GSettings key and a widget property — changing either side automatically updates the other.

**Expected comment:**
```rust
// GSettings is GNOME's persistent settings system (backed by dconf on
// Linux). bind() creates a live two-way sync between the settings key
// and the widget property. The GET flag makes it one-way: setting changes
// update the widget, but widget changes don't write back to settings.
settings.bind("show-line-numbers", &source_view, "show-line-numbers")
    .flags(gio::SettingsBindFlags::GET)
    .build();
```

### `GResource`

**What it is:** A compiled binary bundle of UI templates, CSS, icons, and other assets. Built at compile time from an XML manifest, loaded at runtime. Widgets reference resources by path (e.g., `"/dev/cominotti/lushtext/ui/window.ui"`).

**Expected comment:**
```rust
// Load the GResource bundle — a compiled archive of UI templates, CSS,
// and icons built from resources/dev.cominotti.lushtext.gresource.xml.
// In dev builds this is embedded in the binary (include_bytes!). In
// installed/Flatpak builds it's loaded from disk at PKGDATADIR.
gio::resources_register_include!("lushtext.gresource")
    .expect("Failed to register resources");
```
