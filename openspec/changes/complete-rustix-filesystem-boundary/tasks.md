## 1. Inventory and Final Boundary Shape

- [x] 1.1 Inventory all direct `std::fs`, Unix filesystem extension, direct filesystem `libc`, direct `rustix`, `.canonicalize()`, and `.exists()` occurrences across production code, tests, benches, rules, skills, and guidance.
- [x] 1.2 Inventory all public `services::filesystem` helpers and record which have production, test, or benchmark callers.
- [x] 1.3 Decide the final private backend module layout for shared platform primitives, including confirming that no unavoidable Linux-only `libc` xattr gap remains.
- [x] 1.4 Decide whether durable namespace helpers remain under `filesystem::write` or move to a clearer filesystem operation family, then record the decision in code comments or guidance as appropriate.

## 2. Shared Filesystem Backend Primitives

- [x] 2.1 Move or add private backend helpers for temp-file creation with mode, atomic rename, unlink cleanup, read bytes, metadata probing, canonical identity, file length, directory sync, and parent-directory sync.
- [x] 2.2 Replace direct Unix extension or `libc` calls with `rustix` where rustix supports the needed filesystem operation safely.
- [x] 2.3 Confirm no direct `libc` filesystem calls remain because rustix covers the required Linux xattr and ACL-preservation operations.
- [x] 2.4 Keep ordinary caller-facing return values as app-facing types and `std::io::Error` or the chosen app-facing filesystem error wrapper, with no leaked rustix errno types.

## 3. Durable Write Consolidation

- [x] 3.1 Refactor `durable_write` so its raw filesystem actions delegate to the shared private filesystem backend primitives.
- [x] 3.2 Preserve the atomic replacement sequence: temp in destination directory, content write, flush, required metadata application, final temp sync after metadata, rename, and parent-directory sync.
- [x] 3.3 Preserve before-rename and after-rename failure classification, including temp cleanup before rename and durability-unconfirmed reporting after rename.
- [x] 3.4 Preserve stable target identity and write coordination for editor save, Save As, Replace All, and Replace All undo.
- [x] 3.5 Preserve durable copy fallback behavior, including source metadata inheritance, source retention until destination durability completes, and source parent sync after cleanup.
- [x] 3.6 Preserve streaming durable writes for JSON and other persistence callers without adding extra full-buffer allocations.

## 4. Public Filesystem API Cleanup

- [x] 4.1 Consolidate duplicate rename, durable rename, parent sync, directory creation, removal, target identity, and write coordination helpers so each safety policy has one clear public entry point.
- [x] 4.2 Update editor save, Replace All, local history, drafts, JSON persistence, style-scheme writes, sidebar create/rename/delete, notes, bookmarks, saved searches, tests, and benches to use the final helper names.
- [x] 4.3 Adopt `filesystem::sidecar` across bookmark, document-note, workspace-note, and local-history filesystem mechanics, or remove it if workflow-specific helpers are clearer.
- [x] 4.4 Adopt `FilesystemError` for operation/path context where useful, or remove the wrapper and public export if `std::io::Error` plus `anyhow::Context` remains the intentional contract.
- [x] 4.5 Remove any transition aliases, compatibility wrappers, unused helper functions, unused imports, and stale comments created or uncovered during the migration.

## 5. Audits, Guidance, and Specs

- [x] 5.1 Strengthen `scripts/check-filesystem-boundary.sh` so it reflects the final backend allowlist and fails on raw backend imports outside approved modules.
- [x] 5.2 Add deterministic no-leftovers checks for direct durable-write implementation imports and stale exported filesystem helper surfaces introduced or evaluated by this change.
- [x] 5.3 Update `AGENTS.md`, nested guidance, and `.agents/rules/rust.md` so approved raw filesystem exceptions match the final backend shape.
- [x] 5.4 Update filesystem-sensitive skills or local guidance that still name obsolete exception modules, helper names, or validation commands.
- [x] 5.5 Ensure the delta specs remain aligned with implementation decisions before archive, especially the decision that no direct `libc` backend gap remains.

## 6. Validation and Closure

- [x] 6.1 Run `cargo fmt --check`.
- [x] 6.2 Run `./scripts/check-filesystem-boundary.sh`.
- [x] 6.3 Run targeted Rust tests for filesystem boundary, durable write, editor save, Replace All, notes/bookmarks sidecars, local history, drafts, and JSON persistence.
- [x] 6.4 Run the broader repo validation stack appropriate for filesystem-sensitive Rust changes, including `cargo test` and any existing release/helper tests that cover filesystem persistence.
- [x] 6.5 Run `openspec validate complete-rustix-filesystem-boundary --strict`.
- [x] 6.6 Run `openspec validate --changes --strict` and `openspec validate --specs --strict`.
- [x] 6.7 Run final searches proving no raw filesystem leftovers, duplicate helper leftovers, direct durable-write imports, unused sidecar helper surface, or unused filesystem error wrapper remain.
- [x] 6.8 Update this checklist as tasks complete and leave the change apply-ready with all implementation evidence captured.
