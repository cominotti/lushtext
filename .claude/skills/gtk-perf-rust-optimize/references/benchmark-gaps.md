# Benchmark Gaps

Known performance-sensitive code paths in LushText that lack Criterion benchmarks. Use this reference to identify missing coverage when reviewing changes to hot paths.

## Current Coverage

The benchmark file at `crates/lushtext-core/benches/benchmarks.rs` covers:

| Group | Functions | Max Input Size |
|-------|-----------|----------------|
| `fuzzy_score` | `fuzzy_score()` single-call | 7 named cases |
| `file_index_search` | `FileIndex::search()` | 100k files |
| `file_index_rebuild` | `FileIndex::rebuild()` | 5k files on tmpfs |
| `search_all` | `palette::search_all()` | 10k files |
| `scan_directory` | `file_tree::scan_directory()` | 5k entries |
| `json_persistence` | `workspace_manager`, `json_store` | 10 workspaces, 50 tabs |
| `file_size_classify` | `FileSizeCheck::classify()` | 5 size buckets |

## Missing Benchmarks

### Priority 1: File Load Path (validates simdutf8 optimization)

The actual file-load hot path — `std::fs::read` + UTF-8 validation — is unbenchmarked. This is critical for validating the simdutf8 threshold change (extending SIMD to all file sizes).

```rust
fn bench_file_load_utf8(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_load_utf8");

    for size_mb in [1, 5, 10, 50] {
        let content = "a".repeat(size_mb * 1_000_000);
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, &content).unwrap();

        group.bench_with_input(
            BenchmarkId::new("read_to_string", format!("{}MB", size_mb)),
            &path,
            |b, path| {
                b.iter(|| {
                    let _ = std::fs::read_to_string(path).unwrap();
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("read_simdutf8", format!("{}MB", size_mb)),
            &path,
            |b, path| {
                b.iter(|| {
                    let bytes = std::fs::read(path).unwrap();
                    let _ = simdutf8::basic::from_utf8(&bytes).unwrap();
                    // SAFETY: validated above
                    let _ = unsafe { String::from_utf8_unchecked(bytes) };
                });
            },
        );
    }
    group.finish();
}
```

**Expected baselines**: `read_simdutf8` should be 2-8x faster than `read_to_string` for the validation step, though the I/O dominates at large sizes.

### Priority 2: scan_directory at 10k (the MAX_DIR_ENTRIES cap)

Current benchmarks go up to 5k entries. The `MAX_DIR_ENTRIES` cap is 10,000. The boundary case should be benchmarked.

```rust
// Add to existing bench_scan_directory group:
(10_000, 30),  // 10k entries, 30 samples (I/O bound)
```

**Expected baseline**: ~15-25ms for 10k entries on tmpfs (linear scaling from the 5k benchmark).

### Priority 3: Sort Key Allocation

The `scan_directory` sort uses `sort_by_cached_key` with `to_string_lossy().to_lowercase()`. At 10k entries, this creates 10k String allocations for sort keys. Benchmark to determine if the allocation cost is significant vs. the comparison cost.

```rust
fn bench_sort_key_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_key");

    for count in [100, 1_000, 5_000, 10_000] {
        let entries: Vec<(std::path::PathBuf, bool)> = (0..count)
            .map(|i| (std::path::PathBuf::from(format!("file_{:05}.txt", i)), false))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("cached_key_lowercase", count),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let mut e = entries.clone();
                    e.sort_by_cached_key(|(path, is_dir)| {
                        let name = path.file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        (std::cmp::Reverse(*is_dir), name)
                    });
                });
            },
        );
    }
    group.finish();
}
```

### Priority 4: File Save Path

The save path's `buffer.text().to_string()` + `std::fs::write` is unbenchmarked. Since the GtkTextBuffer part requires GTK initialization, benchmark only the Rust portion:

```rust
fn bench_file_save(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_save");
    group.sample_size(20);

    for size_mb in [1, 5, 10, 50] {
        let content = "a".repeat(size_mb * 1_000_000);
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.txt");

        group.bench_with_input(
            BenchmarkId::new("fs_write", format!("{}MB", size_mb)),
            &(&content, &path),
            |b, (content, path)| {
                b.iter(|| {
                    std::fs::write(path, content.as_bytes()).unwrap();
                });
            },
        );
    }
    group.finish();
}
```

### Priority 5: SearchHit Collection

The heap extraction and conversion path in `search_items` → `rebuild_results`. Isolate the collection overhead:

```rust
fn bench_search_result_collection(c: &mut Criterion) {
    // Build a pre-scored heap of 50 results
    let mut heap = std::collections::BinaryHeap::with_capacity(51);
    for i in 0..50u32 {
        heap.push(std::cmp::Reverse(i));
    }

    c.bench_function("heap_extract_50", |b| {
        b.iter(|| {
            let h = heap.clone();
            let mut results: Vec<_> = h.into_sorted_vec();
            results.reverse();
            results
        });
    });
}
```

**Expected baseline**: <1us for 50 elements. This confirms the [CONSIDER] severity — the allocation is negligible at this scale.

## Adding New Benchmarks

When adding a benchmark for a newly identified hot path:

1. Add the benchmark function to `crates/lushtext-core/benches/benchmarks.rs`
2. Register it in the `criterion_group!` macro
3. Use `sample_size(20-30)` for I/O-bound benchmarks to keep wall-clock time reasonable
4. Use `BenchmarkId::new(name, parameter)` for parameterized benchmarks
5. Run `make bench` to verify, then `make bench-baseline` to save the baseline
6. After optimization, run `make bench-compare` to measure improvement

All benchmarked code must be GTK-free (no display server dependency). Use `tempfile::TempDir` for filesystem benchmarks.
