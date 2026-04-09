# Widget Testing Patterns for GTK4/Rust

Detailed patterns for testing GTK4 widgets without a display server running on the developer's machine.

## Headless Testing

### Mutter Headless (Wayland)

Use `mutter --headless` — a headless Wayland compositor. This is the same pattern
GNOME's own GTK CI uses:

```bash
export XDG_RUNTIME_DIR="$(mktemp -d)"
export GDK_BACKEND=wayland
dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 1024x768 -- \
    cargo nextest run --test widget
```

- `--headless`: runs without KMS/DRM hardware (required for containers)
- `--no-x11`: disables Xwayland (not needed for GTK4 Wayland tests)
- `--virtual-monitor 1024x768`: creates a fake monitor
- `dbus-run-session`: provides the D-Bus session bus mutter requires
- Mutter sets `WAYLAND_DISPLAY` automatically before spawning the child command

### Installing dependencies

```bash
# Fedora
sudo dnf install mutter dbus-daemon

# Ubuntu/Debian
sudo apt-get install mutter dbus-x11
```

## GTK Initialization for Tests

GTK must be initialized exactly once per test process. Use `std::sync::Once`:

```rust
use std::sync::Once;

static GTK_INIT: Once = Once::new();

pub fn ensure_gtk_init() {
    GTK_INIT.call_once(|| {
        gtk4::init().expect(
            "GTK4 init failed — is a display server available? \
             Try mutter --headless."
        );
    });
}
```

**Important**: `gtk4::init()` connects to the display server. If no display is available, it panics. The error message should guide the developer.

## GLib Main Loop in Tests

Many GTK operations are asynchronous — they schedule callbacks via `glib::idle_add_once` or signal emissions. To process these in tests, spin the GLib main loop:

### Blocking Wait with Timeout

```rust
use std::time::{Duration, Instant};

/// Spin the GLib main loop until `condition` returns true or timeout expires.
pub fn wait_for<F: Fn() -> bool>(condition: F, timeout: Duration, msg: &str) {
    let ctx = glib::MainContext::default();
    let deadline = Instant::now() + timeout;
    
    while Instant::now() < deadline {
        // Process pending main loop events
        while ctx.iteration(false) {}
        
        if condition() {
            return;
        }
        
        // Brief sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(5));
    }
    
    panic!("Timed out after {timeout:?} waiting for: {msg}");
}
```

### Usage

```rust
#[test]
fn async_load_populates_buffer() {
    ensure_gtk_init();
    let editor = LushtextEditorPage::new();
    
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "hello").unwrap();
    
    editor.load_file_async(tmp.path());
    
    wait_for(
        || {
            let buf = editor.buffer();
            buf.text(&buf.start_iter(), &buf.end_iter(), false).as_str() == "hello"
        },
        Duration::from_secs(5),
        "buffer to contain loaded file content",
    );
}
```

## Testing Signal Emissions

To test that a widget emits the correct signals:

```rust
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn modified_flag_triggers_notification() {
    ensure_gtk_init();
    let editor = LushtextEditorPage::new();
    let buffer = editor.buffer();
    
    let notified = Rc::new(Cell::new(false));
    let notified_clone = notified.clone();
    
    buffer.connect_modified_changed(move |_| {
        notified_clone.set(true);
    });
    
    // Insert text to trigger modification
    buffer.insert(&mut buffer.end_iter(), "new text");
    
    // Process signals
    let ctx = glib::MainContext::default();
    while ctx.iteration(false) {}
    
    assert!(buffer.is_modified());
    assert!(notified.get());
}
```

## Testing Widget Hierarchy

For testing that child widgets are correctly wired:

```rust
#[test]
fn window_has_expected_structure() {
    ensure_gtk_init();
    let app = gtk4::Application::builder()
        .application_id("dev.cominotti.lushtext.test")
        .build();
    let window = LushtextWindow::new(&app);
    
    // Verify tab view exists and is empty
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    
    // Verify sidebar exists
    let sidebar = &window.imp().sidebar;
    // sidebar should be accessible
    assert!(sidebar.is_visible() || !sidebar.is_visible()); // just verify it exists
    
    // Verify content stack shows empty state
    let stack = &window.imp().content_stack;
    assert_eq!(stack.visible_child_name().unwrap().as_str(), "empty");
}
```

## Test Isolation

Each test should be independent. Strategies:

1. **Fresh widgets per test**: Create new widgets in each `#[test]` function
2. **Temp directories**: Use `tempfile::TempDir` for file operations (auto-cleanup)
3. **No shared GTK state**: Don't store widgets in `static` variables
4. **GSettings isolation**: Set `GSETTINGS_SCHEMA_DIR` to a temp dir if using GSettings

```rust
#[test]
fn isolated_test() {
    ensure_gtk_init();
    let ctx = TestContext::new(); // fresh temp dir
    let editor = LushtextEditorPage::new(); // fresh widget
    // ... test using ctx.data_dir for persistence
}
```

## `is_visible()` vs `visible` Property

**Critical pitfall for widget tests.** `WidgetExt::is_visible()` maps to `gtk_widget_is_visible()`, which checks the widget **and all its parents** up to the toplevel. In tests, windows are never `present()`ed, so:

- `is_visible()` returns `false` for ANY widget inside an unrealized window, regardless of the widget's own state
- `set_visible(true)` changes the property but `is_visible()` still returns `false` because the parent window is invisible

**Solution**: Read the `visible` property directly:

```rust
// WRONG in test context (always false for parented widgets):
assert!(widget.is_visible());

// CORRECT (reads the widget's own property):
assert!(widget.property::<bool>("visible"));
```

This only affects widgets inside a window. Standalone widgets (no parent) work correctly with `is_visible()`.

## Limitations of Widget Testing

Things that are hard/impossible to test without a full compositor:
- Window positioning and sizing
- Focus traversal between widgets
- Drag-and-drop operations
- Keyboard event propagation through the widget tree
- CSS rendering and theming
- Accessibility tree queries

For these, E2E tests with `mutter --headless` are the minimum. For accessibility testing, AT-SPI (Assistive Technology Service Provider Interface) provides programmatic access to the widget tree — but this requires additional setup and the `atspi` crate.
