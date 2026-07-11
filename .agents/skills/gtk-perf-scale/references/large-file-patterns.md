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

**Current implementation** uses `services::filesystem::read::bytes` + `simdutf8` for SIMD UTF-8 validation on all file sizes. Error handling uses the `EditorLoadError` thiserror enum with `Cancelled`, `Metadata`, `Read`, and `TooLarge` variants. Load cancellation uses `Arc<AtomicBool>`.

The key insight: `services::filesystem::metadata` performs a lightweight stat-style query, while a full read allocates the file into memory. Checking size first prevents the allocation entirely for files that exceed the threshold.

**RAM impact**: Peak memory during `apply_loaded_content` is ~2.5-3x file size: the `content` String (~1x) coexists briefly with GtkTextBuffer's internal B-tree (~1.5-2x) during `set_text()`. After `set_text` returns and `content` is dropped, steady-state is ~1.5-2x file size (buffer only). With undo enabled, add another ~1-2x for undo history that accumulates during editing.

---

## 2. Background Save {#2-background-save}

**Status: IMPLEMENTED** — `save_file_async` uses durable atomic write (unique temp file + temp-file sync + `rename` + parent-directory sync) on a background thread via `spawn_blocking_then`.

```rust
pub fn save_file_async(&self) -> bool {
    let path = match self.imp().file_path.borrow().clone() {
        Some(p) => p,
        None => return false, // No path set — caller should show Save As dialog
    };
    if self.is_saving() {
        return false; // Duplicate in-flight saves are rejected.
    }

    let buffer = self.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    let content = text.to_string();
    let was_modified = buffer.is_modified();
    self.set_saving(true); // Keeps the view read-only while the snapshot is written.

    gtk_lush_tasks::spawn_blocking_then(
        self.clone(),
        move || -> Result<u64, String> {
            let bytes = content.as_bytes();
            editor_io::write_bytes_to_path(&path, bytes)
                .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
            Ok(bytes.len() as u64)
        },
        |editor, result| {
            editor.set_saving(false);
            match result {
                Ok(written) => {
                    editor.buffer().set_modified(false);
                    editor.imp().file_size.set(Some(written));
                    // Push success message to status bar (via window callback)
                }
                Err(msg) => {
                    tracing::error!("{}", msg);
                    editor.buffer().set_modified(was_modified);
                    // Push error message to status bar
                }
            }
        },
    );

    true
}
```

Design choice: `buffer.set_modified(false)` is called only after the durable write succeeds. The editor stays temporarily read-only while saving, and duplicate save requests fail fast with an in-flight error. This avoids a clean-looking tab whose content has not reached disk yet, and keeps close flows from destroying the last recovery surface before the save result is known.

**RAM impact**: The `text.to_string()` call creates a copy of the buffer content for the background thread. For a 50MB file, this temporarily adds ~50MB to memory (the original buffer content + the String copy). The copy is freed when the background closure completes. This is unavoidable — GTK buffer content cannot be sent across threads directly. For full analysis of this save-path memory doubling pattern, see `gtk-perf-rust-optimize/references/allocation-patterns.md` section 2.

---

## 3. Cancellable File Load {#3-cancellable-file-load}

When a user closes a tab while a file is still loading, the background thread should stop doing work as soon as possible rather than finishing a 100MB read that nobody wants.

**Current implementation**: The codebase uses whole-file `services::filesystem::read::bytes` + `simdutf8` (not chunked reading). Cancellation is checked before and after the read call:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// In the spawn_blocking_then work closure:
if cancel_token.load(Ordering::Relaxed) {
    return Err(EditorLoadError::Cancelled);
}

let bytes = filesystem::read::bytes(&file_path).map_err(read_err)?;

if cancel_token.load(Ordering::Relaxed) {
    return Err(EditorLoadError::Cancelled);
}

let content = match simdutf8::basic::from_utf8(&bytes) {
    // SAFETY: simdutf8 just confirmed these bytes are valid UTF-8
    Ok(_) => unsafe { String::from_utf8_unchecked(bytes) },
    Err(_) => {
        return Err(EditorLoadError::Read {
            path: file_path,
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8"),
        });
    }
};
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

**Status: IMPLEMENTED** — Loaded editors use an O(1) four-byte-per-character upper estimate with accepted file bytes as a floor and explicit evicted bookkeeping. Residency and eligibility transitions coalesce into one GTK-main-loop pass; above 256 MiB the GTK-free policy selects least-recently-used safe candidates toward a 90% low-water mark.

- `EditorPage::evict()` sets `evicted=true` first (prevents `modified-changed` signal flash), then clears buffer text via irreversible action.
- `reload_if_evicted()` transparently reloads evicted tabs when re-focused via `load_file_async`.
- Active, modified, untitled, loading, saving, failed-load, or otherwise non-reloadable pages remain protected, making the budget soft when necessary.
- Candidate application uses window-local identity maps plus access/policy generations and immediate safety revalidation. The complete pass is O(n log n), dominated by LRU sorting.

**RAM impact**: The estimate is a deterministic editor-text policy rather than an exact process-RSS ceiling. Eviction retains a small fixed tab-bookkeeping estimate, and protected user work may keep residency above the soft budget.
