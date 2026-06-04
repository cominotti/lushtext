## Why

LushText already has a strong public filesystem boundary, but the private implementation still has split raw-platform islands and partially adopted helper abstractions. This change finishes the rustix/filesystem migration so the codebase has one coherent, reusable filesystem backend and no unused or bypassed abstractions left behind.

## What Changes

- Consolidate raw filesystem, Unix extension, direct `libc`, and direct `rustix` usage behind the filesystem boundary's approved private backend modules.
- Make `rustix` the private backend for descriptor-owned Unix filesystem operations used by LushText, including the Linux metadata operations needed by durable writes.
- Move durable-write platform operations behind the same private filesystem backend instead of letting durable writes own a second raw `std::fs`/Unix/`libc` island.
- Resolve overlapping mutation APIs so rename, durable rename, parent sync, directory creation, removal, target identity, and write coordination each have one clear public home.
- Either adopt the existing sidecar filesystem helper surface across bookmark, document-note, workspace-note, and local-history workflows, or remove it if the already-used note/local-history helpers are the better abstraction.
- Either adopt the filesystem error wrapper for app-facing operation/path context or remove it if `std::io::Error`/`anyhow::Context` remains the intentional contract.
- Strengthen the no-leftovers audit so unused boundary abstractions, duplicate public operation families, and direct backend imports cannot remain after implementation.
- Preserve existing save, Replace All, draft, JSON, local-history, sidecar, test fixture, and benchmark behavior while completing the internal architecture.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `internal-filesystem-abstractions`: Tighten the boundary contract to require one coherent private platform backend, rustix-first backend adoption, no unused helper abstractions, no duplicate public operation families, and deterministic audits for those leftovers.
- `durable-file-write-contract`: Require durable writes to be implemented through the same filesystem backend platform primitives while preserving all existing atomic-write, metadata-preservation, sync, failure-classification, stable-target, and streaming guarantees.

## Impact

- Affected code: `crates/lushtext-core/src/services/filesystem/**`, `crates/lushtext-core/src/services/durable_write.rs`, filesystem callers in editor save, Replace All, local history, notes/bookmarks, drafts, JSON persistence, sidebar mutation flows, tests, benches, and boundary-audit scripts.
- Affected guidance: root/nested `AGENTS.md`, `.agents/rules/rust.md`, and any filesystem-sensitive skills or docs that name approved raw filesystem exceptions.
- Dependencies: `rustix` remains the private backend dependency; direct `libc` is not part of the final filesystem backend because rustix covers the required xattr and ACL-preservation operations.
- Behavior: no user-facing feature behavior changes are intended; this is a correctness, maintainability, and architecture-completion change.
