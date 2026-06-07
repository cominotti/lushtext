## Why

LushText now has enough filesystem-heavy workflows that direct `std::fs`, Unix extension, and occasional low-level calls are making safety, durability, and review discipline harder than they need to be. This change creates one readable internal filesystem boundary so application code expresses intent directly while the implementation can use descriptor-oriented `rustix` primitives where they make operations safer or more precise.

## What Changes

- Introduce a new internal filesystem abstraction layer for all application, service, UI, and test filesystem access.
- Use `rustix` inside the filesystem implementation for Unix descriptor-owned operations, directory-relative traversal, metadata inspection, durable sync, atomic replacement primitives, and permission/ownership preservation.
- Preserve or improve current call-site readability by exposing LushText-named operations such as reading text snapshots, scanning workspace entries, writing durable replacements, creating sidecar files, renaming workspace items, and removing paths.
- **BREAKING** for internal code: direct `std::fs`, `std::os::unix::fs`, `std::os::unix::io`, direct filesystem `libc` calls, and direct `rustix` calls become forbidden outside the approved filesystem implementation and test-support boundary.
- Migrate every existing filesystem caller, including document load/save, durable writes, JSON stores, draft/session persistence, sidecars, local history, search/replace backup journals, file tree scanning, file peek, workspace management, watcher setup inputs, tests, benches, and helper scripts that inspect Rust source expectations.
- Add deterministic no-leftovers audit commands so the migration fails if raw filesystem access remains outside the approved boundary.
- Encode the boundary in repository guidance, rules, and relevant skills so future Rust work keeps using the abstractions.

## Capabilities

### New Capabilities

- `internal-filesystem-abstractions`: Defines the required internal filesystem boundary, the readability and safety guarantees it must provide, the full-repo migration requirement, and the rules/skills enforcement contract.

### Modified Capabilities

- `durable-file-write-contract`: Durable write behavior remains the same, but the implementation contract changes so durable writes are exposed through the internal filesystem boundary and no longer through ad hoc caller-owned filesystem operations.

## Impact

- Affected crates: `crates/lushtext-core` and `crates/lushtext`, especially `services/`, `model/` test helpers, UI workflows that currently call filesystem services, benches, and integration/widget tests.
- Affected dependencies: add `rustix` as a first-class workspace dependency with the needed filesystem, process, and std features; refresh Cargo lock/hakari metadata and Flatpak cargo sources.
- Affected documentation: root and nested agent guidance, `.agents/rules/*.md`, and filesystem-sensitive skills such as data safety, responsiveness/performance review, scale review, Rust architecture, and Rust comments.
- Affected validations: OpenSpec validation, Rust formatting/lints/tests, property/fuzz/widget lanes where relevant, cargo dependency refreshes, agent-doc validation, and explicit source audits proving there are no leftover direct filesystem calls outside the approved boundary.
