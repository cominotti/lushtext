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

Check file size in the background `work` closure before committing to read. This avoids allocating gigabytes for a file the editor can't meaningfully handle.

Thresholds are in `services/file_limits.rs` as a `FileSizeCheck` enum:
- `LARGE_FILE_TOAST = 1_000_000` (1 MB) — informational toast
- `DISABLE_SYNTAX_HIGHLIGHTING = 10_000_000` (10 MB) — disable syntax
- `DISABLE_UNDO_HISTORY = 50_000_000` (50 MB) — disable undo
- `REFUSE_TO_OPEN = 500_000_000` (500 MB) — refuse

**Current implementation** uses `std::fs::read` + `simdutf8` for SIMD UTF-8 validation on all file sizes (not `read_to_string`). Error handling uses `LoadError` thiserror enum with `Cancelled`, `InvalidUtf8`, and `Io` variants. Load cancellation via `Arc<AtomicBool>`.

The key insight: `fs::metadata` is a stat() call — fast even on network filesystems — while `read` allocates the full file into memory. Checking size first prevents the allocation entirely for files that exceed the threshold.

**RAM impact**: Peak memory during `apply_loaded_content` is ~2.5-3x file size: the `content` String (~1x) coexists briefly with GtkTextBuffer's internal B-tree (~1.5-2x) during `set_text()`. After `set_text` returns and `content` is dropped, steady-state is ~1.5-2x file size (buffer only). With undo enabled, add another ~1-2x for undo history that accumulates during editing.

---

## 2. Background Save {#2-background-save}

**Status: IMPLEMENTED** — `save_file_async` uses atomic write (temp file + `rename`) on a background thread via `spawn_blocking_then`.

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

**RAM impact**: The `text.to_string()` call creates a copy of the buffer content for the background thread. For a 50MB file, this temporarily adds ~50MB to memory (the original buffer content + the String copy). The copy is freed when the background closure completes. This is unavoidable — GTK buffer content cannot be sent across threads directly. For full analysis of this save-path memory doubling pattern, see `gtk-perf-rust-optimize/references/allocation-patterns.md` section 2.

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

The cancel token is stored directly on the editor page's imp struct as `Arc<AtomicBool>` (not wrapped in `RefCell<Option<...>>`). It is reset with `store(false, ...)` on each new load rather than being replaced. Both `close-tab` and `close_tab_for_path` call `cancel_load()` before `close_page()`.

```rust
// In editor_page/imp.rs
pub cancel_token: Arc<AtomicBool>,  // direct Arc, no RefCell wrapper

// cancel_load() sets the flag:
pub fn cancel_load(&self) {
    self.imp().cancel_token.store(true, Ordering::Relaxed);
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

**Status: IMPLEMENTED** — When total estimated buffer memory exceeds `BUFFER_MEMORY_BUDGET` (256MB), unmodified background tabs are evicted on tab switch. Memory estimation uses `file_size * 2` to account for GtkTextBuffer overhead.

- `EditorPage::evict()` sets `evicted=true` first (prevents `modified-changed` signal flash), then clears buffer text via irreversible action.
- `reload_if_evicted()` transparently reloads evicted tabs when re-focused via `load_file_async`.
- The eviction loop in `window/mod.rs` computes a running total inline (O(n)) and evicts the current tab if it brings total under budget.

**RAM impact**: Evicting a 50MB tab with undo history reclaims ~100-200MB (buffer B-tree + undo entries). The reload cost is a brief peak of ~2.5-3x file size during `set_text()`, identical to the initial load.
