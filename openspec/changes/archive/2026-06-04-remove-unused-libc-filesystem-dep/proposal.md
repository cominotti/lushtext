## Why

The `complete-rustix-filesystem-boundary` change finished migrating the private filesystem backend to `rustix`, including the Linux xattr operations that were the last reason to keep direct `libc`. As a result `libc` is now referenced by zero Rust source files anywhere in the workspace, yet it is still declared as a dependency and still named in the backend module's doc comment. That is the exact "no unused abstractions / no leftovers" outcome the prior change set out to guarantee, but the boundary audit cannot see it because it only scans source for `libc::` usage, not manifests.

## What Changes

- Remove the now-unused `libc` dependency from `crates/lushtext-core/Cargo.toml` and the workspace root `[workspace.dependencies]` (nothing else in the workspace uses it).
- Update the `services::filesystem::sys` module doc comment so it no longer claims `libc` lives in the backend, matching the rustix-only reality.
- Regenerate the cargo-hakari workspace-hack and the Flatpak `cargo-sources.json` so the dependency-graph artifacts stay consistent after the removal.
- Strengthen `scripts/check-filesystem-boundary.sh` so it fails when a raw-backend crate such as `libc` is declared in a crate manifest but has no matching `::` usage in that crate's source, closing the audit gap that let this leftover survive.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `internal-filesystem-abstractions`: Extend the deterministic no-leftovers audit so it also fails on a declared-but-unused raw-backend dependency, not only on direct raw filesystem calls.

## Impact

- Affected code: `Cargo.toml` (workspace root), `crates/lushtext-core/Cargo.toml`, `crates/lushtext-core/src/services/filesystem/sys.rs` (doc comment only), `crates/workspace-hack/**`, `build-aux/cargo-sources.json`, `scripts/check-filesystem-boundary.sh`.
- Dependencies: removes the direct `libc` dependency; rustix remains the sole private Unix backend dependency.
- Behavior: no user-facing behavior changes and no durable-write contract changes; this is a dependency-hygiene and audit-completeness change that finishes the prior rustix boundary migration.
