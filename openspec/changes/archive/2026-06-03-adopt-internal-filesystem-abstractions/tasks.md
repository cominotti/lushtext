## 1. Inventory And Dependency Setup

- [x] 1.1 Capture a baseline inventory of every direct filesystem access in production code, tests, benches, rules, skills, and agent guidance.
- [x] 1.2 Classify each hit by operation family: read, metadata, canonical identity, traversal, create, durable write, rename, remove, sidecar, fixture setup, fixture assertion, or backend-only exception.
- [x] 1.3 Add `rustix` to workspace dependencies with the filesystem/std features needed for descriptor-relative filesystem operations and error conversion.
- [x] 1.4 Refresh `Cargo.lock`, cargo-hakari metadata, and Flatpak cargo sources after adding `rustix`.
- [x] 1.5 Add or identify a deterministic filesystem-boundary audit command that can run in validation and report disallowed hits with file and line numbers.
- [x] 1.6 Cover Cargo build scripts with a build-time filesystem boundary so generated-code setup does not preserve raw filesystem leftovers.

## 2. Filesystem Boundary Structure

- [x] 2.1 Create `crates/lushtext-core/src/services/filesystem/` and export it from the services module.
- [x] 2.2 Add operation-family modules for read, metadata, write, tree, mutate, sidecar, fixture or test support, and private backend implementation.
- [x] 2.3 Define app-facing value types for file snapshots, file facts, canonical identity, directory entries, scan policies, write labels, write outcomes, and mutation outcomes.
- [x] 2.4 Define app-facing error types or conversions that keep backend-specific `rustix` errno details out of ordinary callers.
- [x] 2.5 Implement the private backend adapter with `rustix` for descriptor-relative open/stat/read-directory, rename/unlink/mkdir, sync, symlink-aware metadata, and permission/ownership operations where applicable.
- [x] 2.6 Keep any unavoidable `std::fs`, Unix extension, or filesystem `libc` interop private to documented backend modules.
- [x] 2.7 Add focused unit tests for the new filesystem boundary types and backend conversions.

## 3. Durable Writes And Editor I/O

- [x] 3.1 Move or wrap public durable write entry points under `services::filesystem::write`.
- [x] 3.2 Preserve the existing atomic write ordering, temp metadata handling, parent-directory sync, before/after-rename classification, and stable write coordination behavior.
- [x] 3.3 Migrate `editor_io` load, metadata, canonical identity, mtime, save, symlink-save, and file-size paths to the filesystem boundary.
- [x] 3.4 Migrate UI window/editor callers that currently perform direct metadata or canonical path probes during save, load, Save As, external change handling, and local-history availability checks.
- [x] 3.5 Update durable write tests so they verify the filesystem boundary API rather than importing durable implementation helpers directly.

## 4. State Stores, Drafts, Sidecars, And History

- [x] 4.1 Migrate `json_store` reads and streaming durable writes to the filesystem boundary.
- [x] 4.2 Migrate `draft_service` metadata checks, draft reads, orphan cleanup, sparse draft tests, manifest paths, and draft writes to the filesystem boundary.
- [x] 4.3 Migrate `session_service` persistence and tests to the filesystem boundary.
- [x] 4.4 Migrate bookmark, document-note, workspace-note, note-storage, and saved-search persistence to the filesystem boundary.
- [x] 4.5 Migrate local-history snapshot capture, list, load, prune, lineage migration, and tests to the filesystem boundary.
- [x] 4.6 Migrate search backup and Replace All undo journal persistence to the filesystem boundary.
- [x] 4.7 Verify sidecar move/remove helpers preserve existing sidecar identity and migration behavior after in-app renames and deletes.

## 5. Workspace, Search, Palette, And UI File Operations

- [x] 5.1 Migrate `file_tree` scanning, bounded lookahead, empty-folder detection, and directory entry metadata to the filesystem tree helpers.
- [x] 5.2 Migrate command-palette indexing, workspace-root canonicalization, symlink-loop handling, ignored-path handling, and tests to filesystem tree/read helpers.
- [x] 5.3 Migrate content search traversal, file reads, metadata checks, unreadable-file handling, and tests to filesystem read/tree helpers.
- [x] 5.4 Migrate content search Replace All reads, metadata checks, writes, rollback, undo, and permission-preservation tests to filesystem read/write helpers.
- [x] 5.5 Migrate file peek bounded reads and metadata snapshots to filesystem read helpers.
- [x] 5.6 Migrate workspace manager persistence and workspace watch setup inputs to filesystem helpers while preserving Gio/notify watcher responsibilities.
- [x] 5.7 Migrate sidebar workspace-section create, inline rename, cancel cleanup, delete, remove-directory-tree, and parent sync operations to filesystem mutation helpers.
- [x] 5.8 Migrate window document path tracking and rename/delete follow-up logic away from direct canonicalization where filesystem helpers provide the same identity.

## 6. Tests, Properties, Fuzzing, And Benches

- [x] 6.1 Add fixture filesystem helpers for writing text, writing bytes, reading text, asserting content, creating directories, creating sparse files, symlinks, permission changes, renames, removals, and cleanup.
- [x] 6.2 Migrate `crates/lushtext-core` unit tests, property tests, fuzz corpus replay, and service tests to fixture helpers.
- [x] 6.3 Migrate `crates/lushtext` integration tests to fixture helpers.
- [x] 6.4 Migrate widget tests to fixture helpers in batches: app activation, editor page, window, sidebar, workspace section, command palette, search panel, markdown preview, status bar, and common harness setup.
- [x] 6.5 Migrate benchmarks to fixture helpers and filesystem boundary reads so benchmarks do not preserve direct `std::fs` examples.
- [x] 6.6 Keep backend-only fixture exceptions documented and limited to fixture/helper modules.
- [x] 6.7 Run migrated tests after each major batch to catch behavior regressions before the final audit.

## 7. Guidance, Rules, And Skills

- [x] 7.1 Update root `AGENTS.md` and relevant nested `AGENTS.md` files to describe `services::filesystem` as the required filesystem boundary.
- [x] 7.2 Update `.agents/rules/rust.md` with the direct filesystem access ban, approved exception modules, readability expectations, and audit command.
- [x] 7.3 Update `.agents/rules/build.md` or documentation rules if the validation stack gains a new filesystem audit target.
- [x] 7.4 Update `data-safety` skill so file I/O and persistence reviews require the filesystem boundary and durable write entry points.
- [x] 7.5 Update `gtk-perf-review`, `gtk-perf-scale`, `gtk-responsiveness`, and `gtk-perf-rust-optimize` skills so performance and responsiveness reviews flag direct raw filesystem access.
- [x] 7.6 Update `rust-hex-arch` skill so architecture reviews treat filesystem access as an adapter boundary and discourage raw calls in domain/UI modules.
- [x] 7.7 Update `rust-comments` skill so comments focus on backend invariants, descriptor lifetimes, and durability ordering rather than obvious wrapper calls.
- [x] 7.8 Run agent documentation validation after all rule and skill edits.

## 8. No-Leftovers Audit And Cleanup

- [x] 8.1 Remove obsolete public durable-write imports, re-exports, and caller-facing helpers that bypass the filesystem boundary.
- [x] 8.2 Run the no-leftovers audit for production code and fix every disallowed hit outside approved backend modules.
- [x] 8.3 Run the no-leftovers audit for tests and benches and fix every disallowed hit outside approved fixture/backend modules.
- [x] 8.4 Run the no-leftovers audit for `AGENTS.md`, nested guidance, `.agents/rules`, and `.agents/skills` to ensure guidance does not preserve stale direct-call examples.
- [x] 8.5 Confirm direct `rustix` imports appear only in approved private backend modules.
- [x] 8.6 Confirm direct filesystem `libc` calls, if any remain, appear only in documented backend metadata-copy helpers.
- [x] 8.7 Confirm direct `Path::canonicalize` and equivalent direct path probes are either removed or explicitly justified by the filesystem boundary allowlist.

## 9. Validation And Completion

- [x] 9.1 Run `cargo fmt --all -- --check`.
- [x] 9.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 9.3 Run `cargo test --workspace`.
- [x] 9.4 Run property-test and fuzz-corpus validation for filesystem-sensitive suites.
- [x] 9.5 Run widget tests through the project headless harness.
- [x] 9.6 Run dependency/package refresh validation for cargo-hakari and Flatpak cargo sources.
- [x] 9.7 Run `make check-agent-docs` or the documented equivalent after rule and skill updates.
- [x] 9.8 Run `openspec validate adopt-internal-filesystem-abstractions --strict`.
- [x] 9.9 Run `openspec validate --changes --strict`.
- [x] 9.10 Record the final recommendation and validation evidence in the implementation summary.
