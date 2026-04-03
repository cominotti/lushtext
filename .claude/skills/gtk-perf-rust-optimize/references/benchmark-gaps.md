# Benchmark Gaps

Known performance-sensitive code paths in LushText that lack Criterion benchmarks. Use this reference to identify missing coverage when reviewing changes to hot paths.

## Current Coverage

The benchmark file at `crates/lushtext-core/benches/benchmarks.rs` (single file, all groups) covers:

| Group | Functions | Max Input Size |
|-------|-----------|----------------|
| `fuzzy_score` | `fuzzy_score()` single-call | 7 named cases |
| `file_index_search` | `FileIndex::search()` | 100k files |
| `file_index_rebuild` | `FileIndex::rebuild()` | 5k files on tmpfs |
| `file_index_incremental` | `add_file`, `remove_path`, `rename_path` | 100k files |
| `search_all` | `palette::search_all()` | 10k files |
| `scan_directory` | `file_tree::scan_directory()` | 10k entries |
| `json_persistence` | `workspace_manager`, `json_store` | 10 workspaces, 50 tabs |
| `file_size_classify` | `FileSizeCheck::classify()` | 5 size buckets |
| `utf8_validation` | `read_to_string` vs `read` + `simdutf8` | 1/5/10/50 MB |

## Missing Benchmarks

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

## Adding New Benchmarks

When adding a benchmark for a newly identified hot path:

1. Add the benchmark function to `crates/lushtext-core/benches/benchmarks.rs`
2. Register it in the `criterion_group!` macro
3. Use `sample_size(20-30)` for I/O-bound benchmarks to keep wall-clock time reasonable
4. Use `BenchmarkId::new(name, parameter)` for parameterized benchmarks
5. Run `make bench` to verify, then `make bench-baseline` to save the baseline
6. After optimization, run `make bench-compare` to measure improvement

All benchmarked code must be GTK-free (no display server dependency). Use `tempfile::TempDir` for filesystem benchmarks.
