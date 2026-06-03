# internal-filesystem-abstractions Specification

## Purpose
Define LushText's internal filesystem boundary so production code, tests, guidance, and audit tooling use readable application-level filesystem operations instead of raw platform APIs.

## Requirements
### Requirement: Production filesystem access uses the internal boundary
All production code SHALL route filesystem reads, metadata probes, path identity checks, directory traversal, file creation, durable writes, renames, removals, sidecar operations, and workspace file operations through the LushText internal filesystem boundary. Direct `std::fs`, Unix filesystem extensions, direct filesystem `libc`, direct `rustix`, and direct path filesystem probes MUST NOT appear outside the approved filesystem implementation modules.

#### Scenario: Migrated production code has no raw filesystem leftovers
- **WHEN** the implementation migration is complete
- **THEN** a source audit over production Rust code finds no direct `std::fs`, Unix filesystem extension, direct filesystem `libc`, direct `rustix`, or direct path filesystem probe usage outside the approved filesystem implementation modules

#### Scenario: New production filesystem caller uses the boundary
- **WHEN** a service or UI workflow needs to read, inspect, create, rename, remove, scan, or write a filesystem path
- **THEN** it calls a LushText filesystem operation that names the application intent
- **AND** it does not import raw filesystem APIs directly

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
The implementation SHALL provide deterministic audit commands that fail when disallowed direct filesystem access appears outside the approved filesystem implementation and test fixture boundary. The audit MUST cover production code, tests, benches, root and nested guidance, rules, and skills.

#### Scenario: Audit fails on a direct std filesystem call
- **WHEN** a production service outside the approved filesystem modules contains a direct `std::fs` call
- **THEN** the no-leftovers audit reports the file and line
- **AND** the implementation is not considered complete

#### Scenario: Audit allows only explicit backend and fixture exceptions
- **WHEN** the audit encounters raw filesystem calls in approved backend or fixture modules
- **THEN** those occurrences are allowed only when documented by the audit allowlist
- **AND** every other occurrence is treated as a migration leftover
