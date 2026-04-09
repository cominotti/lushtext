# Nucleo Full Framework Migration

> Moved from `.agents/skills/gtk-perf-scale/references/search-scaling.md` — this is an aspirational architecture change, not a deployed pattern.

The codebase currently uses `nucleo-matcher = "0.3"` (the low-level scoring library) via `fuzzy_score` and `search_items` in `services/palette.rs`. This document describes upgrading to `nucleo = "0.5"` (the full async framework with background worker threads, incremental results, and match highlighting).

## Why nucleo's full framework

| Aspect | Current `nucleo-matcher` | Full `nucleo` |
|--------|--------------------------|---------------|
| Scoring | SIMD-accelerated (already in use) | Same scoring engine |
| Threading | Scoring runs on main thread (synchronous) | Dedicated background worker thread |
| Results | Synchronous — blocks until all candidates scored | Incremental — results stream in |
| Match positions | Not tracked | Returns exact match positions (enables highlight rendering) |
| Index ownership | Separate `FileIndex` + `search_items` | `Nucleo<T>` owns both data and matcher |

**RAM note**: `Nucleo<T>` uses ~2x the `IndexedFile` collection during active search (items + scored copies with UTF-32 char buffers). For 100k files (~20MB), expect ~40MB during search, ~20MB idle.

## Integration Guide

### Step 1: Add dependency

```toml
# In workspace Cargo.toml [workspace.dependencies]:
nucleo = "0.5"

# In crates/lushtext-core/Cargo.toml [dependencies]:
nucleo = { workspace = true }
```

Then: `cargo hakari generate && make cargo-sources`

### Step 2: Replace FileIndex + search_items with Nucleo<T>

`Nucleo<T>` owns both the data and the matcher, replacing `FileIndex` + `fuzzy_score` + `search_items` with a single type.

```rust
use nucleo::{Nucleo, Config, Injector, Utf32String};
use nucleo::pattern::{CaseMatching, Normalization, Pattern, AtomKind};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub name: String,
    pub workspace_root: Arc<PathBuf>,
}

pub fn create_matcher(notify: impl Fn() + Send + Sync + 'static) -> Nucleo<IndexedFile> {
    Nucleo::new(
        Config::DEFAULT,
        Arc::new(notify),
        None,   // single column (match against filename)
        1,      // 1 worker thread
    )
}

pub fn inject_files(injector: &Injector<IndexedFile>, roots: &[PathBuf]) {
    let mut visited = std::collections::HashSet::new();
    for root in roots {
        let canonical_root = match root.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let workspace_root = Arc::new(root.clone());
        inject_recursive(injector, root, &workspace_root, &canonical_root, &mut visited, 0);
    }
}

fn inject_recursive(
    injector: &Injector<IndexedFile>,
    dir: &std::path::Path,
    workspace_root: &Arc<PathBuf>,
    canonical_root: &std::path::Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    depth: u32,
) {
    if depth > 64 { return; }
    let canonical = match dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return,
    };
    if !canonical.starts_with(canonical_root) || !visited.insert(canonical) {
        return;
    }

    for (path, is_dir) in crate::services::file_tree::scan_directory(dir) {
        if is_dir {
            inject_recursive(injector, &path, workspace_root, canonical_root, visited, depth + 1);
        } else {
            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let file = IndexedFile {
                path,
                name: name.clone(),
                workspace_root: Arc::clone(workspace_root),
            };
            injector.push(file, |item, cols| {
                cols[0] = Utf32String::from(item.name.as_str());
            });
        }
    }
}

pub fn update_query(nucleo: &mut Nucleo<IndexedFile>, query: &str) {
    nucleo.pattern.reparse(
        0,
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        query.starts_with(&nucleo.pattern.column_pattern(0).atoms().map_or(
            String::new(),
            |atoms| atoms.iter().map(|a| a.text()).collect::<String>(),
        )),
    );
}

pub fn get_results(nucleo: &Nucleo<IndexedFile>, max: usize) -> Vec<(u32, &IndexedFile)> {
    let snapshot = nucleo.snapshot();
    (0..snapshot.matched_item_count().min(max as u32))
        .filter_map(|idx| {
            let item = snapshot.get_matched_item(idx)?;
            Some((item.score, item.data))
        })
        .collect()
}
```

### Step 3: Wire into GTK main loop

The `notify` callback fires on the worker thread. Bounce to main thread:

```rust
let window_weak: glib::SendWeakRef<LushtextWindow> = window.downgrade().into();
let matcher = create_matcher(move || {
    let weak = window_weak.clone();
    glib::idle_add_once(move || {
        if let Some(window) = weak.upgrade() {
            window.imp().command_palette.refresh_from_matcher();
        }
    });
});
```

### Step 4: Match highlighting

nucleo returns `Indices` (matched byte positions) per result for Pango markup rendering:

```rust
let item = snapshot.get_matched_item(position).unwrap();
let indices = item.indices();
let name = &item.data.name;

let mut markup = String::with_capacity(name.len() * 2);
for (i, ch) in name.chars().enumerate() {
    if indices.contains(&(i as u32)) {
        markup.push_str("<b>");
        markup.push(ch);
        markup.push_str("</b>");
    } else {
        match ch {
            '&' => markup.push_str("&amp;"),
            '<' => markup.push_str("&lt;"),
            '>' => markup.push_str("&gt;"),
            _ => markup.push(ch),
        }
    }
}
name_label.set_markup(&markup);
```

### What changes in existing code

- **Delete**: `fuzzy_score`, `fuzzy_score_chars`, `search_items` in `palette.rs` — replaced by nucleo
- **Delete**: `FileIndex` struct and `collect_files_recursive` — replaced by `Injector` + `inject_files`
- **Keep**: `all_commands()`, `search_commands()` — commands are ~11 items; nucleo overhead not justified
- **Keep**: `search_all()` as dispatcher between file results (nucleo) and command results (existing loop)

## SIMD Crate Adoption Beyond Search

> Cross-reference: For simdutf8 (file-load path), see `gtk-perf-rust-optimize/references/simd-opportunities.md`.

### memchr — Fast byte scanning

Already a transitive dependency. Use explicitly for newline counting in large files:

```rust
use memchr::memchr_iter;

fn count_lines(content: &[u8]) -> usize {
    memchr_iter(b'\n', content).count()
}
```

50MB file: ~1.5ms with memchr vs ~50ms scalar.

### aho-corasick — Multi-pattern matching (future)

For project-wide search or multi-keyword filtering. Uses memchr internally for initial byte scan + DFA for multi-pattern matching.
