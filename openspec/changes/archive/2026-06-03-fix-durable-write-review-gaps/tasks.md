## 1. Durable-write primitive closure

- [x] 1.1 Split metadata probing from metadata application so atomic writes can know whether the destination exists, which mode to use at temp creation, and which path supplies metadata.
- [x] 1.2 Create overwrite temp files with permissions no wider than the existing destination's standard mode bits; keep new-file writes on platform default permissions.
- [x] 1.3 Reorder atomic writes so content write/flush, metadata application, and the final temp-file `sync_all()` all complete before `rename()`.
- [x] 1.4 Keep `DurableWriteError::BeforeRename` for every content, metadata, or final temp-sync failure before `rename()`, and keep `AfterRename` only for parent-directory sync failures after `rename()`.
- [x] 1.5 Add a streaming durable-write helper that accepts a writer closure and shares the same metadata, final-sync, rename, parent-sync, cleanup, and failure-classification behavior as `atomic_write_bytes_classified`.
- [x] 1.6 Route `json_store::save` through streaming serialization with `serde_json::to_writer_pretty` instead of materializing the whole JSON document first.
- [x] 1.7 Confirm byte-slice callers (`draft_service`, style-scheme writes, ordinary editor bytes) still use the shared helper and inherit the corrected ordering.

## 2. Stable write coordination

- [x] 2.1 Implement a stable write-target identity resolver: canonical path for existing targets and symlink targets, canonical parent plus file name for missing targets.
- [x] 2.2 Replace destination-inode `FileWriteLock` coordination with a stable target guard that does not require opening the destination read-write.
- [x] 2.3 Wire editor save, Save As, Replace All apply, and Replace All undo through the same stable target guard before reading or writing target bytes.
- [x] 2.4 Add regression coverage proving concurrent save vs Replace All serializes for the same canonical target.
- [x] 2.5 Add regression coverage proving symlink path and resolved target path share one coordination guard.

## 3. Symlink-backed save semantics

- [x] 3.1 Add an editor save-target helper that chooses the canonical loaded target for file-backed saves while preserving the user-visible display path.
- [x] 3.2 Update ordinary save so opening `link.txt` and saving writes the resolved target and does not replace `link.txt` with a regular file.
- [x] 3.3 Update Save As so selecting an existing symlink writes the resolved target or fails before replacement when the target cannot be resolved.
- [x] 3.4 Keep duplicate-tab detection and open-path bookkeeping coherent for display path plus canonical target identity.
- [x] 3.5 Add symlink save tests for ordinary save, Save As, and duplicate target handling.

## 4. Durable copy fallback semantics

- [x] 4.1 Add an atomic-write path that can preserve metadata from an explicit source path rather than from the existing destination.
- [x] 4.2 Update `copy_file_durable` so cross-filesystem rename fallback writes source bytes and source metadata to the destination before removing the source.
- [x] 4.3 Ensure source removal happens only after destination content, source-derived metadata, destination rename, and destination parent sync all succeed.
- [x] 4.4 Add tests for copying a `0644` source over an existing `0600` destination and for preserving supported source xattrs or ACLs.

## 5. Replace All memory, journal, and validation

- [x] 5.1 Add explicit Replace All caps: `MAX_REPLACE_FILE_BYTES = 10 * 1024 * 1024` and `MAX_REPLACE_UNDO_BYTES = 64 * 1024 * 1024`.
- [x] 5.2 Skip and report any target file over the per-file cap before reading the whole file into memory.
- [x] 5.3 Track total undo payload before each write and skip/report any file that would exceed the undo cap before mutating it.
- [x] 5.4 Replace the full `Vec<String>` line-splitting rewrite with a bounded single-pass output builder that still validates stale search results.
- [x] 5.5 Use the established `simdutf8` validation path for Replace All target bytes before building replacement text.
- [x] 5.6 Replace whole-backup JSON rewrites with a per-file durable journal directory whose entry for a file is synced before that file is modified.
- [x] 5.7 Preserve cancellation rollback and undo retry behavior with the new per-file journal format.
- [x] 5.8 Cleanup both the new journal directory and the legacy `replace-backup.json` on startup, search-panel close, and successful undo.

## 6. Load/save adapter and draft preload hardening

- [x] 6.1 Choose save snapshot strategy from the live buffer size; route unknown-size, large untitled, and grown-in-memory buffers through chunked snapshotting.
- [x] 6.2 Add a per-load generation or per-load token identity so older async load completions cannot apply after a newer load starts.
- [x] 6.3 Ensure cancelled loads stay cancelled even if another load begins on the same editor.
- [x] 6.4 Add `MAX_DRAFT_PRELOAD_BYTES = 64 * 1024 * 1024` and stat draft files before eager preload.
- [x] 6.5 Skip eager loading for drafts that would exceed the preload cap without deleting the draft file or manifest entry.
- [x] 6.6 Keep session tab restoration available when draft body preload was skipped because of size limits.

## 7. Regression tests and properties

- [x] 7.1 Add durable-write unit tests proving temp metadata is applied before final temp sync; use a test seam or injected sync failure to classify metadata/final-sync failures as before-rename.
- [x] 7.2 Add durable-write tests proving a `0600` destination never produces a wider-permission temp file containing new bytes.
- [x] 7.3 Add path coordination tests for same canonical target, symlink target, and readable-but-not-writable destination.
- [x] 7.4 Add editor save tests proving symlink saves update targets and preserve symlinks.
- [x] 7.5 Add durable copy fallback tests for source-mode and supported source-xattr/ACL preservation.
- [x] 7.6 Add Replace All tests for per-file cap skip, total undo cap skip, per-file journal persistence, startup cleanup, cancellation rollback, and undo retry.
- [x] 7.7 Add large-buffer save snapshot tests for untitled and grown-in-memory buffers.
- [x] 7.8 Add stale-load generation tests proving old completions cannot mutate current editor state.
- [x] 7.9 Add draft preload cap tests proving oversized drafts are skipped without deletion and tabs can still restore.
- [x] 7.10 Extend property tests for randomized mode preservation across the corrected final-sync ordering and source-metadata copy fallback.

## 8. Benchmarks, docs, and repo guidance

- [x] 8.1 Add Replace All benchmark cases for one 10MB accepted file, one skipped over-cap file, 1,000+ touched files, and journal-enabled replace/undo.
- [x] 8.2 Add measurement or counters showing journal persistence scales linearly rather than rewriting all prior entries.
- [x] 8.3 Update `docs/durable-writes.md` with the corrected ordering: safe temp creation, metadata application, final temp sync, rename, parent sync.
- [x] 8.4 Update `.agents/rules/rust.md`, `AGENTS.md`, and README durability wording for stable target coordination, symlink save behavior, copy fallback metadata, streaming writes, and Replace All caps.
- [x] 8.5 Update OpenSpec canonical specs before archive so `durable-file-write-contract`, `document-save-safety`, `draft-session-recovery`, and `search-replace-safety` retain these contracts after the change is archived.

## 9. Verification

- [x] 9.1 Run `openspec validate fix-durable-write-review-gaps --strict`.
- [x] 9.2 Run `openspec validate --changes --strict` and `openspec validate --specs --strict`.
- [x] 9.3 Run `make check` clean; fix any uncovered blocker in the same work stream.
- [x] 9.4 Run `make test` clean; fix any uncovered blocker in the same work stream.
- [x] 9.5 Run `make test-prop` clean; fix any uncovered blocker in the same work stream.
- [x] 9.6 Run the targeted Replace All and durable-write benchmark smoke after benchmark additions.
- [x] 9.7 Manually verify save behavior for an executable file, a `0600` file, a symlink-backed file, and a Replace All run that hits the configured caps.
