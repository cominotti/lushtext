# Test Recipes by Feature

## Table of Contents

- [File Operations](#file-operations)
- [Tab Management](#tab-management)
- [Session Persistence](#session-persistence)
- [Sidebar and File Tree](#sidebar--file-tree)
- [Helper Functions](#helper-functions)

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
            pinned: false,
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

### Domain Retention Rebases the Active Tab

**Level**: Unit

Use `SessionData` retention only with evidence already computed by an explicit
cleanup or reconciliation workflow. Never probe path existence while restoring
a session: a temporarily unavailable mount must not be erased from the next
persisted snapshot.

```rust
#[test]
fn session_retention_uses_precomputed_evidence_and_rebases_active_index() {
    let retained_paths = std::collections::HashSet::from([
        std::path::PathBuf::from("/project/keep.txt"),
    ]);
    let mut session = SessionData {
        tabs: vec![
            SessionTab {
                path: Some("/project/keep.txt".into()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: true,
            },
            SessionTab {
                path: Some("/project/remove.txt".into()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            },
        ],
        active_tab_index: Some(1),
    };

    session.retain_tabs_by_path(&retained_paths);

    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.tabs[0].path.as_deref(), Some(std::path::Path::new("/project/keep.txt")));
    assert!(session.tabs[0].pinned);
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
    assert!(entries[0].is_dir, "first entry should be directory");
    assert_eq!(entries[0].path.file_name().unwrap(), "a_dir");
    assert_eq!(entries[1].path.file_name().unwrap(), "b_file.txt");
    assert_eq!(entries[2].path.file_name().unwrap(), "c_file.txt");
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
