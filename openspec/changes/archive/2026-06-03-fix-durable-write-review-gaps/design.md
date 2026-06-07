## Context

`harden-durable-file-writes` centralized LushText's temp-file write, metadata preservation, and before/after-rename failure classification. Review of that implementation found that the broad direction is correct, but several edge cases still break the intended contract:

- the temp file is synced before metadata is copied, so chmod/chown/xattr/ACL changes are not part of the temp-file durability proof;
- the temp file starts with process-default permissions, which can briefly expose new bytes for a previously private destination before the mode is tightened;
- the advisory `flock` is acquired on the destination inode, but atomic replace swaps that inode out;
- symlink-backed saves write to the symlink path and can replace the link instead of updating the target;
- `copy_file_durable` behaves like "copy bytes into destination" rather than a true cross-filesystem rename fallback that carries source identity;
- Replace All still has unbounded memory and O(files^2) journal rewrite behavior; and
- adjacent file load/save and draft restore paths have scale/concurrency gaps that can undermine the durability work in real use.

This change is a required closure pass before the durable-write work should be archived. It should be implemented together with, or immediately after, `harden-durable-file-writes` so the canonical contract never lands with these gaps.

## Goals / Non-Goals

**Goals:**

- Make the durable-write helper safe from temp creation through rename: private destinations stay private, metadata mutations are synced before rename, and errors remain classified by write phase.
- Replace inode-scoped locking with stable target coordination that survives atomic replace and covers save, Replace All, and Replace All undo.
- Preserve symlink-backed document semantics by writing the resolved file target rather than replacing the symlink path.
- Make cross-filesystem durable copy fallback preserve source metadata, matching user expectations for a failed `rename()` fallback.
- Bound Replace All memory, journal growth, and file-size exposure with concrete limits and user-visible skipped-path reporting.
- Add streaming durable writes for JSON-like data so large or many-entry state writes do not require a prebuilt `Vec<u8>`.
- Fix adjacent load/save scale hazards: live-buffer save snapshot selection, stale async load apply, and draft preload caps.
- Add unit, property, integration, widget, and benchmark coverage for every reviewed failure mode.

**Non-Goals:**

- No `~` backup files beside user documents.
- No cross-platform metadata parity beyond the existing Unix/Linux target; non-Unix fallbacks must keep compiling.
- No broad rewrite of content search or editor architecture beyond the files needed to close these findings.
- No new persistent user-history feature for Replace All; its undo journal remains temporary safety state.

## Decisions

### Create temp files with safe metadata, then sync after all metadata changes

Atomic overwrite will split destination metadata preparation from temp creation. Before creating the temp file, the helper probes the destination metadata if it exists. On Unix, if the destination exists, the temp file is created with the destination's standard permission bits as early as possible, using `OpenOptionsExt::mode()` so a `0600` destination never briefly gets a `0666 & umask` temp sibling containing new bytes. If the destination is absent, the temp file keeps default create permissions.

After writing and flushing content, the helper reapplies metadata on the open temp fd: hard-fail for standard mode bits, best-effort ownership and xattrs/ACLs, then a final `sync_all()` on the temp file. The final temp sync must run after all metadata mutations and before `rename()`. Failures before rename stay `DurableWriteError::BeforeRename`; a parent-dir sync failure after rename stays `AfterRename`.

*Alternative considered:* keep the existing ordering and add only a second sync after metadata copy. Rejected as incomplete because default temp permissions can expose bytes for a private destination before the chmod step.

### Use a stable target lock keyed by canonical path, not the replaceable inode

Replace `FileWriteLock`'s destination-inode `flock` with a stable target lock. The lock key is the resolved write target: canonical path for existing files and symlink targets, or canonical parent directory plus file name for new paths. The lock implementation should use process-local coordination for same-process save/Replace All/undo races and may back it with a stable lock file under the app data directory for cross-process LushText instances. It must not require opening the destination read-write, because readable files in writable directories can still be valid atomic-replace targets.

Callers acquire this lock before reading the original bytes for Replace All, before editor saves write their snapshot, and before undo restores. Tests must prove that two operations addressing the same canonical target through different path spellings or symlinks serialize.

*Alternative considered:* keep `flock` on the destination fd but open read-only. Rejected because it still locks the old inode, not the path that survives atomic replace.

### Save symlink-backed documents through the resolved target

`EditorPage` already records `canonical_file_path` after load. Save target selection should use the canonical target when present, while preserving the user-visible path for display/open bookkeeping as needed. Save As must also detect an existing symlink destination: write the resolved target, adopt a coherent display path/canonical path pair, and avoid replacing the symlink itself. If resolving the symlink target fails, the save must fail before rename and keep the editor modified.

Tests should cover opening a symlink, saving, and confirming the symlink still exists while the target bytes changed. Duplicate detection and path locks should compare canonical targets so a symlink and its target do not create two independently writable tabs.

*Alternative considered:* reject symlink-backed saves. Rejected because updating the target is the least surprising editor behavior and matches normal file-open semantics.

### Add source-metadata durable copy for cross-filesystem fallback

`copy_file_durable(from, to, tmp_tag)` is a fallback for `rename_durable()`. It must therefore behave like a successful rename as closely as possible: destination bytes and destination identity come from the source. Add an internal variant of the atomic write helper that accepts a metadata source path. For normal overwrites, the source is the existing destination. For cross-filesystem copy fallback, the source is `from`. The same temp creation, metadata application, final temp sync, rename, and parent-dir sync ordering applies.

Tests should cover copying a `0644` source over an existing `0600` destination and a source with a `user.*` xattr or POSIX ACL when supported.

### Add streaming durable writes for serialized state

Keep `atomic_write_bytes` for small byte slices, but add a streaming helper that creates the temp file, applies the same metadata/locking/final-sync contract, and lets callers write through a `Write` closure. `json_store::save` should use `serde_json::to_writer_pretty` through this helper. Replace All's journal should also use streaming per-entry writes. This removes the need to materialize large JSON documents solely to call the durability primitive.

### Replace All gets concrete memory and journal limits

Replace All must skip files that exceed a configured per-file cap and stop accepting more journal payload once a total undo-byte cap is reached. Use explicit constants in the service layer, documented and tested:

- `MAX_REPLACE_FILE_BYTES = 10 * 1024 * 1024`
- `MAX_REPLACE_UNDO_BYTES = 64 * 1024 * 1024`

The workflow reports skipped paths instead of trying to process them. It should avoid `Vec<String>` line splitting for the entire file; build the replacement output in one `String`/`Vec<u8>` pass from the original text and recorded ranges. UTF-8 validation should follow the established `simdutf8` pattern already used by editor file loading.

The persistent undo journal moves from "rewrite the whole backup JSON after each file" to per-file durable entries under a temporary replace-journal directory. Each file's entry is written and synced before that file is mutated. Cleanup on undo, panel close, and startup removes the directory. This keeps persistence O(files) and avoids repeated serialization of already-durable entries.

### Guard load/save adapters and draft preload

Large-save snapshot selection must be based on the live buffer size, not only the last on-disk file size. Unknown or currently-large buffers use chunked snapshotting. Async load callbacks must be generation-guarded so an older load result cannot apply after a newer load starts.

Draft restore preloading must stat draft files before reading bodies and enforce a total preload byte cap. Use `MAX_DRAFT_PRELOAD_BYTES = 64 * 1024 * 1024`. Drafts over the per-file or total cap are not loaded eagerly; they are reported/skipped in restore state without deleting recovery files. The user should not lose draft recovery data merely because eager preload was bounded.

## Risks / Trade-offs

- [Risk] More filesystem syscalls per save can slightly increase save latency. -> Mitigation: all blocking work stays behind `spawn_blocking_then`, and tests/benchmarks cover representative save and Replace All paths.
- [Risk] Path-lock files can become stale if the process crashes. -> Mitigation: use advisory `flock` on a stable sidecar file; stale files are harmless because the lock is released by the OS.
- [Risk] Symlink canonicalization can interact with unavailable portal or sandbox paths. -> Mitigation: fail before rename when the target cannot be resolved, keep the editor modified, and retain existing portal-path identity work as a separate concern.
- [Risk] Replace All caps can skip files users expected to modify. -> Mitigation: skipped paths are explicit in the result and can be revisited with narrower search or manual file edits.
- [Risk] Per-file journal directories are more complex than one JSON file. -> Mitigation: isolate the format in a service helper with tests for write, recovery, cleanup, cancellation, and undo.

## Migration Plan

No user data migration is required. Existing state files remain readable. The temporary Replace All backup format may change; startup already clears stale replace backup state, and this change should extend cleanup to both old `replace-backup.json` and the new journal directory. Rollback is reverting the code and docs; no persistent user-facing schema is introduced.

Before archive, sync the final delta specs into canonical specs so `durable-file-write-contract` becomes a living canonical contract and the existing document-save, draft-session, and search-replace specs include the strengthened requirements.

## Open Questions

- None blocking. The implementation should use no new dependency unless a small synchronization crate proves strictly cleaner than a local stable lock-file helper; if added, it must follow workspace dependency, cargo-hakari, and Flatpak cargo-sources rules.
