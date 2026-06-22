# Durable file writes

LushText persists every file it owns — your documents, drafts, session state,
workspace metadata, and Replace All rewrites — through one shared helper,
`crates/lushtext-core/src/services/durable_write.rs`. This page documents the
guarantees that helper makes so callers do not re-derive the filesystem contract
by hand.

## The atomic write contract

Replacing an existing path runs this sequence, in order:

1. Probe the metadata source before temp creation. Overwrites use the existing
   destination; cross-filesystem copy fallback uses the source file.
2. Create a uniquely named hidden temp file **in the destination's own
   directory** with permissions no wider than the metadata source's standard
   mode bits. Brand-new files keep platform default permissions.
3. Stream or write the new bytes into that temp file, then flush the writer.
4. Apply required metadata to the temp file (standard mode bits are mandatory;
   ownership, ACLs, and xattrs are best-effort).
5. `fsync` (`sync_all`) the temp file **after** all content and metadata
   mutations — the data and required metadata are now durable.
6. `rename` the temp file over the destination — an atomic swap; readers see
   either the complete old file or the complete new file, never a torn write.
7. `fsync` the destination's **parent directory** — the rename (a directory-entry
   change) is now durable too.

Steps 5 and 7 are both required on ext4, XFS, and Btrfs: syncing only the file
contents leaves the name→inode link able to vanish across power loss.

## Identity-metadata preservation

Because the rename installs a brand-new inode, a naive temp-file-then-rename
would reset the file's permissions to the process default and drop everything
else. To match GNOME's `g_file_replace`, the helper copies metadata onto the
temp file before the final temp sync and before the swap:

| Metadata | Guarantee |
| --- | --- |
| Standard permission bits (`0o777`) | Preserved exactly |
| Ownership (uid/gid) | Best-effort (`fchown`); unprivileged saves are usually a no-op and never fail the save |
| POSIX ACLs (`system.posix_acl_access`) | Best-effort, via the xattr copy |
| Extended attributes (`user.*`, `security.selinux`, …) | Best-effort |
| setuid / setgid / sticky | Best-effort only — the kernel **intentionally clears setuid/setgid** when an unprivileged process rewrites file contents, which is the secure behavior |

Practical effects: a `chmod +x` script stays executable after you edit it, and a
`0600` private file is not silently widened to be world-readable. Brand-new files
(no existing destination) keep the platform default permissions and inherit
nothing.

Best-effort metadata is exactly that: unsupported filesystems (`ENOTSUP`) and
permission/policy denials (`EPERM`) are skipped, never fatal. Reapplying the
standard mode bits is the one hard requirement, so a save can never quietly
relax a file's permissions.

## Stable write coordination

Editor saves, Save As, Replace All, and Replace All undo all acquire the same
process-local write guard for the resolved target path. Existing files and
symlinks use the canonical target; missing files use the canonical parent
directory plus the requested file name. This avoids the old inode-lock trap:
atomic rename replaces the inode, so locking the previous destination file is
not a stable coordination point.

The guard never opens the destination read-write. A readable file in a writable
directory can still be a valid atomic-replace target, and coordination should
not fail solely because the existing file mode is read-only.

## Symlinks and copy fallback

Saving through a symlink writes the resolved target and leaves the symlink in
place. Broken symlink targets fail before rename, keeping the editor modified so
the user can retry or choose another destination.

`copy_file_durable()` is a fallback for a rename that must cross filesystems. It
therefore preserves the source file's bytes and identity metadata on the
destination, then removes the source only after the destination write and parent
directory sync succeed.

## Streaming writes

`atomic_write_stream_classified()` lets callers serialize directly into the temp
file while inheriting the same metadata, final-sync, rename, and parent-sync
contract as byte-slice writes. `json_store::save` uses this for pretty JSON, and
Replace All uses per-file durable journal entries instead of repeatedly
rewriting one growing backup.

## Failure classification

`atomic_write_bytes_classified` returns a `DurableWriteError` that tells callers
which side of the rename failed:

- **`BeforeRename`** — the temp write, flush, metadata copy, final temp sync, or the rename
  itself failed. The destination still holds its previous bytes; nothing was
  committed. The editor maps this to `EditorSaveError::WriteTemp`, reports the
  save as failed, and keeps the document modified so the user still has an
  unsaved-work signal.
- **`AfterRename`** — the rename landed (the new bytes *are* the destination) but
  the parent-directory `fsync` failed. The change is visible yet not proven
  crash-durable. The editor maps this to
  `EditorSaveError::DurabilityUnconfirmed`, surfaces a distinct durability
  **warning** (not a generic "save failed"), and keeps the document modified so
  re-saving can re-attempt the directory flush.

This distinction is why a transient directory-sync hiccup is never reported to
the user as lost data.

## No swallowed sync failures (fsyncgate)

Every `fsync`/`sync_all` — on the temp file after metadata, the destination
directory, and each newly created directory in a tree — is propagated. A failed
sync always surfaces as a `DurableWriteError` or `io::Error`; it is never turned
into a silent success. Because the final temp-file sync happens *before* the
rename, a failure there leaves the previous destination bytes intact, so the
safe outcome is always "report failure, previous content preserved."

## Using it

Prefer the shared helper over hand-rolled temp+rename:

```rust
use crate::services::durable_write;

// Don't care about the failure phase:
durable_write::atomic_write_bytes(path, "save", bytes)?;

// Need to distinguish lost-write from durability-unconfirmed:
durable_write::atomic_write_bytes_classified(path, "save", bytes)
    .map_err(|e| match e {
        durable_write::DurableWriteError::BeforeRename(io) => /* report failure */,
        durable_write::DurableWriteError::AfterRename(io)  => /* durability warning */,
    })?;

durable_write::atomic_write_stream_classified(path, "json", |writer| {
    serde_json::to_writer_pretty(writer, value).map_err(std::io::Error::other)
})?;
```

All of editor save, `json_store`, draft persistence, the custom style-scheme
writer, and Replace All route through these functions, so they all inherit the
same metadata preservation, write coordination, and failure classification.
