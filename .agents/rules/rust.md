---
description: Rust coding conventions for lushtext
globs: "**/*.rs"
---

# Rust Conventions

## Crate Structure

- All dependencies declared in the repository-root `[workspace.dependencies]`, consumed with `{ workspace = true }`.
- Every crate depends on `workspace-hack` for cargo-hakari.
- License header `// SPDX-License-Identifier: GPL-3.0-or-later` on LushText
  application `.rs` files. GTK Lush family crates under `crates/gtk-lush/`
  are dual-licensed with `// SPDX-License-Identifier: MIT OR Apache-2.0`.
- LushText may consume in-tree GTK Lush internal-platform crates
  (`gtk-lush-signals`, `gtk-lush-settle`, `gtk-lush-tasks`,
  `gtk-lush-viewport`, `gtk-lush-widgets`, `gtk-lush-proof-harness`, and
  `gtk-lush-proof-spine`) through workspace path dependencies. GTK Lush
  family crates must remain leaf crates and must not depend on LushText or on
  each other at runtime.

## GTK/GLib Imports

- Use `libadwaita` (not `adw`) in all imports — there is no crate alias.
- For `imp()` method: import `glib::subclass::prelude::ObjectSubclassIsExt`.
- For GtkSourceView extension methods on Buffer, View, annotation providers, annotations, and other source-view objects: import `sourceview5::prelude::*`.
- Prefer `gtk4::prelude::*` for general GTK widget methods.
- In `imp.rs` files: use `libadwaita::subclass::prelude::*` (re-exports the full chain).

## Version Alignment

All gtk-rs crates must be from the same release series. Current versions:
- `gtk4 = 0.11`, `gdk4 = 0.11`, `sourceview5 = 0.11`
- `libadwaita = 0.9`
- `glib = 0.22`, `gio = 0.22`, `pango = 0.22`
- `glib-build-tools = 0.22`
- Platform feature floor: GNOME 50 (`gtk4` `gnome_50` / GTK 4.22, Libadwaita 1.9, GLib/GIO 2.88, GtkSourceView feature `v5_18`).

Mixing versions (e.g. glib 0.21 with gtk4 0.11) causes "multiple versions of crate glib" errors.

## GObject Subclassing

Every custom widget: `mod.rs` (public wrapper + `glib::wrapper!`) + `imp.rs` (private struct + trait impls).

- Register child widget types with `ensure_type()` in `class_init()`.
- Use `connect_notify_local()` (not `connect_notify()`) for signal closures capturing GTK widgets — GTK types are not `Send`.
- GTK initialization order: CSS/Display access requires GTK to be initialized. Load CSS in `startup()`, not before `app.run()`.

## Dark Mode

GtkSourceView has its own theming separate from GTK CSS. Always:
1. Query `libadwaita::StyleManager::is_dark()` to pick `"Adwaita"` vs `"Adwaita-dark"`.
2. Connect to `connect_dark_notify()` for runtime changes.
3. Store long-lived settings/style-manager handler registrations in
   `gtk_lush_signals::SignalBag` so editor tabs disconnect them on teardown.

## Background I/O

Use `gtk_lush_tasks::spawn_blocking_then(state, work, then)` for any I/O that may block:
- `state`: non-Send GTK object (auto-wrapped in `ThreadGuard`)
- `work`: runs on background thread, must be `Send`
- `then`: runs on main thread with result, does NOT need to be `Send`

Never pass GTK objects directly across threads — they are not `Send`/`Sync`.
Let `gtk-lush-tasks` carry GTK-thread state through `ThreadGuard`, and keep
workflow freshness checks explicit at the call site.

## GTK Main-Loop Timing

Use `gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer}` for GTK
main-loop timers where latest-generation semantics are the whole contract.
Keep domain generations and worker freshness tokens explicit when they protect
durable writes, undo journals, file loads, or cross-thread result ownership.

## GTK Main-Thread Snapshot Boundaries

GTK text buffers must only be read on the GTK main thread, but whole-buffer
copies can still freeze the UI when they happen in one callback. Reuse
`ui::buffer_snapshot` for editor text snapshots that feed saves, draft
autosave, encoding analysis, preview flows, or optional marker scans.

- Small, already-admitted buffers may use `snapshot_buffer_text_direct()` and
  immediately move the result into the workflow's guarded worker handoff.
- Unknown, grown-in-memory, or large buffers must use
  `snapshot_buffer_text_async()` or an explicit paused/limited state. Chunked
  capture owns independently allocated UTF-8 chunks; any whole-body coalescing,
  transformation, and final destruction must happen on a worker under the same
  admission guard rather than returning one large `String` to GTK.
- A typed payload permit must span capture, worker handoff, transformation,
  persistence, terminal freshness, and rejected/stale disposal. Compact latest
  intent may wait for admission, but document-sized text may not.
- Worker results that mutate UI state must carry a generation counter and a
  weak editor/window identity check. Reject results when the editor was closed,
  switched paths, edited again, or superseded by a newer request.
- Optional UI hints such as Markdown preview rendering and minimap long-line
  markers may skip or pause for large buffers; do not trade editor
  responsiveness for secondary decoration.
- Save As canonical bookkeeping must not call canonicalization on the GTK
  thread after the chooser returns. Use the background save result immediately
  for UI identity and schedule any follow-up canonical refresh through
  `spawn_blocking_then`, applying it only while the editor is still mounted and
  still owns the same path.

## Filesystem Boundary

Production code must use `services::filesystem` for file reads, metadata,
canonical identity, traversal, mutation, sidecar helpers, and durable writes.
Call sites should read in LushText terms, such as
`filesystem::read::text`, `filesystem::metadata::exists`,
`filesystem::metadata::path_status`, `filesystem::metadata::file_facts`,
`filesystem::tree::scan_directory`, `filesystem::mutate::remove_file_if_exists`,
and `filesystem::write::atomic_replace`. Use `exists` or `path_status` for
presence/kind checks so callers do not pay for canonicalization, file length, or
mtime conversion they do not use. Reserve `file_facts` for workflows that
actually need canonical identity, byte size, or modification time.

Approved raw filesystem exceptions are limited to:

- `services::filesystem::sys` for the private descriptor and platform backend.
- `services::filesystem::fixture` for test and benchmark setup/assertions.
- Documented, read-only engine adapters with an audit allowlist. Today this is
  limited to the content-search query path, whose walker/searcher stack may own
  traversal, ignore/glob filtering, binary detection, and streaming reads. Its
  command side must still route Replace All writes, undo journals, cleanup, and
  persistence through `services::filesystem`.

Do not import the private durable implementation from callers. The public
durability surface is `services::filesystem::write`, including
`atomic_replace`, `atomic_replace_stream`, `rename_durable`,
`copy_file_durable`, `create_dir_durable`, `sync_parent_dir`, and
`TargetWriteGuard`.

Tests and benches should use `services::filesystem::fixture` helpers such as
`write_text`, `write_bytes`, `create_dir_all`, `create_sparse_file`,
`symlink`, `set_mode`, and `assert_text`. This keeps examples readable while
preserving the boundary.

Run `./scripts/check-filesystem-boundary.sh` after filesystem-sensitive changes
or before completing work that touches file I/O, persistence, tests, benches,
rules, or skills. A clean run means no disallowed raw filesystem examples remain
outside the approved backend and fixture modules.

Durable atomic writes on Linux require the full ordered filesystem contract:
probe metadata, create the temp file with safe permissions, write and flush
content, apply required metadata, call `sync_all()` on the temp file after those
metadata mutations, `rename()`, then sync the parent directory so ext4, XFS, and
Btrfs cannot lose the renamed directory entry across power loss. Use
`services::filesystem::write::create_dir_durable()` for single-directory
creation that must be durable, and `sync_parent_dir()` only when an existing
durable workflow explicitly needs to seal a namespace mutation.

Prefer the shared `services::filesystem::write::atomic_replace` (or
`atomic_replace_stream` when you need streaming serialization) over hand-rolling
temp-file-then-rename. The shared boundary guarantees these things every
persistence caller must inherit and must not silently drop:

- **Identity-metadata preservation.** Because the rename installs a brand-new
  inode, an overwrite would otherwise reset the destination's permissions,
  ownership, ACLs, and xattrs. The helper copies that metadata onto the temp file
  before the final temp sync and rename (standard `0o777` bits are preserved
  exactly; ownership, ACLs/xattrs, and the setuid/setgid/sticky bits are
  best-effort — the kernel intentionally clears setuid/setgid on a content
  rewrite). New files keep default permissions. Copy fallback uses source
  metadata, not destination metadata. Do not reintroduce a raw `File::create` +
  `rename` that loses this.
- **Stable target coordination.** Editor save, Save As, Replace All, and undo
  must acquire the resolved target guard before reading or writing file bytes.
  Do not coordinate on the destination inode; atomic rename replaces that inode.
  Symlink paths and canonical target paths must share one guard, and acquiring
  the guard must not require opening the destination read-write.
- **Honest failure classification.** `DurableWriteError::BeforeRename` means the
  previous bytes are intact (report as an unwritten/failed save and keep the
  document modified); `DurableWriteError::AfterRename` means the new bytes are on
  disk but the directory `fsync` did not complete (surface a distinct
  "durability unconfirmed" warning, never a generic lost-save). The editor maps
  these to `EditorSaveError::WriteTemp` and
  `EditorSaveError::DurabilityUnconfirmed`. Never swallow an `fsync` error into
  a silent success.

Draft orphan-body cleanup has an additional cross-operation identity contract.
Inspection records the candidate inode; execution reloads the latest trusted
manifest, acquires the same stable `TargetWriteGuard` used by atomic replacement,
then rechecks inode before deletion. Manifest serialization alone is insufficient:
an autosave may finish replacing the body before it acquires the manifest lock.
Never delete a planned orphan body using only its path or draft ID.

Never set `autoexpand = true` on `GtkTreeListModel`.

When save-time policy rewrites buffer text (for example EditorConfig
`trim_trailing_whitespace` or `insert_final_newline`), the saved bytes and live
buffer must agree before the buffer is marked clean. Either mirror the saved
text back into the buffer after a successful write or keep the buffer modified;
do not show a clean tab whose visible text differs from disk.

## Error and Literal Ownership

Cross-boundary error types must name the workflow or domain they report for:
prefer `EditorLoadError`, `EditorSaveError`, `WorkspaceWatchError`,
`DraftReadError`, or `ProofValidationError` over bare names such as
`Error`, `LoadError`, `SaveError`, `ValidationError`, or mechanism-only names
when the type is public, `pub(crate)`, re-exported, service-facing, UI-facing,
or shared across crates. Private helper errors can stay short only when the
owning module and function already make the failing workflow obvious.

Numeric literals that define user-visible behavior, persistence limits,
file-size thresholds, retry budgets, debounce/timeout windows, UI geometry,
schema or protocol limits, or resource caps belong in named typed constants or
small policy values near the workflow, service, model, UI module, or tool that
owns the decision. Do not create a generic constants dump just because two
numbers match; share a constant only when the same policy is intentionally
shared. Inline literals remain fine for `0`/`1`, indexes, simple arithmetic
identities, obvious counters, narrow fixture data, and tests that do not mirror
production policy.

## Mutable State on GObject Structs

- Use `Cell<T>` for `Copy` types (e.g., `Cell<Option<u64>>`, `Cell<u32>`). No borrow overhead, no panic risk from overlapping borrows.
- Use `RefCell<T>` for non-`Copy` types (e.g., `RefCell<Option<PathBuf>>`, `RefCell<Option<String>>`).
- Both default correctly via `#[derive(Default)]` on the imp struct (`Cell<Option<T>>` defaults to `Cell::new(None)`).

## Lint Suppression

- Prefer `#[expect(lint, reason = "...")]` over `#[allow(lint)]` when suppressing a lint for a known reason (e.g., using a deprecated API that has no replacement yet). `#[expect]` is self-policing: it causes a compile error if the lint no longer fires, so stale suppressions are caught automatically. The reason must name the local GTK, generated-code, test, benchmark, or ownership invariant.
- Reserve `#[allow(lint)]` only for cases where the lint may or may not fire depending on configuration or feature flags.
- The workspace Clippy table is curated lint-by-lint after cleanup. Broad groups such as `clippy::restriction`, `clippy::pedantic`, `clippy::nursery`, and `clippy::cargo` are advisory discovery inputs only; do not enable them wholesale as blocking policy.
- Rust 1.96 Clippy lints `manual_option_zip`, `manual_pop_if`, `manual_noop_waker`, `manual_midpoint`, `unchecked_time_subtraction`, `decimal_literal_representation`, `case_sensitive_file_extension_comparisons`, `significant_drop_tightening`, `needless_collect`, `redundant_clone`, `derive_partial_eq_without_eq`, `wildcard_imports`, and `debug_assert_with_mut_call` are denied in the workspace lint table. Prefer the standard helpers those lints point to instead of hand-rolled equivalents. In particular, execute mutations before `debug_assert!` and assert only the captured result so release builds cannot elide required state changes.
- `make lint-advisory` runs broad Clippy, selected design-smell Clippy, selected numeric Clippy, and selected rustc probes. Every current category is classified in `scripts/lint-advisory-policy.toml` as `blocking_candidate`, `must_stay_zero`, `accepted_advisory`, `generated_code_noise`, or `resolved_policy_exception`; refresh that policy only after fixing, promoting, or intentionally classifying new output.
- No `clippy.toml` is currently checked in because this review found no globally safe disallowed method/type ban that applies across backend, fixture, generated, test, and build-support paths without broad suppressions. Add `clippy.toml` only when a future globally safe ban can include reason and replacement metadata. Path-sensitive rules such as filesystem-boundary ownership stay in `scripts/check-filesystem-boundary.sh`, where backend, fixture, build-support, and approved engine-adapter exceptions can be expressed by path.

## Modern Rust Idioms

Use `if let` chains with `&&` / `&& let` (Edition 2024) as the preferred pattern for multi-condition guards that combine option unwrapping with boolean checks:

```rust
// Preferred: if-let chain
if let Some(obj) = obj_weak.upgrade()
    && let Some(ref cb) = *obj.imp().callback.borrow()
{
    cb();
}

// Avoid: nested if-let
if let Some(obj) = obj_weak.upgrade() {
    if let Some(ref cb) = *obj.imp().callback.borrow() {
        cb();
    }
}
```

With Rust 1.96.0, also use `if let` match guards when a match arm needs both the matched value and a short fallible guard:

```rust
match encoding {
    DocumentEncoding::Utf8 if let Ok(text) = simdutf8::basic::from_utf8(bytes) => {
        text.to_string()
    }
    _ => fallback_decode(bytes),
}
```

Prefer the new standard-library helpers when they make intent clearer:

- Use `std::assert_matches` or `std::debug_assert_matches` in tests when a pattern assertion benefits from clearer failure output. Import the macro explicitly in each module; it is not in the prelude.
- Use `slice.array_windows::<N>()` over `slice.windows(N)` when the window width is fixed and every element is indexed.
- Use `Atomic*::try_update` / `Atomic*::update` instead of hand-written compare-exchange loops when the closure expresses the complete state transition clearly.
- Use `Peekable::next_if_eq`, `next_if`, `next_if_map`, or `next_if_map_mut` instead of manual `peek()` + `next()` pairs.
- Use `Vec::push_mut` / `insert_mut` only when the caller immediately needs a mutable reference to the inserted value; plain `push` remains clearer otherwise.
- Use `cfg_select!` for expression-level or tightly grouped cfg choices. Keep item-level `#[cfg]` when separate Unix/non-Unix implementations are already clearer.

Use the Rust 1.96 `core::range` value types only when they reduce real range-moving friction in named byte-span values. Range syntax still produces the legacy `std::ops` range types today, so do not mechanically rewrite every `std::ops::Range<usize>` import. Keep legacy ranges for proptest strategies, third-party APIs, and APIs meant to accept ordinary range syntax; prefer `impl RangeBounds<usize>` for new caller-facing range inputs.

## Error Handling

- Services return `anyhow::Result`.
- For file I/O: try the operation and handle errors; avoid preflight existence checks that race with the operation.
- In GTK signal handlers: log errors with `tracing::error!`, don't panic.

## File Size Limit

**Target limit: keep production `.rs` files under roughly 1000 lines when practical.** `#[cfg(test)]` modules are excluded from this count — co-located tests are encouraged and should not trigger file splits.

Existing over-limit files are accepted refactor debt. When you touch one substantially, prefer extracting helpers or sibling modules that reduce responsibility count and local complexity instead of letting the file grow unchecked.

When production code approaches 1000 lines:
1. **Split by responsibility.** Extract cohesive groups of functions into new modules. For UI widgets, the `mod.rs` / `imp.rs` split already helps — if `mod.rs` grows, extract helpers (e.g., `actions.rs`, `dialogs.rs`). For services, split by sub-domain.
2. **Never split mid-impl block.** Keep all trait impls for a type in one file. Split by extracting private helper functions into sibling modules, then calling them from the main impl.
3. **Prefer vertical, not horizontal splitting.** A 900-line file with one clear responsibility is better than 3 files that constantly cross-reference each other.
4. **Split GTK adapters by workflow before inventing new abstraction layers.** If a widget starts mixing unrelated flows (actions, notifications, persistence, search runtime, focus recovery), prefer sibling modules under the widget folder over new traits or faux-manager types.
5. **Promote repeated field bundles into named value objects or state groupings.** If multiple call sites rebuild the same shape (for example query text + toggle state), move it into `model/`. If an `imp` struct accumulates unrelated timers/counters/maps, group them into small helper structs with clear workflow ownership.

## Testing

- Unit tests: `#[cfg(test)]` inside service modules, no GTK dependency.
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split pattern.
- Widget tests: `crates/lushtext/tests/widget.rs` with `#[path]` split pattern; require display server.
- Use `TestContext` for filesystem isolation (tempdir + simulated XDG dirs).
- Use `services::filesystem::fixture` helpers for file setup and assertions in tests and benches.
- Run `cargo hakari generate` after adding/removing dependencies.
