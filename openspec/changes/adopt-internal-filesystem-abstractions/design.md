## Context

LushText performs filesystem work in many places: editor load/save, durable atomic writes, JSON state stores, drafts, sessions, sidecars, local history, search/replace backup journals, file-tree scanning, command-palette indexing, file peek, workspace mutation actions, watcher setup, property/fuzz/widget tests, and benchmarks. Today those surfaces mix readable `std::fs` calls with domain-specific helpers such as `durable_write`, plus a few Unix extension needs around permissions and symlinks.

The prior rustix analysis points to a strong direction: do not sprinkle `rustix` throughout the app. `rustix` is a good fit as the private Unix/filesystem backend because it exposes descriptor-relative operations, owned file descriptors, metadata/stat calls, rename/unlink/mkdir/openat primitives, and explicit sync/error surfaces. It is not more readable than `std::fs` at ordinary call sites. The redesign therefore makes `rustix` an implementation detail under LushText-named operations.

## Goals / Non-Goals

**Goals:**

- Replace all direct filesystem access in application code with a coherent internal boundary.
- Make common call sites as readable or more readable than the current `std::fs` calls.
- Preserve existing crash-durable write behavior and move its public surface into the filesystem boundary.
- Use descriptor-oriented operations where they improve correctness: directory traversal, symlink-aware identity, metadata preservation, atomic replacement, parent directory sync, and race-resistant workspace operations.
- Provide test fixture helpers so tests also express intent without direct raw filesystem calls.
- Encode the rule in repository docs, rules, and skills, then prove no leftovers with source audits.

**Non-Goals:**

- This does not make LushText cross-platform beyond its current Linux/GNOME target.
- This does not replace Gio file monitors or portals where GTK/Gio integration is the actual boundary.
- This does not introduce a broad trait-based virtual filesystem unless implementation discovers a real need; concrete internal APIs are preferred.
- This does not change user-visible save, draft, session, search, local-history, or sidebar behavior except where existing behavior depends on inconsistent low-level filesystem handling.

## Decisions

### Decision: Create `services::filesystem` as the only production filesystem boundary

Add a `crates/lushtext-core/src/services/filesystem/` module with small operation families:

- `read`: byte/text reads, bounded snapshots, metadata-coupled reads, canonical path resolution.
- `metadata`: file kind, size, mtime, canonical identity, permissions, symlink status, health-oriented facts.
- `write`: durable atomic replacement, streaming durable writes, create-new file/folder, parent-directory sync.
- `tree`: descriptor-relative directory listing, bounded scans, empty-folder lookahead, ignored/hidden filtering inputs.
- `mutate`: rename, remove file, remove directory tree, symlink-aware path operations.
- `sidecar`: sidecar directory creation, sidecar listing/filtering, sidecar move/remove helpers.
- `fixture`: test-only helpers for fixture setup, sparse files, permission changes, symlinks, and disk assertions.
- `sys`: private backend that owns `rustix`, any unavoidable `std::fs` interop, Unix extension imports, and conversion to app-facing errors.

Rationale: callers get vocabulary that matches LushText workflows instead of syscall vocabulary. This keeps UI and service modules readable while giving the backend one place to apply descriptor-oriented safety and durability rules.

Alternatives considered:

- Keep `std::fs` and document discipline. Rejected because direct calls are already widespread and review cannot reliably distinguish safe ordinary use from durability-sensitive use.
- Replace every call with direct `rustix`. Rejected because raw `openat`, `fstatat`, flags, and descriptor lifetimes make most call sites less readable.
- Introduce a global filesystem trait immediately. Rejected for now because LushText does not need multiple production implementations; test fixtures can use concrete helpers without making every service generic.

### Decision: Keep `rustix` private and policy-oriented

Only `services::filesystem::sys` and tightly adjacent backend files may import `rustix`. Public filesystem APIs return LushText types such as `FileSnapshot`, `FileFacts`, `DirectoryEntryInfo`, `WorkspaceScan`, `DurableWriteOutcome`, and existing domain errors where appropriate.

Example call-site shape:

```rust
let snapshot = filesystem::read::text_snapshot(path, ReadPolicy::EditorLoad)?;
let facts = filesystem::metadata::file_facts(path)?;
filesystem::write::atomic_replace(path, bytes, WriteLabel::Save)?;
let entries = filesystem::tree::scan_workspace_root(root, policy, cancel)?;
filesystem::mutate::rename_path(old_path, new_path)?;
```

Rationale: rustix gives better control under the hood; LushText names keep the code reviewable and discoverable.

### Decision: Fold `durable_write` into the filesystem boundary without weakening its contract

The existing durable write behavior remains the normative contract. Its caller-facing functions move under `services::filesystem::write`, or `durable_write` becomes a private implementation module re-exported only through filesystem APIs. Editor saves, JSON stores, drafts, style-scheme writes, Replace All, local history, sidecars, and workspace actions must all use the same write coordination and durability classification.

Rationale: durable writes are already the strongest filesystem abstraction in the repo; the redesign should promote that contract instead of bypassing or duplicating it.

### Decision: Make tests use fixture helpers, not direct raw filesystem calls

Tests and benches should read as clearly as they do today:

```rust
fixture.write_text("src/main.rs", "fn main() {}\n");
fixture.write_bytes("image.png", bytes);
fixture.create_dir("nested");
fixture.symlink("target.txt", "link.txt");
fixture.assert_text("saved.txt", "expected\n");
```

The fixture layer may call the production filesystem boundary, and it may contain limited backend-only raw operations for fixture setup that production code should never see. This keeps the no-leftovers rule honest without making tests noisy.

### Decision: Enforce by audit, docs, and skills

The implementation must add a no-leftovers audit to the validation workflow. The audit should fail on these patterns outside the approved filesystem implementation and test fixture boundary:

- `std::fs::`
- `use std::fs`
- `std::os::unix::fs`
- `std::os::unix::io`
- direct filesystem `libc::`
- direct `rustix::`
- `Path::canonicalize` and related direct path filesystem probes where a filesystem helper exists

Rules and skills must teach future agents to use the boundary. This includes root/nested `AGENTS.md` guidance as needed, `.agents/rules/rust.md`, `.agents/rules/build.md` or documentation rules if validation changes, and filesystem-sensitive skills: `data-safety`, `gtk-perf-review`, `gtk-perf-scale`, `gtk-responsiveness`, `gtk-perf-rust-optimize`, `rust-hex-arch`, and `rust-comments`.

## Risks / Trade-offs

- Large migration can hide behavior regressions -> migrate by operation family with focused tests and keep the final audit strict.
- Fixture helpers can become a second production API -> keep them behind test-only modules and name them around setup/assertion, not application behavior.
- `rustix` APIs are lower-level than `std::fs` -> keep them private and add comments only around backend invariants, descriptor lifetimes, and durability ordering.
- Some Gio/portal paths are legitimate non-filesystem boundaries -> document allowed exceptions explicitly so the audit does not turn into a ritual of ignored false positives.
- Descriptor-relative traversal can be unfamiliar -> expose readable `tree` functions and keep raw descriptors out of sidebar, palette, and search code.
- Dependency churn affects Flatpak and hakari metadata -> refresh dependency artifacts in the same implementation change.

## Migration Plan

1. Add `rustix` to workspace dependencies and refresh lock/hakari/Flatpak cargo-source metadata.
2. Introduce `services::filesystem` with private backend modules and public operation families.
3. Move durable write public entry points behind `filesystem::write` while preserving all existing durable-write tests.
4. Migrate production services by domain: editor I/O, JSON stores, drafts/session, sidecars/local history/bookmarks, content search and Replace All, palette/file-tree scanning, file peek, workspace manager/watch inputs, and UI workspace actions.
5. Migrate tests and benches to fixture helpers.
6. Remove or privatize obsolete helpers and direct imports.
7. Update rules, skills, and agent guidance.
8. Run the full validation stack and final no-leftovers audits.

Rollback strategy is normal git rollback before merge. After merge, rollback should revert the whole change rather than mix old direct filesystem calls with the new boundary.

## Open Questions

None blocking. The recommended path is to adopt the abstraction and use `rustix` privately, not to replace `std::fs` call sites with raw `rustix` call sites.
