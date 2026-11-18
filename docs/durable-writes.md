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

The sequence is not spelled out in the helper's code as straight-line calls.
It is an I/O-free state machine, `services/filesystem/write_protocol.rs`:
`WriteProtocol::step(outcome)` takes the outcome of the last operation and
returns the next one (probe, create temp, write content, apply metadata, sync
temp, rename, sync directory, remove temp, finish with a classification).
`MoveProtocol` and `RenameProtocol` do the same for `move_durable` and
`rename_durable`. `durable_write.rs` is only the shell: it executes each action
with exactly one backend call and feeds that call's outcome back unchanged, so
every ordering and classification decision lives in the core. The loop that
does this exists once, `services/protocol_drive.rs`: each core implements its
`Protocol` trait beside its shell, and every shell (the write, move and rename
here, and Replace All's apply, undo and journal-shrink shells) calls
`protocol_drive::drive` with a closure that executes one action. A move's
`Copy` action runs a whole durable write, a `drive` nested inside the move's.
A source check (`no_shell_steps_its_core_outside_protocol_advance`) fails if a
shell steps its core in a loop of its own. Because the core
is finite and I/O-free, Kani (`make kani`) explores every outcome sequence and
every crash point of the real core against a model disk with POSIX crash
semantics, and proves:

- after any crash, the destination holds the complete previous bytes, or the
  complete new bytes with their metadata and the destination's permissions;
- `BeforeRename` means the previous bytes are still the destination,
  `AfterRename` that the new bytes are, and success that they are durably;
- new bytes never sit in a file created wider than the destination's mode,
  because the temp is created only after a successful probe;
- a move removes its source only once the copy is durable, and a completed
  rename synced every directory it mutated.

A `should_panic` harness keeps the canonical counterexample: a shell that
skips the temp sync can expose a torn destination after a crash.

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

`move_durable()` is a fallback for a rename that must cross filesystems. It
therefore preserves the source file's bytes and identity metadata on the
destination, then removes the source only after the destination write and parent
directory sync succeed. `copy_durable()` is the same durable copy without the
removal: it never touches its source. The two are deliberately separate
primitives; the retired `copy_file_durable()` removed its source under a copy's
name, which phase 0 of the formal-verification programme caught as a defect
waiting to happen.

## Streaming writes

`atomic_write_stream_classified()` lets callers serialize directly into the temp
file while inheriting the same metadata, final-sync, rename, and parent-sync
contract as byte-slice writes. `json_store::save` uses this for pretty JSON, and
Replace All uses per-file durable journal entries instead of repeatedly
rewriting one growing backup. Replace All clears the prior run, writes its first
per-file entry, and durably activates an incremental manifest before the first
target rename; each later durable entry is therefore recovery-visible before
its corresponding rename without quadratic manifest rewrites.

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

## Temp names, labels and kill points

A temp file is named `.{file}.{tag}.{pid}.{seq}.tmp` by
`services/filesystem/temp_name.rs`, the one owner of the format: `format`
builds it and `parse` is the crash-leftover sweep's predicate. The tag is a
`WriteLabel`, a closed set of constants (`WriteLabel::KNOWN`), and the label
travels as a `WriteLabel` from the public entry points through the durable
write, copy and move down to `temp_name::format`, which is the only place it
becomes a string. A caller therefore cannot write a temp whose tag the sweep
would not recognise; a name with an unknown tag can only be crafted by hand,
and `parse` refuses it. The names are pinned per label by
`the_builder_emits_the_same_temp_name_for_every_known_label`, because the
sweep reads names that older builds wrote.

The startup sweep (`services/app_data_leftovers.rs`) does not keep its own
list of where temps can land: it visits every location the app-data layout
(`services/app_data_layout.rs`) marks swept, in a fixed order that spends its
shared budget the same way on every launch: the flat directories, each
format-upgrade run with its `items/`, each local-history lineage, then the
legacy folder-note sidecar directory last. A file location is covered by its
parent directory, which a unit test checks is swept, so a new location added
to the layout is covered or fails to compile.

The crash smoke's kill points (`services/kill_point.rs`, feature
`crash-kill-points`) narrow on a key, `LUSHTEXT_KILL_AT=name@key#n`. The key
is the write label's string for the durable-write and Replace All windows
(`durable-renamed-before-dirsync@json`, for example), and the **draft id**
for the two draft windows, which is why `kill_point::reach` keeps taking a
string.

## Using it

Prefer the shared helper over hand-rolled temp+rename. Callers use the public
boundary, `services::filesystem::write`, and name the write with a label:

```rust
use crate::services::filesystem::{WriteLabel, write as fs_write};

// Don't care about the failure phase:
fs_write::atomic_replace(path, WriteLabel::SAVE, bytes)?;

// Stream into the temp file, distinguishing lost-write from
// durability-unconfirmed:
fs_write::atomic_replace_stream(path, WriteLabel::JSON, |writer| {
    serde_json::to_writer_pretty(writer, value).map_err(std::io::Error::other)
})
.map_err(|e| match e {
    fs_write::DurableWriteError::BeforeRename(io) => /* report failure */,
    fs_write::DurableWriteError::AfterRename(io)  => /* durability warning */,
})?;
```

All of editor save, `json_store`, draft persistence, the custom style-scheme
writer, and Replace All route through these functions, so they all inherit the
same metadata preservation, write coordination, and failure classification.
