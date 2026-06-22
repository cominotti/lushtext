# Allocation Patterns — Reference Documentation

Documentation of established allocation patterns in LushText. This file exists as **reference context** for understanding why certain patterns exist in the codebase, not as a checklist of things to flag during reviews.

## Table of Contents

1. [simdutf8 File Loading Pattern](#1-simdutf8-pattern)
2. [Save-Path Memory Model](#2-save-path)
3. [Error Enum Pattern](#3-error-enum)
4. [ListStore splice Pattern](#4-splice)

---

## 1. simdutf8 File Loading Pattern {#1-simdutf8-pattern}

**Status: IMPLEMENTED** — All file loads use SIMD UTF-8 validation.

The code uses `services::filesystem::read::bytes` + `simdutf8::basic::from_utf8` + `String::from_utf8_unchecked` for all file sizes. This is the established pattern — new file-loading code should follow it.

### Pattern (editor_page/mod.rs)

```rust
let bytes = filesystem::read::bytes(&file_path).map_err(read_err)?;
let content = match simdutf8::basic::from_utf8(&bytes) {
    Ok(_) => unsafe { String::from_utf8_unchecked(bytes) },
    Err(_) => {
        return Err(EditorLoadError::Read {
            path: file_path,
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8"),
        });
    }
};
```

---

## 2. Save-Path Memory Model {#2-save-path}

In `editor_page/mod.rs`, the save path extracts buffer content as:

```rust
let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
let content = text.to_string();  // copies entire file content
```

The Rust String copy is **unavoidable** because:
1. `GString` is not `Send` — it cannot cross thread boundaries
2. The background thread needs owned data

For a 50MB file, peak memory during save is ~3x file_size (GtkTextBuffer + GString + String). This is an accepted cost — no action needed.

---

## 3. Error Enum Pattern {#3-error-enum}

**Status: IMPLEMENTED** — `services/editor_io.rs` uses a `thiserror`-derived `EditorLoadError` enum.

```rust
#[derive(Debug, thiserror::Error)]
pub enum EditorLoadError {
    #[error("load cancelled")]
    Cancelled,
    #[error("Cannot stat {path}: {source}")]
    Metadata { path: PathBuf, source: std::io::Error },
    #[error("Failed to read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("{path} is too large to edit ({size_mb} MB). Consider a pager like `less`.")]
    TooLarge { path: PathBuf, size_mb: u64 },
}
```

**Why this matters for correctness**: `EditorLoadError::Cancelled` enables exhaustive pattern matching. No fragile string comparisons. The error variants document what can go wrong.

New service functions should follow this pattern when they have distinct failure modes.

---

## 4. ListStore splice Pattern {#4-splice}

**Status: IMPLEMENTED** — Both command palette results and file tree children use `ListStore::splice()`.

```rust
// Correct: single items-changed signal
store.splice(0, store.n_items(), &items);
```

This matters for UI smoothness — per-item `append()` fires N signals causing visible jank in the ListView. `splice()` fires one signal for the entire batch.

---

## Patterns That Are Fine As-Is

These patterns have been reviewed and are acceptable. Do not flag them:

- **`to_string_lossy().into_owned()`** — Used in file tree sort and index building. The `.into_owned()` pattern is already in place where it matters.
- **`Arc<PathBuf>` for workspace folders** — Already implemented in `IndexedFile`. Files in the same indexed folder share one allocation.
- **Vec + sort + truncate for top-N search results** — Already implemented in `search_items`. Simple collect, sort descending, truncate at max=50. Readable and fast enough for the fixed small k.
- **`Vec::with_capacity` for large known-size collections** — Used where the size is clearly known (e.g., index rebuild). Don't flag missing capacity hints for small or unknown-size collections.
