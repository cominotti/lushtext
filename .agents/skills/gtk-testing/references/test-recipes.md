# Test Recipes by Feature

Concrete test patterns aligned with the repo's current helpers and harnesses.

There is no standalone `tests/e2e/` target today. Workflow-level UI regressions usually belong in the widget harness under `crates/lushtext/tests/widget/*.rs`.

## File Operations

### Open File -> Tab Created

**Level**: Widget

```rust
#[test]
fn open_file_creates_tab_with_filename() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.rs");
    crate::common::fixture::write_text(&path, "fn main() {}\n");

    let window = crate::common::test_window();
    window.open_document(&path);

    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    let tab_view = &window.imp().tab_view;
    assert_eq!(tab_view.n_pages(), 1);
    assert!(tab_view.nth_page(0).title().contains("test.rs"));
}
```

### Modified Indicator

**Level**: Widget

```rust
#[test]
fn modified_indicator_appears_on_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mod.txt");
    crate::common::fixture::write_text(&path, "original");

    let window = crate::common::test_window();
    window.open_document(&path);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    let editor = active_editor(&window);
    let buffer = editor.buffer();
    buffer.insert(&mut buffer.end_iter(), " added");
    flush_events();

    let page = window.imp().tab_view.nth_page(0);
    assert!(
        page.title().starts_with('\u{2022}'),
        "modified pages use the title bullet marker"
    );
}
```

## Tab Management

### Duplicate Tab Prevention

**Level**: Widget

```rust
#[test]
fn opening_same_file_twice_reuses_tab() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dup.txt");
    crate::common::fixture::write_text(&path, "content");

    let window = crate::common::test_window();
    window.open_document(&path);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    window.open_document(&path);
    flush_events();

    assert_eq!(window.imp().tab_view.n_pages(), 1);
}
```

### Empty State

**Level**: Widget

```rust
#[test]
fn no_tabs_shows_empty_state() {
    let window = crate::common::test_window();

    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert_eq!(
        window.imp().content_stack.visible_child_name().unwrap().as_str(),
        "empty"
    );
}
```

## Session Persistence

### Save and Restore Session

**Level**: Integration

```rust
#[test]
fn session_roundtrip_preserves_cursor() {
    let ctx = TestContext::new();
    let file = ctx.write_file("test.rs", "line1\nline2\nline3");

    let session = SessionData {
        tabs: vec![SessionTab {
            path: Some(file.clone()),
            draft_id: None,
            cursor_line: 2,
            cursor_col: 5,
            scroll_line: 0,
        }],
        active_tab_index: Some(0),
    };

    session_service::save(ctx.data_dir(), &session).unwrap();
    let loaded = session_service::load(ctx.data_dir()).unwrap();

    assert_eq!(loaded.tabs.len(), 1);
    assert_eq!(loaded.tabs[0].path, Some(file));
    assert_eq!(loaded.tabs[0].cursor_line, 2);
    assert_eq!(loaded.tabs[0].cursor_col, 5);
    assert_eq!(loaded.active_tab_index, Some(0));
}
```

### Filter Removes Deleted Files

**Level**: Integration

```rust
#[test]
fn session_filter_removes_deleted_files() {
    let ctx = TestContext::new();
    let real_file = ctx.write_file("exists.txt", "content");

    let mut session = SessionData {
        tabs: vec![
            SessionTab {
                path: Some(real_file.clone()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
            },
            SessionTab {
                path: Some(ctx.path().join("deleted.txt")),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
            },
        ],
        active_tab_index: Some(1),
    };

    session_service::filter_existing_tabs(&mut session);

    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.tabs[0].path, Some(real_file));
    assert_eq!(session.active_tab_index, None);
}
```

## Sidebar / File Tree

### Directory Scanning

**Level**: Service

```rust
#[test]
fn scan_directory_sorts_dirs_first() {
    let ctx = TestContext::new();
    ctx.write_file("project/b_file.txt", "");
    ctx.mkdir("project/a_dir");
    ctx.write_file("project/c_file.txt", "");

    let entries = file_tree::scan_directory(ctx.path().join("project").as_path());

    assert_eq!(entries.len(), 3);
    assert!(entries[0].1, "first entry should be directory");
    assert_eq!(entries[0].0.file_name().unwrap(), "a_dir");
    assert_eq!(entries[1].0.file_name().unwrap(), "b_file.txt");
    assert_eq!(entries[2].0.file_name().unwrap(), "c_file.txt");
}
```

## Helper Functions

Shared utilities for widget tests:

```rust
use std::time::{Duration, Instant};

fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        flush_events();
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(predicate(), "timed out waiting for widget state");
}

fn active_editor(window: &LushtextWindow) -> LushtextEditorPage {
    let tab_view = &window.imp().tab_view;
    let page = tab_view.selected_page().unwrap();
    page.child().downcast::<LushtextEditorPage>().unwrap()
}
```
