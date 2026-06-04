# internal-filesystem-abstractions Specification

## Purpose
Define LushText's internal filesystem boundary so production code, tests, guidance, and audit tooling use readable application-level filesystem operations instead of raw platform APIs.

## Requirements
### Requirement: Production filesystem access uses the internal boundary
All production code SHALL route ordinary filesystem reads, metadata probes, path identity checks, directory traversal, file creation, durable writes, renames, removals, sidecar operations, and workspace file operations through the LushText internal filesystem boundary. Direct `std::fs`, Unix filesystem extensions, direct filesystem `libc`, direct `rustix`, and direct path filesystem probes MUST NOT appear outside the approved filesystem implementation modules. Specialized filesystem engines MAY perform their own traversal or reads only when they are documented as approved engine adapters, provide cohesive behavior the boundary should not reimplement, and remain covered by deterministic audit allowlists and guidance.

#### Scenario: Migrated production code has no raw filesystem leftovers
- **WHEN** the implementation migration is complete
- **THEN** a source audit over production Rust code finds no direct `std::fs`, Unix filesystem extension, direct filesystem `libc`, direct `rustix`, or direct path filesystem probe usage outside the approved filesystem implementation modules
- **AND** any specialized filesystem engine usage outside the filesystem boundary is listed as an approved engine adapter with a narrow module owner and rationale

#### Scenario: New production filesystem caller uses the boundary
- **WHEN** a service or UI workflow needs to read, inspect, create, rename, remove, scan, or write a filesystem path
- **THEN** it calls a LushText filesystem operation that names the application intent
- **AND** it does not import raw filesystem APIs directly

#### Scenario: Specialized engine adapter remains narrow
- **WHEN** content search or another approved engine adapter performs filesystem traversal or file reads through a third-party engine
- **THEN** the adapter documents the engine-owned behavior it depends on
- **AND** mutating operations, durable writes, sidecar writes, backup journals, and cleanup still use the LushText filesystem boundary
- **AND** the no-leftovers audit fails if equivalent engine filesystem usage appears outside the approved adapter modules

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

### Requirement: Filesystem operations remain intent-readable
The internal filesystem boundary SHALL expose small, intention-revealing APIs that are at least as readable at call sites as the `std::fs` operations they replace. Public APIs MUST use LushText domain vocabulary and MUST NOT require ordinary callers to manage raw file descriptors, syscall flags, nul-terminated path conversion, Unix mode constants, or backend-specific error types.

#### Scenario: Editor load reads through a document snapshot helper
- **WHEN** editor I/O loads a file for display
- **THEN** the call site requests a text or byte snapshot with editor-oriented read policy
- **AND** metadata, canonical identity, file size, encoding inputs, and mtime facts are returned through app-facing types

#### Scenario: Workspace scanning reads through tree helpers
- **WHEN** sidebar, palette, or search code scans a workspace directory
- **THEN** the call site requests a workspace or bounded directory scan with a readable policy object
- **AND** raw descriptor-relative traversal details stay inside the filesystem backend

### Requirement: Metadata status probes use lightweight boundary helpers
The filesystem boundary SHALL expose lightweight metadata/status helpers for existence and path-kind queries so callers do not need full `file_facts()` snapshots or local `path_exists()` wrappers when they only need cheap status information. These helpers MUST keep raw path probes inside the private backend and MUST return app-facing status or `FileKind` values rather than raw platform metadata.

#### Scenario: Existing path can be checked without full facts
- **WHEN** production code needs to know whether a path exists
- **THEN** it calls a lightweight filesystem metadata/status helper
- **AND** the call does not require canonical path, byte size, or mtime collection when those facts are not needed

#### Scenario: Directory kind can be checked through the boundary
- **WHEN** production code needs to know whether a path is a file, directory, other kind, or missing
- **THEN** it calls a filesystem metadata/status helper that returns app-facing kind/status information
- **AND** it does not use `Path::is_dir`, `Path::is_file`, `.exists()`, or a local helper wrapping a full `file_facts()` probe

#### Scenario: Rich file facts remain available for workflows that need them
- **WHEN** editor load, status presentation, or another workflow needs canonical identity, byte size, modified time, and kind together
- **THEN** it continues to use `file_facts()` or a richer snapshot helper
- **AND** the lightweight status helper does not replace richer fact collection where those facts are part of the workflow contract

### Requirement: Test status probes use lightweight filesystem helpers
Tests, property tests, widget tests, integration tests, and benchmarks SHALL use lightweight filesystem status helpers when they only need to assert existence, absence, or path kind. Rich filesystem fact helpers such as `file_facts()` MUST remain reserved for assertions that inspect canonical identity, byte size, modified time, or multiple facts together.

#### Scenario: Existence assertion avoids rich facts
- **WHEN** a test only needs to assert that a path exists or is absent
- **THEN** it uses `services::filesystem::metadata::exists` or `path_status`
- **AND** it does not call `file_facts()` solely to check whether metadata can be read

#### Scenario: Rich fact assertion remains explicit
- **WHEN** a test needs canonical path, byte size, modified time, or kind facts as part of the assertion
- **THEN** it may call `file_facts()`
- **AND** the returned facts are inspected rather than used only as an existence proxy

### Requirement: Approved engine adapters are documented and audited
Specialized filesystem engine adapters SHALL be documented as explicit exceptions to the default filesystem boundary. Each approved adapter MUST name the owning module, the third-party engine APIs it uses for filesystem traversal or reads, the behavior that justifies the exception, the filesystem operations it is allowed to perform, and the operations that must still route through `services::filesystem`.

#### Scenario: Content search is an approved read-only engine adapter
- **WHEN** workspace content search uses the ripgrep/ignore stack for parallel walking, gitignore/glob filtering, binary detection, regex matching, streaming line sinks, cancellation, and progress reporting
- **THEN** that read-only traversal and file reading are documented as an approved engine-adapter exception
- **AND** Replace All writes, undo backups, backup cleanup, and any sidecar or persistence operations continue to use the LushText filesystem boundary

#### Scenario: New engine adapter cannot appear silently
- **WHEN** a new production module imports a filesystem-walking or file-reading engine that bypasses `services::filesystem`
- **THEN** the no-leftovers audit reports it unless the module is added to the approved engine-adapter allowlist with rationale
- **AND** implementation cannot be marked complete until the exception is documented or the code is routed through the filesystem boundary

### Requirement: Rustix is a private backend detail
The filesystem boundary SHALL use `rustix` internally for Unix descriptor-owned operations where it improves correctness, durability, or precision, including descriptor-relative open/stat/read-directory operations, file and directory syncing, rename/unlink/mkdir primitives, symlink-aware metadata, and permission or ownership preservation. Direct `rustix` imports MUST remain private to approved backend modules and MUST NOT leak into app-facing return types.

#### Scenario: Descriptor-relative traversal is hidden from callers
- **WHEN** a workspace scan opens a directory and enumerates child entries
- **THEN** the backend may use `rustix` descriptor-relative operations
- **AND** callers receive LushText directory entry types rather than raw descriptors or syscall flag combinations

#### Scenario: Backend errors convert to app-facing errors
- **WHEN** a backend operation receives a `rustix` error
- **THEN** the filesystem boundary converts it to the existing app-facing error family or an app-specific filesystem error
- **AND** callers do not pattern match on backend-specific errno values unless the filesystem API deliberately exposes that policy

### Requirement: Private directory traversal prefers descriptor-relative metadata
The private filesystem backend SHALL use descriptor-relative rustix operations for Unix directory traversal metadata when the current rustix version supports them safely and doing so preserves the public scan semantics. Public callers MUST continue receiving LushText entry types rather than descriptors, syscall flags, or backend errno values.

#### Scenario: Directory child metadata stays inside the opened directory context
- **WHEN** the Unix backend scans a directory and inspects each child entry
- **THEN** child metadata is obtained through descriptor-relative backend operations when supported
- **AND** callers receive the same `DirectoryEntryInfo` or service-level entry shapes as before

#### Scenario: Traversal behavior remains compatible
- **WHEN** a scanned child disappears, is unreadable, is hidden, is a symlink, or is an unsupported filesystem kind
- **THEN** the public scan behavior remains compatible with the existing boundary contract
- **AND** backend-specific errors are converted before leaving the filesystem boundary

### Requirement: Rustix-first backend adoption is complete
The private filesystem backend SHALL use `rustix` for Unix filesystem operations that require descriptor ownership, descriptor-relative behavior, precise metadata, namespace mutation, directory traversal, file or directory syncing, permission preservation, ownership preservation, or Linux extended-attribute preservation when the current rustix version supports the operation safely. The completed implementation MUST NOT retain a direct `libc` filesystem fallback or allowlist because the pinned rustix version covers the metadata-preservation operations LushText needs.

#### Scenario: Supported Unix filesystem operations use rustix
- **WHEN** the backend implements open, stat, directory iteration, rename, unlink, mkdir, chmod, chown, file sync, or directory sync behavior that rustix supports
- **THEN** the implementation uses rustix rather than direct `std::os::unix` extension traits or direct `libc`
- **AND** callers receive app-facing types and `std::io::Error` or app-facing error wrappers

#### Scenario: Required metadata preservation has no direct libc fallback
- **WHEN** preserving required identity metadata copies Linux extended attributes or ACL-backed metadata
- **THEN** the implementation uses rustix xattr helpers through the private backend
- **AND** no direct `libc` filesystem dependency, source call, or audit allowlist remains for that metadata path

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

### Requirement: Mutating filesystem operations preserve safety policies
The filesystem boundary SHALL own policies for file and directory creation, durable replacement, parent-directory syncing, symlink-aware save targets, stable write coordination, sidecar moves, Replace All journaling, and destructive removals. Mutating callers MUST use boundary operations that make their safety policy explicit.

#### Scenario: Save and Replace All use the same coordinated write path
- **WHEN** editor save and Replace All write to the same canonical target
- **THEN** both operations acquire the same filesystem-boundary write coordination guard
- **AND** their durable writes cannot interleave as independent raw filesystem calls

#### Scenario: Workspace item creation syncs through the boundary
- **WHEN** the sidebar creates a new file or folder for inline rename
- **THEN** creation and parent-directory sync happen through a filesystem boundary mutation helper
- **AND** cancellation cleanup uses a filesystem boundary helper rather than a raw remove call

#### Scenario: Sidecar migration uses boundary move and remove helpers
- **WHEN** an in-app rename or delete requires document notes, workspace notes, bookmarks, or local-history sidecars to move or be removed
- **THEN** the sidecar service uses filesystem boundary helpers for listing, durable writing, moving, and cleanup

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

### Requirement: Sidecar filesystem helper cleanup is intentional
Bookmark, document-note, workspace-note, and local-history workflows SHALL either share a small helper for repeated sidecar filesystem mechanics or keep workflow-specific helpers when that is clearer. Any shared helper MUST have active callers and MUST only own filesystem mechanics such as listing candidate JSON sidecars, removing stale sidecar paths, or applying common directory-scan policy; workflow identity, filtering, merge, retention, and empty-document rules MUST remain in the owning service.

#### Scenario: Shared sidecar helper has active callers
- **WHEN** implementation extracts a shared sidecar filesystem helper
- **THEN** bookmark, document-note, workspace-note, or local-history code uses it for repeated filesystem mechanics
- **AND** the no-leftovers audit or final search evidence confirms the helper is not an unused public surface

#### Scenario: Workflow-specific sidecar helpers remain clear
- **WHEN** implementation determines a shared sidecar helper would obscure domain-specific rules
- **THEN** existing workflow-specific helper code remains in the owning services
- **AND** no new unused sidecar helper module, export, or function remains after the cleanup

### Requirement: Tests and benches use fixture filesystem helpers
Tests, property tests, fuzz corpus replay, widget tests, integration tests, and benchmarks SHALL use the internal test fixture filesystem helpers for fixture setup, disk assertions, sparse files, permission changes, symlinks, and cleanup. Direct raw filesystem calls in tests MUST be limited to the approved fixture/backend modules.

#### Scenario: Test fixture setup is readable without raw filesystem calls
- **WHEN** a test needs files, folders, symlinks, permissions, or disk content assertions
- **THEN** it uses fixture helper methods such as write text, write bytes, create directory, create symlink, set permissions, create sparse file, read text, or assert text
- **AND** the test body does not import `std::fs` or Unix filesystem extensions directly

#### Scenario: Bench fixtures follow the same helper surface
- **WHEN** a benchmark creates large directories, sparse files, or seeded document content
- **THEN** it uses the fixture filesystem helper surface
- **AND** benchmark setup does not normalize direct raw filesystem calls for future code to copy

### Requirement: Rules and skills enforce the filesystem boundary
Repository guidance SHALL encode the internal filesystem boundary as the required approach for future filesystem work. The root and relevant nested agent guidance, `.agents/rules` files, and filesystem-sensitive skills MUST instruct agents to use the boundary, flag direct raw filesystem access, and require the no-leftovers audit after filesystem changes.

#### Scenario: Rules document the approved boundary
- **WHEN** an agent reads the Rust, build, documentation, or local module guidance before filesystem work
- **THEN** the guidance identifies `services::filesystem` as the required production boundary
- **AND** it identifies the approved exception modules for backend and fixture code

#### Scenario: Skills trigger the boundary review
- **WHEN** a skill reviews or guides Rust work that touches file I/O, async file work, data safety, performance, scale, architecture, or comments
- **THEN** the skill checks that filesystem access goes through the internal boundary
- **AND** it asks for the no-leftovers audit when the change can introduce direct raw filesystem calls

### Requirement: No-leftovers audit is deterministic
The implementation SHALL provide deterministic audit commands that fail when disallowed direct filesystem access appears outside the approved filesystem implementation and test fixture boundary. The audit MUST cover production code, tests, benches, root and nested guidance, rules, and skills. The audit MUST also fail when a raw-backend crate that the boundary controls, such as `libc`, is declared in a crate manifest but has no matching backend usage in that crate's source, so a backend dependency cannot linger after the operations that needed it move to another backend.

#### Scenario: Audit fails on a direct std filesystem call
- **WHEN** a production service outside the approved filesystem modules contains a direct `std::fs` call
- **THEN** the no-leftovers audit reports the file and line
- **AND** the implementation is not considered complete

#### Scenario: Audit allows only explicit backend and fixture exceptions
- **WHEN** the audit encounters raw filesystem calls in approved backend or fixture modules
- **THEN** those occurrences are allowed only when documented by the audit allowlist
- **AND** every other occurrence is treated as a migration leftover

#### Scenario: Audit fails on a declared-but-unused raw backend dependency
- **WHEN** a crate manifest declares a controlled raw-backend dependency such as `libc` but no source file in that crate references it
- **THEN** the no-leftovers audit reports the unused backend dependency
- **AND** the implementation is not considered complete until the dependency is used or removed

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

### Requirement: No-leftovers audit covers helper drift and engine exceptions
The deterministic filesystem-boundary audit SHALL fail when production code introduces local status-probe wrappers, full-facts existence probes, unapproved filesystem engine adapters, or stale helper surfaces created by this future-proofing change. The audit MUST avoid false positives for domain methods that merely expose model state and for toolkit search-path APIs that are not filesystem probes.

#### Scenario: Local status helper drift is caught
- **WHEN** production code outside the approved filesystem modules defines a local `path_exists` helper or uses `file_facts(...).is_ok()` only as an existence probe
- **THEN** the no-leftovers audit reports the file and line
- **AND** implementation is not considered complete until the caller uses the lightweight metadata/status helper or documents why richer facts are required

#### Scenario: Engine exception drift is caught
- **WHEN** production code imports or calls filesystem-walking or file-reading engine APIs outside approved adapter modules
- **THEN** the no-leftovers audit reports the file and line
- **AND** implementation is not considered complete until the code is routed through `services::filesystem` or added as a documented approved engine adapter

#### Scenario: Audit avoids domain and toolkit false positives
- **WHEN** production code calls a domain model method such as `FileTreeItem::is_dir()` or a GTK/GIO search-path API
- **THEN** the audit does not report it as a filesystem-boundary violation
- **AND** the audit allowlist explains any non-obvious exception that remains necessary

### Requirement: No-leftovers audit covers polish-level filesystem drift
The deterministic filesystem-boundary audit SHALL catch polish-level leftovers after the completed rustix migration, including status-only `file_facts()` probes in tests, newly introduced local status wrappers, and unused sidecar helper surfaces created during cleanup.

#### Scenario: Test status-probe drift is caught
- **WHEN** a test or benchmark calls `file_facts(...).is_ok()` or `file_facts(...).is_err()` only as an existence probe
- **THEN** the no-leftovers audit reports the file and line
- **AND** implementation is not considered complete until the assertion uses a lightweight status helper or inspects rich facts

#### Scenario: New sidecar helper surface cannot linger unused
- **WHEN** cleanup introduces a new sidecar helper module, export, or function
- **THEN** the no-leftovers audit or final completion evidence confirms it has call sites
- **AND** implementation is not considered complete while an unused helper surface remains
