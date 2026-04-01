# Test Recipes by Feature

Concrete test implementations for every user-facing feature in LushText.

## File Operations

### Open File → Tab Created

**Level**: Widget + E2E
```rust
#[test]
fn open_file_creates_tab_with_filename() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    let tmp = write_temp_file("test.rs", "fn main() {}");
    
    window.open_document(tmp.path());
    
    let tab_view = &window.imp().tab_view;
    assert_eq!(tab_view.n_pages(), 1);
    assert!(tab_view.nth_page(0).title().contains("test.rs"));
}
```

### Save File Roundtrip

**Level**: E2E
```rust
#[test]
fn save_preserves_content() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    let tmp = write_temp_file("save_test.txt", "original");
    
    window.open_document(tmp.path());
    wait_for_load(&window);
    
    // Modify content
    let editor = active_editor(&window);
    let buffer = editor.buffer();
    buffer.set_text("modified content");
    
    // Save
    editor.save_file();
    wait_for(|| !buffer.is_modified(), TIMEOUT, "save to complete");
    
    // Verify file on disk
    let saved = std::fs::read_to_string(tmp.path()).unwrap();
    assert_eq!(saved, "modified content");
}
```

### Modified Indicator

**Level**: Widget
```rust
#[test]
fn modified_indicator_appears_on_edit() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    let tmp = write_temp_file("mod.txt", "original");
    
    window.open_document(tmp.path());
    wait_for_load(&window);
    
    let page = window.imp().tab_view.nth_page(0);
    assert!(!page.title().ends_with('*'), "Should not show * before edit");
    
    // Edit the buffer
    let editor = active_editor(&window);
    let buffer = editor.buffer();
    buffer.insert(&mut buffer.end_iter(), " added");
    
    // Process signals
    pump_main_loop();
    
    assert!(page.title().ends_with('*'), "Should show * after edit");
}
```

## Tab Management

### Empty State

**Level**: Widget
```rust
#[test]
fn no_tabs_shows_empty_state() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert_eq!(
        window.imp().content_stack.visible_child_name().unwrap().as_str(),
        "empty"
    );
}

#[test]
fn opening_tab_switches_to_tabs_view() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    let tmp = write_temp_file("tab.txt", "content");
    
    window.open_document(tmp.path());
    pump_main_loop();
    
    assert_eq!(
        window.imp().content_stack.visible_child_name().unwrap().as_str(),
        "tabs"
    );
}
```

### Duplicate Tab Prevention

**Level**: E2E
```rust
#[test]
fn opening_same_file_twice_reuses_tab() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    let tmp = write_temp_file("dup.txt", "content");
    
    window.open_document(tmp.path());
    window.open_document(tmp.path()); // same file again
    
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}
```

## Workspace Persistence

### Save and Restore Session

**Level**: Integration (no display needed)
```rust
#[test]
fn session_roundtrip_preserves_cursor() {
    let ctx = TestContext::new();
    let file = ctx.write_file("test.rs", "line1\nline2\nline3");
    
    let session = SessionData {
        workspace_id: WorkspaceId("test".into()),
        tabs: vec![SessionTab {
            path: file.clone(),
            cursor_line: 2,
            cursor_col: 5,
            scroll_line: 0,
        }],
        active_tab: Some(file.clone()),
    };
    
    session_service::save(&ctx.data_dir, &session).unwrap();
    let loaded = session_service::load(&ctx.data_dir, &session.workspace_id).unwrap();
    
    assert_eq!(loaded.tabs.len(), 1);
    assert_eq!(loaded.tabs[0].cursor_line, 2);
    assert_eq!(loaded.tabs[0].cursor_col, 5);
    assert_eq!(loaded.active_tab, Some(file));
}
```

### Filter Removes Deleted Files

**Level**: Integration
```rust
#[test]
fn session_filter_removes_deleted_files() {
    let ctx = TestContext::new();
    let real_file = ctx.write_file("exists.txt", "content");
    let ghost_path = ctx.root.join("deleted.txt"); // never created
    
    let mut session = SessionData {
        workspace_id: WorkspaceId("test".into()),
        tabs: vec![
            SessionTab { path: real_file.clone(), cursor_line: 0, cursor_col: 0, scroll_line: 0 },
            SessionTab { path: ghost_path.clone(), cursor_line: 0, cursor_col: 0, scroll_line: 0 },
        ],
        active_tab: Some(ghost_path),
    };
    
    session_service::filter_existing_tabs(&mut session);
    
    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.tabs[0].path, real_file);
    assert_eq!(session.active_tab, None); // ghost was active, now cleared
}
```

## Sidebar / File Tree

### Directory Scanning

**Level**: Service (tests scan_directory directly)
```rust
#[test]
fn scan_directory_sorts_dirs_first() {
    let ctx = TestContext::new();
    ctx.write_file("project/b_file.txt", "");
    ctx.mkdir("project/a_dir");
    ctx.write_file("project/c_file.txt", "");
    
    let entries = file_tree::scan_directory(&ctx.root.join("project"));
    
    assert_eq!(entries.len(), 3);
    assert!(entries[0].1, "First entry should be a directory");
    assert_eq!(entries[0].0.file_name().unwrap(), "a_dir");
    assert_eq!(entries[1].0.file_name().unwrap(), "b_file.txt");
    assert_eq!(entries[2].0.file_name().unwrap(), "c_file.txt");
}

#[test]
fn scan_directory_skips_hidden_files() {
    let ctx = TestContext::new();
    ctx.write_file("project/.hidden", "");
    ctx.write_file("project/visible.txt", "");
    
    let entries = file_tree::scan_directory(&ctx.root.join("project"));
    
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0.file_name().unwrap(), "visible.txt");
}
```

## Helper Functions

Shared utilities for test files:

```rust
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

fn test_app() -> gtk4::Application {
    gtk4::Application::builder()
        .application_id("dev.cominotti.lushtext.test")
        .build()
}

fn write_temp_file(name: &str, content: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    dir // return dir to keep it alive
}

fn active_editor(window: &LushtextWindow) -> LushtextEditorPage {
    let tab_view = &window.imp().tab_view;
    let page = tab_view.selected_page().unwrap();
    page.child().downcast::<LushtextEditorPage>().unwrap()
}

fn wait_for_load(window: &LushtextWindow) {
    wait_for(
        || {
            let editor = active_editor(window);
            let buf = editor.buffer();
            buf.text(&buf.start_iter(), &buf.end_iter(), false).len() > 0
        },
        TIMEOUT,
        "file content to load into buffer",
    );
}

fn pump_main_loop() {
    let ctx = glib::MainContext::default();
    while ctx.iteration(false) {}
}
```
