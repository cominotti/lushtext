# Benchmark Setup for LushText

How to add `criterion` benchmarks, what to measure, and how to detect regressions in CI.

## Table of Contents

1. [Adding criterion](#1-adding-criterion)
2. [Benchmark Targets](#2-targets)
3. [Example Benchmarks](#3-examples)
4. [CI Regression Detection](#4-ci)
5. [Expected Baselines](#5-baselines)

---

## 1. Adding criterion {#1-adding-criterion}

Add to workspace `Cargo.toml`:

```toml
[workspace.dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

Add to `crates/lushtext-core/Cargo.toml`:

```toml
[dev-dependencies]
criterion = { workspace = true }

[[bench]]
name = "palette"
harness = false

[[bench]]
name = "file_tree"
harness = false
```

After adding, run:
```bash
cargo hakari generate
make cargo-sources  # for Flatpak
```

Create the bench files:
```
crates/lushtext-core/benches/
├── palette.rs      # fuzzy_score, search_items, FileIndex::rebuild
└── file_tree.rs    # scan_directory at various directory sizes
```

---

## 2. Benchmark Targets {#2-targets}

Priority order based on how often each function is called and its impact on UX:

| Priority | Function | Hot path | Why benchmark |
|----------|----------|----------|---------------|
| P0 | `fuzzy_score_chars` | Every keystroke × every file | Determines if debounce is needed and at what threshold |
| P0 | `search_items` (end-to-end) | Every keystroke | Total search latency = scoring + sort + truncate |
| P1 | `FileIndex::rebuild` | On workspace change | Determines if rebuild coalescing is needed |
| P1 | `scan_directory` | On tree node expand | Determines if directory entry cap is needed |
| P2 | `buffer.set_text` | On file open | Requires GTK init — harder to bench but critical for threshold calibration |

P0 targets affect every search interaction. P1 affects periodic operations. P2 requires special setup.

---

## 3. Example Benchmarks {#3-examples}

### `benches/palette.rs`

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use lushtext_core::services::palette::{fuzzy_score, FileIndex};
use lushtext_core::model::palette::SearchMode;
use std::path::PathBuf;
use tempfile::TempDir;

/// Benchmark fuzzy_score with varying candidate lengths.
fn bench_fuzzy_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("fuzzy_score");

    let candidates = [
        ("short", "main.rs"),
        ("medium", "workspace_section_mod.rs"),
        ("long", "very_long_filename_with_multiple_separators_and_extensions.test.tsx"),
    ];

    for (name, candidate) in &candidates {
        group.bench_with_input(
            BenchmarkId::new("match", name),
            candidate,
            |b, candidate| {
                b.iter(|| fuzzy_score(black_box("mrs"), black_box(candidate)));
            },
        );
    }

    group.bench_function("no_match", |b| {
        b.iter(|| fuzzy_score(black_box("xyz"), black_box("main.rs")));
    });

    group.bench_function("empty_query", |b| {
        b.iter(|| fuzzy_score(black_box(""), black_box("main.rs")));
    });

    group.finish();
}

/// Benchmark search_items with varying index sizes.
fn bench_search_items(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_items");

    for size in [100, 1_000, 10_000, 50_000, 100_000] {
        // Create a synthetic index
        let files: Vec<_> = (0..size)
            .map(|i| lushtext_core::model::palette::IndexedFile {
                path: PathBuf::from(format!("/workspace/src/module_{}/file_{}.rs", i / 100, i)),
                name: format!("file_{}.rs", i),
                workspace_root: PathBuf::from("/workspace"),
            })
            .collect();

        let index = FileIndex::from_files(files);

        group.bench_with_input(
            BenchmarkId::new("query_3char", size),
            &index,
            |b, index| {
                b.iter(|| {
                    lushtext_core::services::palette::search_all(
                        black_box(index),
                        black_box("fil"),
                        SearchMode::Files,
                        50,
                    )
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("empty_query", size),
            &index,
            |b, index| {
                b.iter(|| {
                    lushtext_core::services::palette::search_all(
                        black_box(index),
                        black_box(""),
                        SearchMode::Files,
                        50,
                    )
                });
            },
        );
    }

    group.finish();
}

/// Benchmark FileIndex::rebuild on a real directory tree.
fn bench_file_index_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_index_rebuild");

    for file_count in [100, 1_000, 5_000] {
        let dir = TempDir::new().unwrap();

        // Create a tree with ~file_count files across 10-level depth
        create_synthetic_tree(dir.path(), file_count, 10);

        group.bench_with_input(
            BenchmarkId::new("rebuild", file_count),
            &dir,
            |b, dir| {
                b.iter(|| FileIndex::rebuild(black_box(&[dir.path().to_path_buf()])));
            },
        );
    }

    group.finish();
}

fn create_synthetic_tree(root: &std::path::Path, total_files: usize, max_depth: usize) {
    let files_per_dir = 10;
    let dirs_needed = total_files / files_per_dir;
    let mut created = 0;

    fn create_level(
        path: &std::path::Path,
        depth: usize,
        max_depth: usize,
        files_per_dir: usize,
        created: &mut usize,
        target: usize,
    ) {
        if depth >= max_depth || *created >= target {
            return;
        }
        for i in 0..files_per_dir {
            if *created >= target {
                break;
            }
            std::fs::write(path.join(format!("file_{}.rs", i)), "fn main() {}").unwrap();
            *created += 1;
        }
        // Create subdirectories
        for i in 0..3 {
            if *created >= target {
                break;
            }
            let sub = path.join(format!("dir_{}", i));
            std::fs::create_dir_all(&sub).unwrap();
            create_level(&sub, depth + 1, max_depth, files_per_dir, created, target);
        }
    }

    create_level(root, 0, max_depth, files_per_dir, &mut created, total_files);
}

criterion_group!(benches, bench_fuzzy_score, bench_search_items, bench_file_index_rebuild);
criterion_main!(benches);
```

Note: The `FileIndex::from_files` constructor doesn't exist yet — it would need to be added as a test/bench helper:

```rust
impl FileIndex {
    /// Create an index from pre-built file list (for benchmarks and tests).
    #[cfg(any(test, feature = "bench"))]
    pub fn from_files(files: Vec<IndexedFile>) -> Self {
        Self { files }
    }
}
```

Alternatively, expose `files` as `pub(crate)` and construct directly in the bench.

### `benches/file_tree.rs`

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use lushtext_core::services::file_tree;
use tempfile::TempDir;

fn bench_scan_directory(c: &mut Criterion) {
    let mut group = c.benchmark_group("scan_directory");

    for file_count in [10, 100, 1_000, 5_000] {
        let dir = TempDir::new().unwrap();
        for i in 0..file_count {
            std::fs::write(dir.path().join(format!("file_{:05}.rs", i)), "").unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("flat_dir", file_count),
            &dir,
            |b, dir| {
                b.iter(|| file_tree::scan_directory(black_box(dir.path())));
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_scan_directory);
criterion_main!(benches);
```

---

## 4. CI Regression Detection {#4-ci}

### GitHub Actions workflow addition

Add to `.github/workflows/ci.yml`:

```yaml
  bench:
    name: Benchmarks (regression check)
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4
      - name: Install system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run benchmarks
        run: cargo bench --package lushtext-core -- --output-format bencher | tee bench_output.txt
      - name: Check for regressions
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: bench_output.txt
          alert-threshold: '120%'
          fail-on-alert: true
          comment-on-alert: true
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

This fails the PR if any benchmark regresses by more than 20%. The `github-action-benchmark` action stores historical data in a GitHub Pages branch and adds comparison comments to PRs.

### Local regression check

```bash
# Save baseline
cargo bench --package lushtext-core -- --save-baseline main

# Switch to feature branch, run comparison
cargo bench --package lushtext-core -- --baseline main
```

criterion generates HTML reports in `target/criterion/` with comparison charts.

---

## 5. Expected Baselines {#5-baselines}

These are approximate numbers on a 2023 laptop (Ryzen 7, NVMe SSD). Use them as sanity checks — if your numbers are 10x worse, something is wrong.

| Benchmark | Input size | Expected range | Concerning if |
|-----------|-----------|---------------|---------------|
| `fuzzy_score` (match) | 15-char candidate | 50–150 ns | > 500 ns |
| `fuzzy_score` (no match) | 15-char candidate | 30–100 ns | > 300 ns |
| `search_items` | 1,000 files | 0.1–0.3 ms | > 1 ms |
| `search_items` | 10,000 files | 1–3 ms | > 10 ms |
| `search_items` | 100,000 files | 10–30 ms | > 50 ms |
| `FileIndex::rebuild` | 1,000 files | 5–15 ms | > 50 ms |
| `FileIndex::rebuild` | 10,000 files | 50–150 ms | > 500 ms |
| `scan_directory` | 1,000 entries | 0.5–2 ms | > 5 ms |
| `scan_directory` | 5,000 entries | 2–10 ms | > 20 ms |

The `search_items` at 100k files baseline of 10–30ms is why the debounce threshold matters: at 150ms debounce, the search runs at most ~7 times per second, and each run at 30ms uses ~20% of the frame budget. Without debounce, 10 keypresses/second × 30ms = 300ms/second of main-thread time consumed by search alone.
