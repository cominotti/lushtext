---
title: 'Fix file index truncation by skipping well-known build/dependency directories'
type: 'feature'
created: '2026-04-05'
status: 'done'
baseline_commit: '7534ad6'
context: ['.agents/AGENTS.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The command palette file index scans all non-hidden directories recursively, including massive build/dependency directories like `node_modules/` and `target/`. Workspaces with these directories easily exceed the 100k file cap (user sees 550k files), causing truncation and an incomplete command palette search.

**Approach:** Add a hardcoded set of well-known build/dependency directory names to skip during `collect_files_recursive` in `palette.rs`. This is a palette-index-only change — the sidebar file tree continues to show all directories. Keep the 100k safety cap.

## Boundaries & Constraints

**Always:** Only skip directories by exact name match (not prefix/glob). The skip list applies only to palette file indexing, not sidebar `scan_directory`. Keep `MAX_INDEXED_FILES` as a safety net.

**Ask First:** Adding new directory names beyond the initial list. Any change to `MAX_INDEXED_FILES` value.

**Never:** Do not add `.gitignore` parsing or the `ignore` crate. Do not make the skip list user-configurable (future work). Do not modify `file_tree.rs` behavior.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Workspace with `node_modules/` | Root containing `src/` (10 files) + `node_modules/` (50k files) | Index contains only `src/` files (~10) | N/A |
| Nested ignored dir | `project/subdir/target/` (deep nesting) | `target/` subtree skipped at any depth | N/A |
| Legitimate dir with same name | User has a `target/` dir they want indexed | Skipped — known trade-off matching VS Code defaults | N/A |
| All roots ignored | Root IS `node_modules/` itself | Root is always scanned (skip applies to children only) | N/A |

</frozen-after-approval>

## Code Map

- `crates/lushtext-core/src/services/palette.rs` -- `collect_files_recursive` + `IGNORED_INDEX_DIRS` constant + unit tests

## Tasks & Acceptance

**Execution:**
- [x] `crates/lushtext-core/src/services/palette.rs` -- Add `IGNORED_INDEX_DIRS: &[&str]` constant with `["node_modules", "target", "__pycache__", "venv", "vendor"]` and add directory name check in `collect_files_recursive` before recursive call
- [x] `crates/lushtext-core/src/services/palette.rs` (tests) -- Add regression tests: `test_file_index_skips_ignored_dirs`, `test_file_index_skips_nested_ignored_dirs`, `test_file_index_includes_non_ignored_dirs`, `test_file_index_ignored_dirs_reduce_count`

**Acceptance Criteria:**
- Given a workspace root containing a `node_modules/` directory with files, when `FileIndex::rebuild` is called, then no files from `node_modules/` appear in the index
- Given a workspace root containing only non-ignored directories like `src/`, when `FileIndex::rebuild` is called, then all files are indexed normally
- Given a `target/` directory nested inside `project/subdir/`, when `FileIndex::rebuild` is called, then the nested `target/` subtree is skipped
- Given existing tests, when `make test` is run, then all existing + new tests pass

## Design Notes

The skip list uses exact directory name comparison via `Path::file_name()`, checked before the recursive call in `collect_files_recursive`. This is O(k) per directory where k is the skip list size (~7 entries) — negligible compared to the I/O cost of scanning.

The list intentionally excludes ambiguous names like `build`, `dist`, `out`, `bin` which are common as legitimate source directories. Conservative defaults that cover the highest-impact cases (node_modules alone is typically 80%+ of excess files).

## Verification

**Commands:**
- `make test` -- expected: all tests pass including new regression tests
- `make check` -- expected: no clippy warnings or fmt issues

## Suggested Review Order

- Skip list constant — 5 unambiguous build/dependency directory names
  [`palette.rs:178`](../../crates/lushtext-core/src/services/palette.rs#L178)

- Helper function — exact name match via `Path::file_name()`
  [`palette.rs:187`](../../crates/lushtext-core/src/services/palette.rs#L187)

- Integration point — check before recursive call, inside `if is_dir` branch
  [`palette.rs:236`](../../crates/lushtext-core/src/services/palette.rs#L236)

- Regression: all ignored dir names are skipped
  [`palette.rs:1208`](../../crates/lushtext-core/src/services/palette.rs#L1208)

- Regression: nested ignored dirs are skipped at any depth
  [`palette.rs:1232`](../../crates/lushtext-core/src/services/palette.rs#L1232)

- Regression: non-ignored dirs (build, dist, out) are still indexed
  [`palette.rs:1249`](../../crates/lushtext-core/src/services/palette.rs#L1249)

- Regression: ignored dirs reduce file count, avoiding cap truncation
  [`palette.rs:1267`](../../crates/lushtext-core/src/services/palette.rs#L1267)

- Regression: workspace root named as ignored dir is still scanned
  [`palette.rs:1294`](../../crates/lushtext-core/src/services/palette.rs#L1294)

- Documentation update
  [`AGENTS.md:100`](../../.agents/AGENTS.md#L100)
