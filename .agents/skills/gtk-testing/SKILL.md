---
name: gtk-testing
description: "Guide integration and E2E testing strategy for GTK4/Libadwaita applications written in Rust. Trigger whenever writing tests, discussing test strategy, adding new features (to prompt test coverage), modifying test infrastructure, or working on files in tests/. Also trigger when the user asks about testing GTK widgets, headless testing, xvfb-run, TestContext, property-based testing, or CI test configuration. Use proactively after any feature implementation to suggest what tests should accompany the change — even if the user doesn't ask."
---

Guide comprehensive testing for LushText — from unit tests for domain types through E2E tests for full user workflows. GTK4 apps are notoriously hard to test because widgets need a display server, GObject lifecycle is managed by the framework, and async I/O uses GLib's main loop rather than standard Rust async. This skill provides concrete patterns for every testing level.

The testing approach is pragmatic: test as much as possible without a display server (services, models), use `xvfb-run` for widget tests when needed, and treat the existing `TestContext` pattern as the foundation to build on.

## Testing Pyramid for GTK4 Desktop Apps

```
         ┌─────────┐
         │  E2E    │  Few — full user workflows via xvfb-run
         ├─────────┤
         │ Widget  │  Some — individual widget behavior via xvfb-run
         ├─────────┤
         │  Integ  │  Many — cross-service workflows (no display needed)
         ├─────────┤
         │ Service │  Many — business logic, persistence roundtrips
         ├─────────┤
         │  Unit   │  Many — domain model invariants, pure logic
         └─────────┘
```

| Level | What | Location | Display? | Run with |
|-------|------|----------|----------|----------|
| **Unit** | Domain type invariants, pure functions | `model/*.rs` `#[cfg(test)]` | No | `make test-unit` |
| **Service** | Business logic, JSON roundtrips, file operations | `services/*.rs` `#[cfg(test)]` | No | `make test-unit` |
| **Integration** | Cross-service workflows, lifecycle sequences | `tests/integration/` | No | `make test-int` |
| **Widget** | Individual widget creation, property setting, signal emission | `tests/widget/` (new) | Yes | `xvfb-run make test-widget` |
| **E2E** | Full user workflows: open file, edit, save, switch tabs | `tests/e2e/` (new) | Yes | `xvfb-run make test-e2e` |

## Decision Matrix: What Test Level for Each Feature

| Feature | Unit | Service | Integration | Widget | E2E |
|---------|------|---------|-------------|--------|-----|
| Workspace CRUD | Model validation | load/save roundtrip | Full lifecycle | — | — |
| Session persistence | Tab serialization | save/restore/filter | Multi-workspace | — | Tab restore |
| File opening | — | — | — | Tab creation | Open + verify |
| Modified indicator | — | — | — | Buffer modified → title | Edit + check `*` |
| Empty state | — | — | — | Stack shows status page | Close all tabs |
| Sidebar file tree | — | scan_directory | — | TreeListModel population | Click to open |
| Dark mode | — | — | — | Color scheme switch | — |
| Keyboard shortcuts | — | — | — | — | Ctrl+T/O/S |
| Search bar | — | — | — | Toggle visibility | Find text |
| Duplicate tab prevention | — | — | — | — | Open same file twice |

## Level 1: Unit Tests (No Display)

Unit tests live inside `#[cfg(test)]` modules in `model/*.rs`. They test domain type invariants — construction, validation, serialization, equality.

### Pattern: Domain Type Roundtrip

```rust
// model/workspace.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_id_equality() {
        let id1 = WorkspaceId("abc".into());
        let id2 = WorkspaceId("abc".into());
        assert_eq!(id1, id2);
    }

    #[test]
    fn workspace_entry_serialization_roundtrip() {
        let entry = WorkspaceEntry::Directory {
            path: PathBuf::from("/home/user/projects"),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: WorkspaceEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn workspaces_file_default_is_empty() {
        let file = WorkspacesFile::default();
        assert!(file.workspaces.is_empty());
        assert!(file.active_workspace.is_none());
    }
}
```

### Pattern: Property-Based Testing with `proptest`

For domain types with rich invariants, use `proptest` to generate random inputs:

```rust
// In model/session.rs #[cfg(test)]
use proptest::prelude::*;

proptest! {
    #[test]
    fn session_tab_roundtrip(
        path in "(/[a-z]+)+\\.rs",
        cursor_line in 0u32..10000,
        cursor_col in 0u32..1000,
        scroll_line in 0u32..10000,
    ) {
        let tab = SessionTab {
            path: PathBuf::from(path),
            cursor_line,
            cursor_col,
            scroll_line,
        };
        let json = serde_json::to_string(&tab).unwrap();
        let deserialized: SessionTab = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(tab, deserialized);
    }
}
```

Add `proptest` as a dev dependency:
```toml
# workspace Cargo.toml
[workspace.dependencies]
proptest = { version = "1.0", default-features = false, features = ["std"] }
```

## Level 2: Service Tests (No Display)

Service tests exercise business logic with real file I/O on temp directories. They use the `TestContext` pattern already established in the project.

### `TestContext` Pattern (Existing)

```rust
// tests/integration/common.rs
pub struct TestContext {
    _tmp: tempfile::TempDir,
    pub root: PathBuf,
    pub data_dir: PathBuf,
}

impl TestContext {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let data_dir = root.join("data/lushtext");
        std::fs::create_dir_all(&data_dir).unwrap();
        Self { _tmp: tmp, root, data_dir }
    }

    pub fn write_file(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn mkdir(&self, rel: &str) -> PathBuf {
        let path = self.root.join(rel);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
```

### Service Test Examples

```rust
#[test]
fn test_workspace_add_entry_deduplicates() {
    let ctx = TestContext::new();
    let mut data = workspace_manager::load(&ctx.data_dir).unwrap();
    let ws = workspace_manager::active_workspace(&mut data).unwrap();
    let id = ws.id.clone();

    let entry = WorkspaceEntry::Directory {
        path: PathBuf::from("/some/dir"),
    };
    workspace_manager::add_entry(&mut data, &id, entry.clone());
    workspace_manager::add_entry(&mut data, &id, entry.clone()); // duplicate
    
    let ws = data.workspaces.iter().find(|w| w.id == id).unwrap();
    assert_eq!(ws.entries.len(), 1); // not 2
}
```

## Level 3: Integration Tests (No Display)

Integration tests exercise cross-service workflows — sequences of operations that span multiple services. They live in `crates/lushtext/tests/integration/`.

### Test Lifecycle Patterns

```rust
#[test]
fn test_full_workspace_lifecycle() {
    let ctx = TestContext::new();
    
    // 1. Initial load creates default workspace
    let mut data = workspace_manager::load(&ctx.data_dir).unwrap();
    let ws = workspace_manager::active_workspace(&mut data).unwrap();
    assert_eq!(ws.name, "workspace");
    
    // 2. Add entries
    let dir = ctx.mkdir("project");
    workspace_manager::add_entry(&mut data, &ws.id, 
        WorkspaceEntry::Directory { path: dir });
    
    // 3. Save and reload
    workspace_manager::save(&ctx.data_dir, &data).unwrap();
    let reloaded = workspace_manager::load(&ctx.data_dir).unwrap();
    
    // 4. Verify persistence
    assert_eq!(reloaded.workspaces.len(), 1);
    assert_eq!(reloaded.workspaces[0].entries.len(), 1);
}
```

## Level 4: Widget Tests (Requires Display)

Widget tests create individual GTK widgets and verify their behavior. They need a display server — use `xvfb-run` in CI.

### Setup: GTK Initialization in Tests

```rust
// tests/widget/common.rs
use std::sync::Once;

static GTK_INIT: Once = Once::new();

pub fn ensure_gtk_init() {
    GTK_INIT.call_once(|| {
        gtk4::init().expect("Failed to initialize GTK4 for testing");
    });
}
```

### Widget Test Pattern

```rust
// tests/widget/test_editor_page.rs
use crate::common::ensure_gtk_init;

#[test]
fn editor_page_starts_unmodified() {
    ensure_gtk_init();
    let editor = LushtextEditorPage::new();
    assert!(!editor.buffer().is_modified());
    assert!(editor.file_path().is_none());
}

#[test]
fn editor_page_load_sets_content() {
    ensure_gtk_init();
    let editor = LushtextEditorPage::new();
    
    // Write test file
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "hello world").unwrap();
    
    // Load file (synchronously for test — bypass spawn_blocking_then)
    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let buffer = editor.buffer();
    buffer.set_text(&content);
    
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).as_str(),
        "hello world"
    );
}
```

### Testing Async Operations with GLib Main Loop

For operations that use `spawn_blocking_then`, you need to spin the GLib main loop in tests:

```rust
#[test]
fn async_file_load_completes() {
    ensure_gtk_init();
    let editor = LushtextEditorPage::new();
    
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "async content").unwrap();
    
    // Trigger the async load
    editor.load_file_async(tmp.path());
    
    // Spin the main loop until the idle callback fires
    let ctx = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        ctx.iteration(false); // non-blocking iteration
        
        let buffer = editor.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        if text.as_str() == "async content" {
            return; // Success!
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("Async load did not complete within 5 seconds");
}
```

### Helper: Wait for Condition

```rust
pub fn wait_for<F: Fn() -> bool>(condition: F, timeout_ms: u64, description: &str) {
    let ctx = glib::MainContext::default();
    let deadline = std::time::Instant::now() 
        + std::time::Duration::from_millis(timeout_ms);
    
    while std::time::Instant::now() < deadline {
        if condition() {
            return;
        }
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("Timed out waiting for: {description}");
}
```

## Level 5: E2E Tests (Requires Display)

E2E tests exercise full user workflows through the widget hierarchy. They create a `LushtextWindow` and simulate user actions.

```rust
#[test]
fn e2e_open_file_creates_tab() {
    ensure_gtk_init();
    
    let app = LushtextApplication::new();
    // Note: full app.run() requires a running main loop.
    // For E2E tests, create the window directly:
    let window = LushtextWindow::new(&app);
    
    // Write test file
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "test content").unwrap();
    
    // Simulate file open
    window.open_document(tmp.path());
    
    // Verify tab was created
    let tab_view = &window.imp().tab_view;
    assert_eq!(tab_view.n_pages(), 1);
    
    let page = tab_view.nth_page(0);
    assert!(page.title().contains(
        tmp.path().file_name().unwrap().to_str().unwrap()
    ));
}

#[test]
fn e2e_duplicate_open_switches_tab() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    let window = LushtextWindow::new(&app);
    
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "content").unwrap();
    
    // Open same file twice
    window.open_document(tmp.path());
    window.open_document(tmp.path());
    
    // Should still be one tab (dedup)
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn e2e_empty_state_shows_status_page() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    let window = LushtextWindow::new(&app);
    
    // No tabs open — should show empty state
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert_eq!(
        window.imp().content_stack.visible_child_name().unwrap().as_str(),
        "empty"
    );
}
```

## CI Integration

### Makefile Targets

```makefile
# Add to existing Makefile
test-widget:
	$(CARGO_TEST) --test widget $(CARGO_TEST_FLAGS)

test-e2e:
	$(CARGO_TEST) --test e2e $(CARGO_TEST_FLAGS)

test-ui: test-widget test-e2e

# Full test suite with display
test-all-headless:
	xvfb-run -a $(MAKE) test test-ui
```

### GitHub Actions

```yaml
- name: Run headless tests
  run: |
    sudo apt-get install -y xvfb libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev
    xvfb-run -a make test test-widget test-e2e
```

### Test File Structure

```
crates/lushtext/tests/
├── integration.rs            # Existing: #[path] split for integration tests
├── integration/
│   ├── common.rs             # TestContext
│   ├── workspace.rs          # Workspace lifecycle tests
│   └── session.rs            # Session persistence tests
├── widget.rs                 # New: #[path] split for widget tests
├── widget/
│   ├── common.rs             # ensure_gtk_init(), wait_for()
│   ├── editor_page.rs        # EditorPage widget tests
│   ├── sidebar.rs            # Sidebar widget tests
│   └── window.rs             # Window widget tests
├── e2e.rs                    # New: #[path] split for E2E tests
└── e2e/
    ├── common.rs             # App + window setup helpers
    ├── file_operations.rs    # Open, save, close file workflows
    ├── tab_management.rs     # Tab create, switch, close, reorder
    └── workspace.rs          # Workspace switch, session restore
```

## What to Test After Each Feature

When implementing a new feature, always plan tests at the appropriate levels:

**New domain type**: Unit test for construction, serialization roundtrip, validation
**New service function**: Service test with TestContext, edge cases (missing file, empty data)
**New UI widget**: Widget test for initial state, property changes, signal emissions
**New user workflow**: E2E test for the complete happy path + key error cases
**Bug fix**: Regression test at the lowest level that reproduces the bug

## Anti-Patterns

- **Testing GTK internals**: Don't assert on CSS classes, pixel positions, or rendering. Test behavior (tab count, visible text, widget state).
- **Flaky async tests**: Always use `wait_for` with a timeout instead of fixed `sleep`. The exact timing of `idle_add_once` callbacks varies.
- **Testing framework code**: Don't test that `AdwTabView.append()` works — that's GTK's job. Test that YOUR code calls it correctly.
- **Huge E2E tests**: Keep E2E tests focused on one workflow. If a test sets up 5 files, opens 3 tabs, edits 2, and saves 1, it's testing too much. Split it.
