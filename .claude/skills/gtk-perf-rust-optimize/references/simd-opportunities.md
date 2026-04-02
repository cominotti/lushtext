# SIMD Optimization Opportunities

Code patterns for maximizing SIMD acceleration in LushText. The project targets x86-64-v3 (AVX2) and Apple Silicon (NEON) — SIMD paths are guaranteed to run on every target machine.

## Table of Contents

1. [simdutf8 for All File Sizes](#1-simdutf8-universal)
2. [memchr for Byte Scanning](#2-memchr)
3. [SIMD Line Counting](#3-line-counting)
4. [Throughput Reference](#4-throughput)

---

## 1. simdutf8 for All File Sizes {#1-simdutf8-universal}

The current code gates simdutf8 on `!check.syntax_enabled()` (files >10MB). Files <=10MB use `std::fs::read_to_string`, which performs scalar UTF-8 validation internally. This leaves performance on the table for every file load.

**The fix**: always use `std::fs::read` + `simdutf8` + `String::from_utf8_unchecked`. Gate syntax highlighting on file size separately — the UTF-8 validation method should not be tied to the syntax gate.

### Current code (editor_page/mod.rs)

```rust
// Only uses SIMD for large files (>10MB)
if !check.syntax_enabled() {
    let bytes = std::fs::read(&file_path).map_err(read_err)?;
    match simdutf8::basic::from_utf8(&bytes) {
        Ok(_) => unsafe { String::from_utf8_unchecked(bytes) },
        Err(_) => return Err(format!("not valid UTF-8")),
    }
} else {
    std::fs::read_to_string(&file_path).map_err(read_err)?
}
```

### Recommended pattern

```rust
// SIMD UTF-8 validation for ALL file sizes
let bytes = std::fs::read(&file_path).map_err(read_err)?;
let content = match simdutf8::basic::from_utf8(&bytes) {
    // SAFETY: simdutf8 just confirmed these bytes are valid UTF-8
    Ok(_) => unsafe { String::from_utf8_unchecked(bytes) },
    Err(_) => return Err(format!("{} is not valid UTF-8", file_path.display())),
};
// Syntax highlighting gated separately in apply_loaded_content
```

**Why this matters**: even for a 1MB file, simdutf8 validates in ~0.08ms vs ~0.7ms for the scalar path inside `read_to_string`. The `std::fs::read` call also avoids one internal reallocation that `read_to_string` does when growing its String buffer.

---

## 2. memchr for Byte Scanning {#2-memchr}

`memchr` provides SIMD-accelerated single-byte and multi-byte scanning. It is already a transitive dependency (via nucleo-matcher) but should be a direct dependency for explicit use.

### Adding the dependency

```toml
# In workspace Cargo.toml [workspace.dependencies]:
memchr = "2"

# In crates/lushtext-core/Cargo.toml [dependencies]:
memchr = { workspace = true }
```

Then: `cargo hakari generate && make cargo-sources`

### Newline counting

Replace any scalar newline counting with `memchr_iter`:

```rust
use memchr::memchr_iter;

/// Count newlines in a byte slice.
/// ~32 bytes/cycle on AVX2 vs ~1 byte/cycle scalar.
pub fn count_newlines(content: &[u8]) -> usize {
    memchr_iter(b'\n', content).count()
}
```

### Line offset lookup

Find the byte offset where line N begins:

```rust
use memchr::memchr_iter;

/// Find the byte offset of the start of line `line` (0-indexed).
/// Returns None if the content has fewer lines.
pub fn line_start_offset(content: &[u8], line: usize) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }
    memchr_iter(b'\n', content)
        .nth(line - 1)
        .map(|pos| pos + 1)
}
```

### When to use memchr vs scalar

| Data size | Use memchr? | Reason |
|-----------|-------------|--------|
| <64 bytes | No | SIMD setup overhead dominates |
| 64 bytes–1KB | Maybe | Marginal benefit, but no harm |
| >1KB | Yes | 32x throughput on AVX2 |

For LushText, file content is always >64 bytes (the file is open in the editor), so memchr is always beneficial.

---

## 3. SIMD Line Counting {#3-line-counting}

If the status bar displays "Line X, Col Y" by scanning the buffer text for newlines, this should use memchr. For a 50MB file:

| Method | Time |
|--------|------|
| `.chars().filter(\|&c\| c == '\n').count()` | ~50ms |
| `memchr_iter(b'\n', bytes).count()` | ~1.5ms |

The GtkTextBuffer API provides `get_iter_at_offset` and cursor position natively, so this may not be needed if the status bar reads from GTK directly. But any Rust-side line counting (e.g., for the command palette's file preview, or for save-path line counting) should use memchr.

---

## 4. Throughput Reference {#4-throughput}

Reference numbers on modern hardware (AMD Zen4 / Apple M2):

| Operation | Scalar | SIMD | Speedup | Crate |
|-----------|--------|------|---------|-------|
| UTF-8 validation | ~1.5 GB/s | ~12 GB/s | 8x | simdutf8 |
| Byte scanning (memchr) | ~1 byte/cycle | ~32 bytes/cycle | 32x | memchr |
| Fuzzy scoring | ~100ns/candidate | ~50ns/candidate | 2x | nucleo-matcher |
| Multi-pattern search | O(n*k) scalar | O(n) DFA + SIMD | 5-10x | aho-corasick |

These numbers assume warm caches. Cold-cache performance is dominated by memory latency, not instruction throughput — SIMD still wins but by a smaller margin (2-4x instead of 8-32x).
