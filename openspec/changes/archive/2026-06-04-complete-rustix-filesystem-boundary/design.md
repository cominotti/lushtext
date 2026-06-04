## Context

LushText already routes ordinary production callers through `services::filesystem`, and the filesystem-boundary audit currently passes. The remaining architecture debt is inside and around the boundary: `services::filesystem::sys` owns some `rustix` descriptor primitives, while `services::durable_write` still owns a separate raw `std::fs`/Unix-extension/`libc` implementation island. The public API shape is also not fully finished: `filesystem::sidecar` and `FilesystemError` exist but are not consistently adopted, and rename/durable-rename semantics are split across overlapping operation families.

The desired end state is not "replace every safe standard-library call with rustix." The desired end state is one coherent filesystem boundary where `rustix` is the private Unix backend for descriptor-sensitive operations, unavoidable platform gaps are isolated, and callers use LushText vocabulary such as snapshots, directory scans, durable replacement, sidecar storage, target identity, and fixture setup.

## Goals / Non-Goals

**Goals:**

- Make `services::filesystem` the single public filesystem adapter for production code, tests, benches, and guidance.
- Consolidate raw filesystem implementation details into a single private backend area owned by the filesystem boundary.
- Prefer `rustix` for Unix operations where it provides safe, descriptor-owned primitives: open/stat/read-directory, fsync/fdatasync, rename, unlink, mkdir, chmod/chown, file length, and other supported syscalls.
- Preserve the durable-write contract exactly: temp-file-then-rename ordering, metadata preservation, final temp sync after metadata, parent-directory sync, before/after-rename classification, stable target coordination, streaming writes, and copy fallback safety.
- Resolve unused or half-adopted abstractions by either adopting them everywhere they fit or removing them from the public surface.
- Strengthen the audit so "complete" means no direct raw filesystem leftovers, no stale helper surfaces, and no duplicate public operation families.

**Non-Goals:**

- No user-facing feature changes.
- No new storage backend, plugin filesystem abstraction, or trait-based virtual filesystem.
- No weakening of save, Replace All, draft, local-history, note/bookmark, session, workspace, test, or benchmark behavior.
- No attempt to hide GTK/GIO file chooser or monitor APIs behind this boundary when they are toolkit integration points rather than raw filesystem access.

## Decisions

### Decision: Keep free-function operation families, not filesystem traits

`services::filesystem::{read, metadata, tree, mutate, write, fixture}` remains the caller-facing port. A trait would add indirection without a second production backend or a clear mocking need. Tests already use temp directories and fixture helpers, which exercise the real filesystem contract that matters for this app.

Alternative considered: introduce a `Filesystem` trait or port object. Rejected because this desktop app has one local filesystem backend, and the repo's architecture guidance favors free-function ports until there are multiple implementations or a real mock seam.

### Decision: Create one private platform backend for raw operations

Durable writes should keep the durability state machine, but raw operations used by that state machine should move behind the filesystem boundary's private platform/backend layer. This includes temp-file creation with mode, rename/unlink cleanup, metadata probing, chmod/chown, directory sync, read/copy fallback helpers, canonical identity helpers, and any Linux xattr handling.

Alternative considered: leave `durable_write.rs` as an approved raw exception. Rejected because it preserves two places where raw platform behavior can drift, and it makes future audits treat a large implementation file as a permanent exception.

### Decision: Rustix covers the current Unix backend contract

Use `rustix` for the filesystem syscalls LushText needs when it provides safe I/O ownership and good path support. The pinned `rustix` 1.1.x API includes the xattr helpers needed for Linux ACL and user-xattr preservation (`listxattr`, `getxattr`, `setxattr`, and `fsetxattr`), so this change has no remaining direct-`libc` filesystem gap. Ordinary callers must never see raw descriptors, syscall flags, C strings, or backend errno values.

Alternative considered: keep a documented direct-`libc` fallback for future metadata gaps. Rejected because implementation proved rustix covers the required xattr and ACL-preservation operations, and a speculative fallback would weaken the no-leftovers audit.

### Decision: Public mutation semantics need one owner

Namespace mutation APIs should be named by policy:

- Non-durable fixture or simple mutation helpers stay in `filesystem::mutate` only when callers do not need a crash-durable namespace guarantee.
- Durable namespace helpers, target identity, write guards, atomic replacement, durable copy fallback, and parent-directory sync stay in `filesystem::write` or a clearly named durability-focused family.
- Any overlapping `rename_path`/`rename_durable` behavior must be resolved so production callers do not guess which helper syncs parent directories.

Alternative considered: keep both helpers and rely on comments. Rejected because this is exactly the kind of subtle filesystem policy that should live in the API shape.

### Decision: Sidecar helpers must be adopted or removed

`filesystem::sidecar` currently names a useful workflow but is not used. Implementation must either route bookmark, document-note, workspace-note, and local-history sidecar listing/move/remove/ensure operations through that helper surface, or remove it and place the reusable behavior in the already-used note/local-history storage helpers. A public helper with zero call sites is a leftover.

Alternative considered: keep it for future use. Rejected because "future use" helpers invite drift and weaken the no-leftovers goal.

### Decision: Filesystem errors must have one intentional public contract

The filesystem boundary can either adopt `FilesystemError` for operation/path context where it improves caller diagnostics, or remove it and standardize on `std::io::Error` plus `anyhow::Context` at service boundaries. The implementation should not leave an exported error type that no caller uses.

Alternative considered: leave `FilesystemError` exported but unused. Rejected as a stale abstraction.

### Decision: The audit becomes part of the contract

The existing filesystem-boundary audit should keep catching raw filesystem imports and direct path probes outside approved locations. It should also fail on stale leftovers introduced by this migration: public direct imports of `durable_write`, unused exported filesystem helper surfaces, duplicate public mutation helpers with the same safety contract, and raw backend imports outside the narrowed allowlist.

Alternative considered: rely on code review for these subtler cases. Rejected because the user explicitly wants no leftovers, and deterministic checks are the best way to make that true.

## Risks / Trade-offs

- [Risk] Moving durable write internals can accidentally weaken crash-durability ordering. -> Mitigation: preserve existing durable-write tests first, add focused backend parity tests, and run the data-safety validation path before marking tasks complete.
- [Risk] A future metadata operation may need a syscall that the pinned rustix version does not expose. -> Mitigation: treat that as new design work rather than preserving a speculative direct-`libc` escape hatch in this completed change.
- [Risk] API consolidation can become a broad mechanical churn. -> Mitigation: migrate callers by workflow, keep public names where they are already correct, and add compatibility aliases only inside the change if needed for staged implementation; final state must remove aliases that exist only for transition.
- [Risk] Sidecar helper adoption could blur domain-specific note/bookmark identity logic. -> Mitigation: keep identity rebasing and domain decisions in note/bookmark/local-history services; only centralize filesystem mechanics such as ensure/list/move/remove.
- [Risk] Audit checks for unused helpers can be brittle. -> Mitigation: keep checks narrow to known boundary artifacts and pair them with code-search evidence in tasks.

## Migration Plan

1. Inventory every raw filesystem/backend occurrence and every public filesystem helper call site.
2. Create or reshape private filesystem backend modules so durable writes can use shared platform primitives.
3. Move durable-write raw operations behind those primitives while preserving existing durable-write public behavior through `filesystem::write`.
4. Consolidate public mutation/write helper naming so each safety policy has one obvious entry point.
5. Adopt or remove `filesystem::sidecar` and `FilesystemError`.
6. Strengthen the audit script and guidance allowlists to match the final backend shape.
7. Run the filesystem-boundary audit, strict OpenSpec validation, Rust tests covering durable writes and sidecar workflows, and formatting/lint checks.

Rollback is straightforward before release because this is an internal refactor: revert the change artifacts and implementation commit. No persisted user data migration is expected.

## Open Questions

- Answered during implementation: `rustix` covers the Linux xattr operations needed by the durable-write metadata-preservation contract, so there is no remaining direct-`libc` filesystem gap.
- Answered during implementation: durable namespace helpers remain under `filesystem::write` because existing save, Replace All, fixture, and persistence call sites already read clearly through the durability-focused operation family.
