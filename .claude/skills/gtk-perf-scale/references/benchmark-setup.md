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
name = "benchmarks"
harness = false
```

After adding, run:
```bash
cargo hakari generate
make cargo-sources  # for Flatpak
```

All benchmarks live in a single file:
```
crates/lushtext-core/benches/
└── benchmarks.rs   # All benchmark groups (fuzzy_score, scan_directory, file_index, etc.)
```

---

## 2. Benchmark Targets {#2-targets}

Priority order based on how often each function is called and its impact on UX:

| Priority | Function | Hot path | Why benchmark |
|----------|----------|----------|---------------|
| P0 | `fuzzy_score` | Every keystroke × every file | Determines if debounce is needed and at what threshold |
| P0 | `search_items` (end-to-end) | Every keystroke | Total search latency = scoring + sort + truncate |
| P1 | `FileIndex::rebuild` | On workspace change | Determines if rebuild coalescing is needed |
| P1 | `scan_directory` | On tree node expand | Determines if directory entry cap is needed |
| P2 | `buffer.set_text` | On file open | Requires GTK init — harder to bench but critical for threshold calibration |

P0 targets affect every search interaction. P1 affects periodic operations. P2 requires special setup.

---

## 3. Example Benchmarks {#3-examples}

### `benches/benchmarks.rs` (key patterns)

All benchmarks live in a single file. Synthetic indexes use `FileIndex::from(Vec<IndexedFile>)` (the `From` trait impl). The `IndexedFile` struct requires an `Arc<PathBuf>` for `workspace_root`:

```rust
use std::sync::Arc;
use lushtext_core::model::palette::IndexedFile;
use lushtext_core::services::palette::FileIndex;

// Construct synthetic indexes for benchmarks:
let root = Arc::new(PathBuf::from("/workspace"));
let files: Vec<_> = (0..size)
    .map(|i| IndexedFile {
        path: PathBuf::from(format!("/workspace/src/file_{}.rs", i)),
        name: format!("file_{}.rs", i),
        workspace_root: Arc::clone(&root),
    })
    .collect();
let index = FileIndex::from(files);

// search_all takes &FileIndex:
palette::search_all(&index, "fil", SearchMode::Files, 50);
```

Current benchmark groups: `fuzzy_score`, `file_index_search`, `file_index_rebuild`, `file_index_incremental`, `search_all`, `scan_directory`, `json_persistence`, `file_size_classify`, `utf8_validation`.

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
