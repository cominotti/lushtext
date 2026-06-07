## Context

The completed rustix filesystem work left LushText with a strong default boundary: production code reaches local filesystem reads, metadata, traversal, mutation, durable writes, sidecar mechanics, fixtures, and tests through `services::filesystem`, while low-level `rustix` and the few remaining `std::fs` conveniences stay private to `services::filesystem::sys`.

The remaining architectural risk is no longer "raw calls everywhere." It is boundary drift at the places where a simple wrapper is not enough:

- Content search intentionally uses the ripgrep/ignore engine stack (`ignore::WalkBuilder` plus `grep_searcher::search_path`) for high-throughput walking and file reads. That is not a direct `std::fs` call, but it is filesystem I/O outside the LushText filesystem operation families.
- Several callers use `file_facts(path).is_ok()` or local `path_exists()` wrappers when they only need a cheap existence or kind query. `file_facts()` also tries canonicalization, file length, and mtime, so it is heavier than the intent at those call sites.
- The private Unix backend already opens directories with rustix, but child metadata during directory visits is still resolved from a joined child path. Current rustix filesystem APIs include descriptor-relative operations such as `openat`, directory iteration, `fstatat`, `renameat`, `unlinkat`, `mkdirat`, `chmodat`, `fsync`, and path `Arg` support, so the backend can keep tightening descriptor ownership without changing callers.
- The boundary audit catches direct raw imports and `.exists()`, but it does not yet express the positive model for approved engine adapters or local helper drift.

## Goals / Non-Goals

**Goals:**

- Keep `services::filesystem` as the default filesystem adapter and make every exception explicit, narrow, and auditable.
- Bless specialized engine adapters only when they provide behavior the boundary should not reimplement, starting with the content-search ripgrep/ignore stack.
- Add reusable, intention-revealing metadata status helpers for existence and file-kind checks.
- Replace local `path_exists()` wrappers and `file_facts(...).is_ok()` status probes with the new helper surface.
- Harden directory traversal internals to prefer rustix descriptor-relative metadata where it improves correctness and keeps backend semantics localized.
- Extend the no-leftovers audit and guidance so future agents see both the rule and the exception process.

**Non-Goals:**

- No user-facing search, sidebar, preview, save, draft, local-history, note, bookmark, or workspace behavior changes.
- No new filesystem trait, virtual filesystem abstraction, or second production backend.
- No attempt to force the ripgrep/ignore content-search engine through `filesystem::tree` or `filesystem::read`; that would risk performance and duplicate mature engine behavior.
- No attempt to hide GTK/GIO file chooser, resource search path, style-scheme search path, or file-monitor APIs behind `services::filesystem` when they are toolkit integration points.
- No dependency changes expected.

## Decisions

### Decision: Keep `services::filesystem` as the default port and introduce named engine-adapter exceptions

The boundary should remain the normal route for local filesystem work. A small, documented exception class should cover specialized libraries that own their own traversal or read model for good reasons. Content search is the first approved exception because the ripgrep ecosystem provides parallel walking, gitignore/glob semantics, binary detection, regex matching, streaming line sinks, cancellation checks, and search-progress behavior as one cohesive engine.

The exception should be explicit in code comments, guidance, and the boundary audit. The audit does not need to parse every third-party API call, but it should detect newly introduced engine adapter imports or search-path file engines that are not on the allowlist.

Alternative considered: require content search to use `filesystem::tree` plus `filesystem::read`. Rejected because it would reimplement a poorer ripgrep engine, weaken search performance, and likely make cancellation/backpressure behavior less coherent.

### Decision: Add metadata status helpers rather than reusing `file_facts()` for existence checks

Callers that only need "is this path present?" or "is this path a directory?" should not call `file_facts()`. The boundary should expose a cheap query family, for example:

- `metadata::path_status(path) -> std::io::Result<PathStatus>`
- `metadata::exists(path) -> bool`
- `metadata::kind(path) -> std::io::Result<FileKind>`
- `metadata::is_directory(path) -> std::io::Result<bool>`

The exact names can settle during implementation, but the public surface should distinguish cheap status from richer facts. `file_facts()` remains the editor/load-oriented query that gathers canonical identity, size, and mtime.

Alternative considered: keep local helpers because they are small. Rejected because each local helper teaches a slightly different pattern and hides whether the caller needs cheap metadata or a richer snapshot.

### Decision: Continue rustix-first backend hardening without exposing rustix outside `sys`

The private backend should keep using rustix for descriptor-owned Unix operations where it gives better ownership, race resistance, or durability precision. The next step is directory child metadata: when walking a directory already opened by rustix, child metadata should be collected through descriptor-relative rustix APIs where supported, while still returning the same LushText `RawDirectoryEntry` and public `DirectoryEntryInfo` shapes.

This remains an implementation detail. Public callers should not see descriptors, `rustix::fs::AtFlags`, path `Arg` constraints, or backend errno values. Backend errors continue to convert to `std::io::Error` or existing service-level errors.

Alternative considered: replace every remaining private `std::fs` use with rustix. Rejected because the design principle is rustix where it improves correctness or precision, not rustix as ceremony for ordinary byte reads.

### Decision: Strengthen audits around intent, not just raw imports

The current audit should remain the hard gate for direct raw filesystem calls. This change should add narrower checks for:

- local `fn path_exists` helpers outside approved modules,
- `file_facts(...).is_ok()` and similar full-facts status probes in production code where the new status helper should be used,
- unapproved direct imports of filesystem-walking or file-reading engine crates outside documented adapter modules,
- approved engine adapters missing a short rationale comment or guidance entry,
- stale helper surfaces created by this change.

The audit should avoid broad false positives against domain methods such as `FileTreeItem::is_dir()` or toolkit "search path" APIs. The goal is a deterministic reminder, not a brittle style police script.

Alternative considered: rely on review discipline for these subtler cases. Rejected because the prior filesystem work proved that deterministic checks are the best way to prevent small leftovers from becoming permanent architecture.

## Risks / Trade-offs

- [Risk] The audit could become noisy by matching domain `is_dir()` methods or GTK/GIO search-path APIs. -> Mitigation: keep checks narrow to known filesystem-boundary patterns and maintain explicit allowlists with comments.
- [Risk] Descriptor-relative traversal could subtly change symlink or missing-child behavior. -> Mitigation: preserve existing public scan semantics and add focused tests for hidden files, missing children, symlinks, permission errors, and cancellation/truncation.
- [Risk] Blessing engine adapters could become a loophole for bypassing the boundary. -> Mitigation: require each exception to name the owning module, the engine behavior it depends on, the operations it may perform, and the validation proving no ordinary raw filesystem calls leaked.
- [Risk] New metadata helpers could duplicate existing `file_facts()` semantics. -> Mitigation: document the split: status helpers are cheap existence/kind queries; `file_facts()` is the richer facts bundle for editor and metadata workflows.
- [Risk] Replacing local status probes may touch many files without changing behavior. -> Mitigation: migrate by workflow and keep the final diff focused on callers that currently perform existence/kind probes.

## Migration Plan

1. Inventory current status-probe patterns, local `path_exists()` helpers, content-search engine filesystem operations, and private backend directory traversal behavior.
2. Add the metadata status helper surface and tests that prove it avoids unnecessary canonicalization while preserving existing missing-path and kind behavior.
3. Migrate production callers from local wrappers and `file_facts(...).is_ok()` to the new helper names.
4. Document content search as an approved specialized engine adapter and add tests or audit evidence that ordinary content-search writes still use the filesystem write boundary.
5. Harden `filesystem::sys::visit_directory_entries` toward descriptor-relative child metadata on Unix while preserving public scan ordering and error-tolerance behavior.
6. Extend `scripts/check-filesystem-boundary.sh`, root/nested guidance, and filesystem-sensitive rules/skills for the new status helper and engine-adapter exception model.
7. Run the filesystem-boundary audit, targeted filesystem/content-search tests, full relevant Rust tests, and strict OpenSpec validation.

Rollback is a normal revert before release because this is an internal boundary hardening change. It should not require persisted data migration.

## Open Questions

- Should the public status type be a small enum such as `PathStatus::{Missing, File, Directory, Other}` or a `Result<Option<FileKind>>` style API? The implementation should choose whichever reads best at the caller sites.
- Should approved engine adapters be listed inside the audit script, a small documentation file, or both? The recommendation is both: script for enforcement, guidance for human intent.
