# Large File Patterns for GTK4/Rust

Code examples for handling files from 1MB to 500MB+ in a GtkSourceView-based editor.

## Table of Contents

1. [Size-Gated Loading](#1-size-gated-loading)
2. [Background Save](#2-background-save)
3. [Cancellable File Load](#3-cancellable-file-load)
4. [Syntax Highlighting Gate](#4-syntax-highlighting-gate)
5. [Buffer Eviction](#5-buffer-eviction)

---

## 1. Size-Gated Loading {#1-size-gated-loading}

Check file size in the background `work` closure before committing to `read_to_string`. This avoids allocating gigabytes for a file the editor can't meaningfully handle.

```rust
use crate::services::async_task;
use std::path::PathBuf;

/// File size thresholds (bytes)
const SIZE_TOAST: u64 = 1_000_000;         // 1 MB — informational toast
const SIZE_NO_HIGHLIGHT: u64 = 10_000_000;  // 10 MB — disable syntax highlighting
const SIZE_NO_UNDO: u64 = 50_000_000;       // 50 MB — disable undo history
const SIZE_REFUSE: u64 = 500_000_000;       // 500 MB — refuse to open

/// Result of a size-gated file read.
enum LoadResult {
    /// File loaded successfully with its content and metadata.
    Ok { content: String, size: u64 },
    /// File is too large to open.
    TooLarge { size: u64, path: PathBuf },
    /// I/O error during read.
    Error(String),
}

pub fn load_file_async(&self, path: &Path) {
    let file_path = path.to_path_buf();
    self.imp().file_path.replace(Some(file_path.clone()));

    async_task::spawn_blocking_then(
        self.clone(),
        move || {
            // Check size BEFORE reading content
            let metadata = match std::fs::metadata(&file_path) {
                Ok(m) => m,
                Err(e) => return LoadResult::Error(
                    format!("Cannot read {}: {}", file_path.display(), e)
                ),
            };
            let size = metadata.len();

            if size > SIZE_REFUSE {
                return LoadResult::TooLarge { size, path: file_path };
            }

            match std::fs::read_to_string(&file_path) {
                Ok(content) => LoadResult::Ok { content, size },
                Err(e) => LoadResult::Error(
                    format!("Failed to read {}: {}", file_path.display(), e)
                ),
            }
        },
        |editor, result| {
            match result {
                LoadResult::Ok { content, size } => {
                    editor.imp().file_size.set(Some(size));
                    editor.apply_loaded_content_with_size(&content, size);
                }
                LoadResult::TooLarge { size, path } => {
                    let mb = size / 1_000_000;
                    tracing::warn!("Refused to open {}: {}MB exceeds limit", path.display(), mb);
                    // Show dialog or toast to the user
                }
                LoadResult::Error(msg) => {
                    tracing::error!("{}", msg);
                }
            }
        },
    );
}

/// Apply content with size-dependent feature gating.
fn apply_loaded_content_with_size(&self, content: &str, size: u64) {
    let buffer = self.buffer();

    // For very large files, keep undo permanently disabled
    buffer.begin_irreversible_action();
    buffer.set_text(content);
    if size < SIZE_NO_UNDO {
        buffer.end_irreversible_action();
    }
    // else: leave irreversible action open — no undo for huge files

    buffer.set_modified(false);
    let start = buffer.start_iter();
    buffer.place_cursor(&start);

    // Gate syntax highlighting on size
    if size < SIZE_NO_HIGHLIGHT {
        self.reapply_language();
    } else {
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_highlight_syntax(false);
    }
}
```

The key insight: `fs::metadata` is a stat() call — fast even on network filesystems — while `read_to_string` allocates the full file into memory. Checking size first prevents the allocation entirely for files that exceed the threshold.

**RAM impact**: Peak memory during `apply_loaded_content_with_size` is ~2.5-3x file size: the `content` String (~1x) coexists briefly with GtkTextBuffer's internal B-tree (~1.5-2x) during `set_text()`. After `set_text` returns and `content` is dropped, steady-state is ~1.5-2x file size (buffer only). With undo enabled, add another ~1-2x for undo history that accumulates during editing.

---

## 2. Background Save {#2-background-save}

The current `save_file` in `editor_page/mod.rs` calls `std::fs::write` on the main thread. For files >1MB on anything slower than a local SSD, this freezes the UI.

```rust
pub fn save_file_async(&self) -> bool {
    let path = match self.imp().file_path.borrow().clone() {
        Some(p) => p,
        None => return false, // No path set — caller should show Save As dialog
    };

    let buffer = self.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    let content = text.to_string();

    // Mark as non-modified immediately so the user sees the tab title
    // update right away. If save fails, we'll re-mark as modified.
    buffer.set_modified(false);

    async_task::spawn_blocking_then(
        self.clone(),
        move || -> Result<u64, String> {
            let bytes = content.as_bytes();
            std::fs::write(&path, bytes)
                .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
            Ok(bytes.len() as u64)
        },
        |editor, result| {
            match result {
                Ok(written) => {
                    editor.imp().file_size.set(Some(written));
                    // Push success message to status bar (via window callback)
                }
                Err(msg) => {
                    tracing::error!("{}", msg);
                    // Re-mark as modified since save failed
                    editor.buffer().set_modified(true);
                    // Push error message to status bar
                }
            }
        },
    );

    true
}
```

Design choice: `buffer.set_modified(false)` is called *before* the async write, not after. This gives instant visual feedback (tab title loses the dot). If the write fails, the `then` callback re-marks the buffer as modified. This is the UX pattern used by VS Code and most modern editors — optimistic UI with rollback on failure.

**RAM impact**: The `text.to_string()` call creates a copy of the buffer content for the background thread. For a 50MB file, this temporarily adds ~50MB to memory (the original buffer content + the String copy). The copy is freed when the background closure completes. This is unavoidable — GTK buffer content cannot be sent across threads directly.

---

## 3. Cancellable File Load {#3-cancellable-file-load}

When a user closes a tab while a file is still loading, the background thread should stop doing work as soon as possible rather than finishing a 100MB read that nobody wants.

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::Read;

/// Read a file with periodic cancellation checks.
/// For files < 1MB, uses read_to_string (one check).
/// For files >= 1MB, reads in 256KB chunks with a check per chunk.
fn read_file_cancellable(
    path: &std::path::Path,
    cancelled: &AtomicBool,
) -> Result<Option<String>, std::io::Error> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let metadata = std::fs::metadata(path)?;
    let size = metadata.len() as usize;

    if size < 1_000_000 {
        // Small file: read in one shot, check cancel after
        let content = std::fs::read_to_string(path)?;
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        return Ok(Some(content));
    }

    // Large file: read in chunks with cancellation checks
    let mut file = std::fs::File::open(path)?;
    let mut content = String::with_capacity(size);
    let mut buf = vec![0u8; 256 * 1024]; // 256KB chunks

    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = std::str::from_utf8(&buf[..n])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        content.push_str(chunk);
    }

    Ok(Some(content))
}
```

Store the cancel token on the editor page's imp struct:

```rust
// In editor_page/imp.rs
pub struct LushtextEditorPage {
    // ... existing fields ...
    pub cancel_token: RefCell<Option<Arc<AtomicBool>>>,
}
```

Create and store it when loading starts; set it when the tab is closed:

```rust
// In load_file_async:
let cancelled = Arc::new(AtomicBool::new(false));
self.imp().cancel_token.replace(Some(cancelled.clone()));

// In close-tab action or page close handler:
if let Some(token) = editor.imp().cancel_token.borrow().as_ref() {
    token.store(true, Ordering::Relaxed);
}
```

---

## 4. Syntax Highlighting Gate {#4-syntax-highlighting-gate}

GtkSourceView5's syntax highlighting engine uses regex-based rules that scan backward from the current position to find context boundaries (e.g., "am I inside a string literal?"). For small files, this is imperceptible. For large files, the initial scan can take seconds.

```rust
/// Apply syntax language only if the file is below the highlight threshold.
fn reapply_language_gated(&self, file_size: u64) {
    let buffer = self.buffer();

    if file_size >= SIZE_NO_HIGHLIGHT {
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_highlight_syntax(false);
        tracing::info!(
            "Syntax highlighting disabled for large file ({}MB)",
            file_size / 1_000_000
        );
        return;
    }

    if let Some(ref fp) = *self.imp().file_path.borrow() {
        let lang_manager = sourceview5::LanguageManager::default();
        if let Some(language) = lang_manager.guess_language(fp.to_str(), None) {
            buffer.set_language(Some(&language));
            buffer.set_highlight_syntax(true);
        }
    }
}
```

Note: GtkSourceView5 performs incremental re-highlighting as the user edits, so once the initial highlight pass is done, editing performance is consistent regardless of file size. The threshold primarily protects the initial load experience.

---

## 5. Buffer Eviction {#5-buffer-eviction}

For editors that keep many tabs open, memory can grow unbounded. A simple eviction strategy unloads buffer content from inactive tabs:

```rust
/// Evict buffer content from a background tab to save memory.
/// The file path is preserved so the content can be reloaded when the tab
/// is activated again.
fn evict_buffer(&self) {
    let buffer = self.buffer();
    if !buffer.is_modified() {
        buffer.set_text("");
        self.imp().evicted.set(true);
    }
}

/// Reload content when an evicted tab becomes active.
fn ensure_loaded(&self) {
    if self.imp().evicted.get() {
        self.imp().evicted.set(false);
        if let Some(path) = self.file_path() {
            self.load_file_async(&path);
        }
    }
}
```

This is a **CONSIDER** optimization — only implement if memory monitoring shows tabs collectively consuming >500MB. The tradeoff is a brief reload delay (~50–200ms) when switching to an evicted tab.

**RAM impact**: Evicting a 50MB tab with undo history reclaims ~100-200MB (buffer B-tree + undo entries). Even for smaller files, evicting 20 inactive 500KB tabs reclaims ~20-40MB. The reload cost is a brief peak of ~2.5-3x file size during `set_text()`, identical to the initial load.

When to trigger eviction:
- LRU-based: evict the least-recently-viewed tab when total buffer memory exceeds a budget (e.g., 256MB)
- Count-based: evict when >20 unmodified tabs are open
- Manual: provide a "Close Other Tabs" or "Free Memory" command
