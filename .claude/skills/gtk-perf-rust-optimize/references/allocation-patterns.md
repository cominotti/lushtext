# Allocation Patterns & Zero-Copy Opportunities

Patterns for eliminating unnecessary allocations in LushText's hot paths. Each pattern includes the anti-pattern, the fix, and the quantified impact.

## Table of Contents

1. [to_string_lossy().to_string() → .into_owned()](#1-to-string-lossy)
2. [Save-Path Memory Doubling](#2-save-path)
3. [Cow<str> Opportunities](#3-cow)
4. [Error Enum vs String](#4-error-enum)
5. [Collection Intermediaries](#5-collections)

---

## 1. to_string_lossy().to_string() → .into_owned() {#1-to-string-lossy}

`OsStr::to_string_lossy()` returns `Cow<'_, str>`. When the path is valid UTF-8 (>99.9% of paths on Linux/macOS), the `Cow` is `Borrowed` — calling `.to_string()` on a borrowed Cow allocates a new String unnecessarily. `.into_owned()` only allocates when the Cow is already `Owned` (non-UTF-8 path), and returns the inner String directly when `Owned`.

### Anti-pattern

```rust
let name = path.file_name()
    .map(|n| n.to_string_lossy().to_string())  // always allocates
    .unwrap_or_default();
```

### Fix

```rust
let name = path.file_name()
    .map(|n| n.to_string_lossy().into_owned())  // allocates only for non-UTF-8
    .unwrap_or_default();
```

### Status: ALL FIXED

All sites now use `.into_owned()` instead of `.to_string()`. The `file_tree.rs` sort uses `UniCase` comparison with zero String allocations during sort. No `.to_string_lossy().to_string()` call chains remain in the codebase.

### When to keep `.to_string()`

If you need the String to outlive the OsStr source (e.g., moving into a struct), `.into_owned()` is correct. If you only need a `&str` temporarily (e.g., passing to a function that accepts `&str`), avoid the allocation entirely:

```rust
// No allocation — borrow the Cow
let lossy = path.file_name().unwrap().to_string_lossy();
some_function(&lossy);  // &Cow<str> auto-derefs to &str
```

---

## 2. Save-Path Memory Doubling {#2-save-path}

In `editor_page/mod.rs`, the save path extracts buffer content as:

```rust
let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
let content = text.to_string();  // copies entire file content
```

`buffer.text()` returns a `glib::GString`. `.to_string()` copies it into a Rust `String`. For a 50MB file, this means:

- 50MB in GtkTextBuffer (internal B-tree)
- 50MB in GString (GTK allocation)
- 50MB in String (Rust allocation)
- = ~150MB peak during save

The Rust String copy is **unavoidable** because:
1. `GString` is not `Send` — it cannot cross thread boundaries
2. The background thread needs owned data
3. GString's internal representation may not be contiguous UTF-8

**When auditing**: flag this pattern if it appears without a comment acknowledging the unavoidable copy. If a future gtk-rs version provides a way to extract bytes from GString as `Vec<u8>` without copying, this should be revisited.

---

## 3. Cow<str> Opportunities {#3-cow}

Functions that accept `String` but only read the data waste an allocation when called with a string literal or a borrowed string. Use `Cow<'_, str>` or `&str` instead.

### Pattern: Function that sometimes owns, sometimes borrows

```rust
// Anti-pattern: always allocates
fn set_label(&self, text: String) {
    self.label.set_text(&text);
}

// Fix: accept &str since we only read
fn set_label(&self, text: &str) {
    self.label.set_text(text);
}
```

### Pattern: Conditional allocation

```rust
use std::borrow::Cow;

// When the function sometimes needs to allocate (e.g., formatting)
// and sometimes can borrow:
fn display_name(&self) -> Cow<'_, str> {
    if let Some(name) = self.cached_name.as_deref() {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("Untitled-{}", self.id))
    }
}
```

### Where Cow matters in LushText

- `IndexedFile.name`: currently `String`. If the name is always derived from `Path::file_name()` and consumed by reference in search, this could be `Cow<'static, str>` for command names and `String` for file names. However, since IndexedFile is stored long-term, `String` is correct here — Cow would tie the lifetime to the source Path.
- Status bar messages: `push_message(&str, MessageKind)` already takes `&str` — correct.

---

## 4. Error Enum vs String {#4-error-enum}

**Status: IMPLEMENTED** — `editor_page/mod.rs` uses a `thiserror`-derived `LoadError` enum with `Cancelled`, `InvalidUtf8`, and `Io` variants. Pattern matching replaces string comparison.

### Current pattern (editor_page/mod.rs)

```rust
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("load cancelled")]
    Cancelled,
    #[error("file is not valid UTF-8: {0}")]
    InvalidUtf8(std::path::PathBuf),
    #[error("{0}")]
    Io(String),
}

// Zero allocation for cancellation:
if cancelled.load(Ordering::Relaxed) {
    return Err(LoadError::Cancelled);
}

// Exhaustive pattern match:
match result {
    Err(LoadError::Cancelled) => { /* silently ignore */ }
    Err(e) => { /* show error in UI */ }
    Ok(content) => { /* load into buffer */ }
}
```

**Benefits**: `LoadError::Cancelled` is zero-size (no heap allocation), pattern matching is exhaustive, no fragile string comparisons.

---

## 5. Collection Intermediaries {#5-collections}

### Heap extraction — current pattern

**Status: OPTIMIZED** — `search_items` uses `into_sorted_vec()` for efficient single-allocation extraction:

```rust
// Current: single allocation via into_sorted_vec()
heap.into_sorted_vec().into_iter().map(|r| r.0).collect()
```

At max=50 results, heap overhead is negligible (~4KB).

### splice() vs per-item append()

The codebase already uses `ListStore::splice()` correctly — this is a [GOOD] pattern to protect:

```rust
// Correct: single items-changed signal
store.splice(0, store.n_items(), &items);

// Anti-pattern: N signals, N potential reallocations
for item in &items {
    store.append(item);
}
```
