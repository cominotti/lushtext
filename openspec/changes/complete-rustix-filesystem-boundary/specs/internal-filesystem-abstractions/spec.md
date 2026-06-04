## ADDED Requirements

### Requirement: Raw filesystem implementation has one private backend owner
The filesystem boundary SHALL keep direct `std::fs`, Unix filesystem extension, direct filesystem `libc`, direct `rustix`, raw file descriptor, C string path conversion, and backend-specific errno handling inside approved private backend modules owned by `services::filesystem`. Durable-write implementation code MUST use those private backend operations instead of maintaining a separate raw filesystem implementation island.

#### Scenario: Durable writes do not own a second raw backend
- **WHEN** the filesystem-boundary migration is complete
- **THEN** durable write implementation code no longer contains direct raw filesystem operations that duplicate private filesystem backend operations
- **AND** any remaining raw backend usage is located inside the approved filesystem backend allowlist

#### Scenario: Public callers cannot reach backend details
- **WHEN** a production service saves, renames, scans, removes, syncs, or inspects a path
- **THEN** it uses a public `services::filesystem` operation family
- **AND** it does not import private backend modules, raw descriptors, syscall flags, or direct backend error types

### Requirement: Rustix-first backend adoption is complete
The private filesystem backend SHALL prefer `rustix` for Unix filesystem operations that require descriptor ownership, descriptor-relative behavior, precise metadata, namespace mutation, directory traversal, file or directory syncing, permission preservation, or ownership preservation when the current rustix version supports the operation safely. Direct `libc` usage MUST remain only for documented Linux filesystem operations not covered by rustix and MUST be isolated behind a narrow backend helper.

#### Scenario: Supported Unix filesystem operations use rustix
- **WHEN** the backend implements open, stat, directory iteration, rename, unlink, mkdir, chmod, chown, file sync, or directory sync behavior that rustix supports
- **THEN** the implementation uses rustix rather than direct `std::os::unix` extension traits or direct `libc`
- **AND** callers receive app-facing types and `std::io::Error` or app-facing error wrappers

#### Scenario: Unsupported metadata gaps stay isolated
- **WHEN** preserving required identity metadata needs a Linux filesystem syscall that rustix does not expose in the pinned version
- **THEN** the implementation isolates the direct `libc` call in the private backend
- **AND** the audit allowlist and code comments name the exact operation and why it remains outside rustix

### Requirement: Public filesystem operation families have no duplicate safety contracts
The filesystem boundary SHALL expose one clear public entry point for each safety policy. Callers MUST NOT have to choose between duplicate helpers that appear to provide the same rename, durable rename, parent sync, directory creation, removal, write coordination, target identity, or sidecar filesystem semantics.

#### Scenario: Rename helpers communicate durability policy
- **WHEN** production code needs to rename a file or directory
- **THEN** the available public helper name and module make the parent-directory sync policy explicit
- **AND** there is no second public helper with the same apparent contract but different durability behavior

#### Scenario: Directory creation helpers communicate durability policy
- **WHEN** production code creates a directory or directory tree
- **THEN** it uses the helper whose name and module communicate whether crash-durable parent sync is part of the operation
- **AND** tests and fixtures use fixture helpers rather than production-only durability helpers unless the test is specifically validating durability behavior

### Requirement: Sidecar filesystem helpers are either adopted or removed
The implementation SHALL not leave an exported sidecar filesystem helper surface unused. Bookmark, document-note, workspace-note, and local-history workflows MUST either reuse a common sidecar filesystem helper for shared ensure/list/move/remove mechanics or the unused helper surface MUST be removed in favor of the already-used workflow-specific storage helpers.

#### Scenario: Shared sidecar filesystem mechanics are reused
- **WHEN** multiple sidecar services need to ensure a storage directory, list visible sidecar files, move sidecar paths, or remove stale sidecar files
- **THEN** those services use the same shared filesystem helper surface for the filesystem mechanics
- **AND** document identity rebasing and domain-specific filtering remain in the owning service or domain helper

#### Scenario: Unused sidecar helper surface is removed
- **WHEN** implementation determines that workflow-specific helpers are the clearer abstraction
- **THEN** any exported `filesystem::sidecar` helper with no callers is removed
- **AND** the no-leftovers audit confirms there is no unused public sidecar filesystem surface

### Requirement: Filesystem error abstraction is intentional and used
The filesystem boundary SHALL have one intentional app-facing error contract. If an operation/path contextual filesystem error wrapper remains exported, production code or tests MUST use it where that context is valuable. If callers intentionally rely on `std::io::Error` plus service-level context, the unused wrapper MUST be removed.

#### Scenario: Exported filesystem error wrapper has callers
- **WHEN** `FilesystemError` or another app-facing filesystem error wrapper remains exported
- **THEN** at least one boundary operation uses it to carry operation and path context
- **AND** callers do not need to inspect backend-specific rustix or libc error values

#### Scenario: Unused filesystem error wrapper is removed
- **WHEN** the final design keeps `std::io::Error` and `anyhow::Context` as the filesystem error contract
- **THEN** the unused filesystem error wrapper is removed from the public boundary
- **AND** the no-leftovers audit confirms it is not exported without callers

### Requirement: No-leftovers audit covers stale abstractions and backend leaks
The no-leftovers audit SHALL fail when direct raw filesystem access appears outside the approved backend and fixture allowlist, when production code imports durable-write implementation helpers directly, when exported filesystem helper surfaces have no callers after this migration, or when duplicate public operation families retain overlapping safety contracts.

#### Scenario: Audit fails on backend imports outside the allowlist
- **WHEN** source code outside the approved filesystem backend or fixture modules imports direct `rustix`, direct filesystem `libc`, direct `std::fs`, Unix filesystem extension traits, or raw path filesystem probes
- **THEN** the audit reports the file and line
- **AND** implementation cannot be marked complete

#### Scenario: Audit fails on stale helper leftovers
- **WHEN** an exported filesystem helper introduced or evaluated by this change has no production, test, or benchmark callers
- **THEN** the audit or completion checklist reports it as a leftover
- **AND** implementation cannot be marked complete until the helper is adopted or removed

#### Scenario: Audit fails on direct durable implementation use
- **WHEN** production code imports or calls a durable-write implementation helper instead of the filesystem write boundary
- **THEN** the audit reports the direct usage
- **AND** implementation cannot be marked complete
