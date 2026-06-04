## Why

The rustix filesystem migration is now structurally strong, but the next risks are at the edges: specialized filesystem engines, cheap path-status probes, descriptor-relative traversal hardening, and audit coverage for subtle bypasses. This change makes those edges explicit so future filesystem work can move faster without reopening raw-platform ambiguity.

## What Changes

- Treat `services::filesystem` as the default filesystem boundary while explicitly documenting which non-raw engine adapters may perform their own filesystem traversal or reads, starting with the ripgrep/ignore content-search stack.
- Add small, reusable metadata/status helpers so callers can ask "does this path exist?", "what kind is it?", or "is this directory?" without using full `file_facts()` snapshots or creating local `path_exists` wrappers.
- Harden the private Unix backend toward descriptor-relative traversal where rustix supports it, especially directory child metadata during workspace-style scans.
- Extend deterministic audits so local path-probe helper drift and engine-adapter exceptions are visible, intentional, and reviewable.
- Update rules/guidance so future contributors know when to use the filesystem boundary, when a specialized engine is allowed, and what proof is required before adding another exception.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `internal-filesystem-abstractions`: Clarify engine-adapter exceptions, require lightweight metadata/status helpers, strengthen rustix descriptor-relative backend use, and expand no-leftovers audits for subtle path probes and exception drift.

## Impact

- Affected code: `crates/lushtext-core/src/services/filesystem/**`, `crates/lushtext-core/src/services/content_search/search.rs`, callers that currently use `file_facts(...).is_ok()` or local `path_exists` helpers, and focused filesystem tests/benches.
- Affected validation: `scripts/check-filesystem-boundary.sh`, agent/rule guidance, and strict OpenSpec validation.
- Dependencies: no new dependency expected; `rustix` remains the private Unix backend and the existing ripgrep/ignore stack remains the content-search engine.
- Behavior: no user-facing behavior changes intended; this is boundary hardening and future-proofing.
