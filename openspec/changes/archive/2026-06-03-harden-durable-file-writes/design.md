## Context

Every persistence caller in LushText funnels through `services/durable_write.rs`,
which already implements the four-step Linux durability contract correctly
(write temp → `fsync` temp → `rename` → `fsync` parent dir) and even adds a
write-ahead undo journal for Replace All and advisory `flock` coordination. Two
gaps remain versus GNOME's `g_file_replace`:

1. **No identity-metadata preservation.** Temp-file-then-rename replaces the
   destination inode with a fresh one created by `File::create`, which gets
   default permissions (`0666 & ~umask`) and the saving user's ownership/context.
   Mode bits, ownership, POSIX ACLs, user xattrs, and the SELinux label are all
   dropped. `crates/lushtext-core/src/services/editor_io.rs:753`
   (`write_bytes_to_path`), `json_store.rs:50`, `draft_service.rs:255`,
   `content_search/replace.rs:451` (`atomic_write`), and
   `durable_write::atomic_write_bytes` all create the temp file without copying
   the destination's metadata.

2. **Conflated save failure classes.** `editor_io::write_bytes_to_path` maps both
   a failed `rename` and a failed parent-dir `fsync` to `SaveError::Finalize`
   (`editor_io.rs:782-794`), so a post-rename directory-sync hiccup is reported
   identically to a genuine lost write — even though the bytes are already on
   disk. The Replace All path already models this correctly via
   `AtomicWriteError::{BeforeRename, AfterRename}` (`replace.rs:432-438`); the
   editor save path should adopt the same distinction.

Constraints: shipped target is Linux/GTK; metadata syscalls must sit behind
`#[cfg(unix)]` with the existing non-Unix no-op fallbacks. The metadata behavior
must stay GTK-free so it remains unit/property testable without a display server,
consistent with `.agents/rules/build.md` mutation/property scoping.

## Goals / Non-Goals

**Goals:**

- A single metadata-preservation routine in `durable_write` that all atomic
  overwrite paths inherit, so editor saves, JSON state, drafts, style-scheme
  writes, and Replace All rewrites all keep the destination's identity.
- Preserve mode bits unconditionally; preserve ownership, POSIX ACLs, and
  user/security xattrs best-effort, never failing the write on `EPERM`/`ENOTSUP`.
- A uniform before-rename vs after-rename failure classification exposed by the
  shared helper and consumed by the editor save path, so the user-facing signal
  is honest.
- `fsync`-failure ("fsyncgate") semantics documented and locked by tests: a
  failed durability sync is always surfaced, never swallowed.
- Regression coverage for every guarantee, plus rules/README/AGENTS updates.

**Non-Goals:**

- **`~` backup files on save.** `g_file_replace` can optionally keep a backup of
  the previous version; this change deliberately does **not** add one. GNOME Text
  Editor does not keep `~` backups in the saved location, and doing so would
  clutter user workspaces and surprise version-control workflows. This is a
  decided exclusion, not deferred work; if backup-on-save is ever wanted it is a
  separate product feature with its own UX, not a durability fix.
- Changing the existing fsync ordering, the flock coordination, or the Replace
  All write-ahead journal — those are already correct and stay as-is.
- Cross-platform metadata parity for non-Unix targets, which keep the existing
  no-op behavior.

## Decisions

### Copy metadata onto the temp fd before rename, inside `durable_write`

A new `#[cfg(unix)]` helper — `preserve_destination_metadata(dest, &temp_file)` —
runs after the temp bytes are written and synced but **before** the rename. It:

1. `stat`s the destination; if it does not exist, returns early (new-file path
   keeps default perms).
2. `fchmod`s the temp file's fd to the destination's mode bits (unconditional;
   failure is a real error).
3. `fchown`s the temp fd to the destination uid/gid best-effort, ignoring
   `EPERM` (the common unprivileged case where the file is already owned by the
   user, so the copy is a no-op anyway).
4. Copies extended attributes from destination to temp best-effort, which carries
   `system.posix_acl_access` (POSIX ACLs) and `security.selinux` (SELinux label)
   along with `user.*` xattrs, ignoring `ENOTSUP`/`EPERM`/`EACCES`.

Operating on the open temp **fd** (`fchmod`/`fchown`) rather than the temp path
avoids a TOCTOU window on the security-sensitive permission copy.

*Alternative considered:* reimplement each call site. Rejected — five call sites
would drift; centralizing in the one audited helper is the whole point of
`durable_write`.

### xattr access: thin `libc` bindings over a new crate, unless a vetted crate is cleaner

ACLs and the SELinux label come "for free" by copying xattrs, so we need
`listxattr`/`getxattr`/`fsetxattr`. Prefer thin `libc` wrappers (already a
dependency) to avoid expanding the dependency/`cargo-hakari`/`cargo-sources`
surface. If a small, well-maintained `xattr` crate proves materially simpler and
passes `cargo deny`, adopt it and run `cargo hakari generate` + `make
cargo-sources`. Either way the xattr copy is best-effort and isolated to one
function.

*Alternative considered:* only copy mode + ownership, skip xattrs/ACLs/SELinux.
Rejected — the proposal commits to "everything, including the minor nuances," and
ACL/SELinux drop is a real (if rarer) correctness/security regression.

### Promote `AtomicWriteError`-style classification into the shared helper

Generalize the before/after-rename distinction (currently private to
`replace.rs`) into `durable_write` so `atomic_write_bytes` and the editor save
path return a typed result distinguishing the two. `editor_io` gains a
`SaveError::DurabilityUnconfirmed` (after-rename) variant distinct from the
existing pre-rename `WriteTemp`/`Finalize` cases, and `load_save.rs` surfaces it
as a durability warning while keeping the document modified (matching the repo's
defensive data-safety posture and the existing remain-modified-on-failure rule).
`replace.rs` is refactored to use the shared classification instead of its
private copy.

*Alternative considered:* mark the doc clean on after-rename failure (bytes are
visible). Rejected — until the directory entry is flushed the change can vanish
on power loss, so keeping it modified for a retry is the safer, repo-consistent
choice.

### Test strategy

- Unit tests in `durable_write`: overwrite preserves a `0600` mode and an
  executable bit; new-file path uses default perms; xattr round-trip on a tmpfs
  that supports `user.*` (gated/skipped when the temp filesystem returns
  `ENOTSUP`); a simulated parent-dir-sync failure yields an after-rename error.
- `editor_io` tests: save over a `0600`/executable file preserves mode; the two
  failure classes map to distinct `SaveError` variants.
- A property test (existing `property-tests` lane) over random mode bits asserting
  preservation across an overwrite round trip.

## Risks / Trade-offs

- **xattr support varies by filesystem** (tmpfs may reject `user.*`, some
  mounts disable ACLs) → all xattr/ACL/ownership copies are best-effort and never
  fail the save; tests detect `ENOTSUP` and skip rather than assert.
- **`fchown` almost always fails for unprivileged users** → treated as a no-op
  success; ownership preservation only matters under elevated/`sudo` editing and
  must never block an ordinary save.
- **Copying `security.selinux` could, under strict policy, be denied or
  mislabel** → best-effort copy ignores the failure, leaving the kernel's default
  transition label, which is the same outcome as today (no regression).
- **Behavior change in failure messaging** → the new after-rename warning text
  and the `SaveError` variant are covered by tests so widget/error-path
  assertions stay deterministic.

## Migration Plan

No data migration. The change is backward compatible: files written before the
change simply gain metadata preservation on their next save. Rollback is reverting
the commit; no persisted format or schema changes. Per
`.agents/rules/preexisting-blockers.md`, any pre-existing failing check uncovered
during implementation is fixed in this same change set rather than deferred.

## Open Questions

- None blocking. The single implementation choice left open — thin `libc` xattr
  bindings vs. a vetted `xattr` crate — is decided at coding time by whichever
  passes `cargo deny` with the smaller surface; both satisfy the same spec
  scenarios.
