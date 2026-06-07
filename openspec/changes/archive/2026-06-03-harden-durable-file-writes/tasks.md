## 1. Metadata-preservation primitive in `durable_write`

- [x] 1.1 Decide xattr access path (thin `libc` bindings vs. a vetted `xattr` crate); if a crate is added, update `[workspace.dependencies]`, consume with `{ workspace = true }`, run `cargo hakari generate`, and run `make cargo-sources`.
- [x] 1.2 Add `#[cfg(unix)] fn preserve_destination_metadata(dest: &Path, temp: &File)` that returns early when `dest` does not exist, else `fchmod`s the temp fd to the destination mode (hard error on failure).
- [x] 1.3 Extend it to `fchown` the temp fd to the destination uid/gid best-effort, ignoring `EPERM`.
- [x] 1.4 Extend it to copy extended attributes (carrying `system.posix_acl_access` ACLs and `security.selinux` label plus `user.*`) from destination to temp fd best-effort, ignoring `ENOTSUP`/`EPERM`/`EACCES`.
- [x] 1.5 Add a `#[cfg(not(unix))]` no-op variant so non-Unix builds keep compiling.

## 2. Before/after-rename failure classification in `durable_write`

- [x] 2.1 Promote a shared error type that distinguishes a before-rename failure (temp write/flush/sync/rename) from an after-rename failure (parent-dir `fsync` after the rename landed).
- [x] 2.2 Wire `atomic_write_bytes` to call `preserve_destination_metadata` before the rename and to return the classified error type; keep temp-file cleanup on before-rename failures.
- [x] 2.3 Refactor `content_search/replace.rs` to use the shared classification and shared metadata copy instead of its private `AtomicWriteError`/`atomic_write` copy, preserving its existing journal and rollback behavior.

## 3. Adopt the primitive in the remaining atomic write paths

- [x] 3.1 Route `editor_io::write_bytes_to_path` through the shared metadata copy before rename and the shared classification.
- [x] 3.2 Route `json_store::save` and `draft_service::write_draft` through the shared metadata copy / classification (preserving metadata only when overwriting an existing file).
- [x] 3.3 Confirm `ui/editor_page/imp.rs` style-scheme write (`atomic_write_bytes`) and the sidebar new-file/new-dir create paths inherit the correct behavior (new files keep default perms).

## 4. Honest save-failure signalling in the editor

- [x] 4.1 Add a `SaveError::DurabilityUnconfirmed` (after-rename) variant distinct from the pre-rename `WriteTemp`/`Finalize` cases in `editor_io.rs`.
- [x] 4.2 In `ui/editor_page/load_save.rs`, surface the after-rename case as a durability warning while keeping the document marked modified; keep the pre-rename case reporting unwritten changes and modified.
- [x] 4.3 Confirm the `fsync`-failure path is never swallowed anywhere and add a code comment documenting the fsyncgate semantics (failed sync always surfaced).

## 5. Tests

- [x] 5.1 `durable_write` unit tests: overwrite preserves `0600` mode and the executable bit; new-file path uses default perms.
- [x] 5.2 `durable_write` xattr round-trip test, skipping when the temp filesystem returns `ENOTSUP`.
- [x] 5.3 `durable_write` test that a simulated parent-dir-sync failure yields an after-rename error and that a temp-sync failure yields a before-rename error with the destination untouched.
- [x] 5.4 `editor_io` tests: saving over a `0600`/executable file preserves mode; the two failure classes map to distinct `SaveError` variants.
- [x] 5.5 Replace All tests: rewriting and undoing a `0600`/executable file preserves its mode (`content_search/replace.rs`).
- [x] 5.6 Property test (under the `property-tests` lane) over random mode bits asserting preservation across an overwrite round trip.

## 6. Documentation

- [x] 6.1 Update `.agents/rules/rust.md` Background I/O section to record the metadata-preservation contract alongside the existing durable-write note.
- [x] 6.2 Update `AGENTS.md` (durability/services design notes) and `README.md` if save behavior is described there.
- [x] 6.3 Update any durability docs under `docs/` to describe metadata preservation and the before/after-rename distinction.

## 7. Verification

- [x] 7.1 Run `make check` (clippy + fmt) clean.
- [x] 7.2 Run `make test` and `make test-prop` clean; fix any pre-existing blocker uncovered rather than deferring it.
- [x] 7.3 Manually verify via `make run`: edit and save an executable script and a `0600` file, then confirm on disk (`ls -l`, `getfacl`) that mode/ACL survived, watching stderr for GTK/durability warnings.
