## MODIFIED Requirements

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

## ADDED Requirements

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
