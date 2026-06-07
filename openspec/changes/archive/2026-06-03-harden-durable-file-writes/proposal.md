## Why

LushText's atomic temp-file-then-rename writes already implement the full Linux
durability contract (write → `fsync` temp → `rename` → `fsync` parent dir), but
because the replacement is always a brand-new inode it silently drops the
destination's permission bits, ownership, POSIX ACLs, extended attributes, and
SELinux label. Editing a `chmod +x` script makes it non-executable; saving a
`0600` secret recreates it world-readable. Separately, the editor save path
collapses two very different failures — "your bytes never reached disk" and "your
bytes are on disk but the directory entry is not yet flushed" — into one generic
`Finalize` error, so a benign directory-sync hiccup is reported to the user as a
lost save. This change closes the one substantive gap between our write path and
GNOME's `g_file_replace`, and makes failure signalling honest.

## What Changes

- Teach the shared `durable_write` helper to copy the destination's identity
  metadata (mode bits, ownership best-effort, POSIX ACLs, user/SELinux extended
  attributes) onto the temp file **before** the rename, so an atomic overwrite
  keeps the file's prior on-disk identity. New-file creation keeps default
  permissions.
- Split the post-rename durability-sync failure from the pre-rename
  write/rename failure across every atomic write path (editor save,
  `json_store`, drafts, style-scheme write, `atomic_write_bytes`), mirroring the
  classification the Replace All path already uses. A post-rename sync failure is
  surfaced as a distinct durability warning, never as a silent success and never
  as a generic "save failed."
- Surface the editor save's post-rename durability failure to the user as its
  own warning while keeping the document marked modified so a retry can re-attempt
  the directory flush, instead of the current message that implies the bytes were
  lost.
- Document and lock down the `fsync`-failure ("fsyncgate") semantics: a failed
  durability sync is always propagated, never swallowed.
- Add regression coverage: permission/ownership/ACL/xattr preservation across an
  overwrite, new-file default permissions, and the post-rename vs pre-rename error
  classification.
- Update `AGENTS.md`, `.agents/rules/rust.md` (Background I/O durability note),
  and `README.md`/durability docs to record the metadata-preservation contract.

## Capabilities

### New Capabilities
- `durable-file-write-contract`: The shared filesystem durability primitive used
  by every persistence caller — the atomic temp-file-then-rename ordering, parent
  and directory-tree `fsync` rules, destination identity-metadata preservation on
  overwrite, and the before-rename vs after-rename failure classification that
  never silently discards a durability error.

### Modified Capabilities
- `document-save-safety`: File-backed saves preserve the destination's identity
  metadata, and a post-rename durability-sync failure is reported as a distinct
  durability warning instead of an indistinguishable lost-save error.
- `search-replace-safety`: Replace All file rewrites preserve each replaced
  file's permissions and identity metadata through the shared atomic write path.

## Impact

- Code: `crates/lushtext-core/src/services/durable_write.rs` (metadata copy +
  error classification), `services/editor_io.rs` (save error split + UI signal),
  `services/json_store.rs`, `services/draft_service.rs`,
  `services/content_search/replace.rs`, `ui/editor_page/imp.rs` (style-scheme
  write), and the editor save-result handling in `ui/editor_page/load_save.rs`.
- Platform: Linux-only metadata syscalls (`fchmod`, `fchown`, `*xattr`) live
  behind `#[cfg(unix)]`; the existing non-Unix no-op paths stay compiling.
- Dependencies: may add a small extended-attribute crate (or thin `libc`
  bindings) to the workspace; requires `cargo hakari generate` and
  `make cargo-sources` if a dependency is added.
- Tests: new unit/property coverage in `durable_write` and `editor_io`; no GTK or
  display-server dependency for the metadata behavior.
