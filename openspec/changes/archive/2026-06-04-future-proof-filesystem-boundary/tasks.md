## 1. Inventory and Final API Shape

- [x] 1.1 Inventory production `file_facts(...).is_ok()`, local `path_exists` helpers, and status-only kind checks under `crates/lushtext-core/src` and record which callers need cheap status versus rich facts.
- [x] 1.2 Inventory filesystem engine imports and calls, including `ignore::WalkBuilder`, `grep_searcher::SearcherBuilder`, and `search_path`, and confirm content search is the only intended engine-adapter exception.
- [x] 1.3 Inspect `filesystem::sys::visit_directory_entries` and confirm which rustix descriptor-relative metadata API should replace joined-path child metadata on Unix.
- [x] 1.4 Choose the final public metadata status shape (`PathStatus`, `Option<FileKind>`, or narrow helper functions) based on the inventoried caller readability.

## 2. Lightweight Metadata Status Helpers

- [x] 2.1 Add the selected lightweight status type or helper functions under `services::filesystem::metadata` without exposing raw platform metadata.
- [x] 2.2 Add unit tests proving missing paths, files, directories, symlinks or other kinds, and permission/error behavior are reported through the new status helpers.
- [x] 2.3 Ensure status helpers avoid unnecessary canonicalization, byte-size collection, and mtime collection when callers only need existence or kind.
- [x] 2.4 Migrate production callers from local `path_exists` helpers and status-only `file_facts(...).is_ok()` probes to the new metadata/status helpers.
- [x] 2.5 Keep callers that genuinely need canonical identity, byte size, or mtime on `file_facts()` or richer snapshot helpers, and document any intentional remaining full-facts probes.

## 3. Specialized Engine Adapter Contract

- [x] 3.1 Add a short code comment or local module note in content search explaining why the ripgrep/ignore stack is an approved read-only filesystem engine adapter.
- [x] 3.2 Document the allowed content-search engine operations: parallel walking, gitignore/glob filtering, binary detection, regex matching, streaming line reads, cancellation, and progress reporting.
- [x] 3.3 Verify Replace All writes, undo backup persistence, backup cleanup, and any sidecar or persistence operations still route through `services::filesystem`.
- [x] 3.4 Add or update tests/evidence covering content-search reads remaining read-only while Replace All and undo writes use the durable filesystem write boundary.

## 4. Descriptor-Relative Traversal Hardening

- [x] 4.1 Update the Unix backend directory visitor to obtain child metadata through descriptor-relative rustix operations when supported.
- [x] 4.2 Preserve public `filesystem::tree` and `services::file_tree` entry shapes, sorting, hidden-file filtering, truncation, and cancellation behavior.
- [x] 4.3 Add focused tests for hidden entries, disappeared children, unreadable children, symlinks or other file kinds, and missing directories after the traversal change.
- [x] 4.4 Confirm backend-specific rustix errors still convert to `std::io::Error` or app-facing service errors before leaving the filesystem boundary.

## 5. Audit and Guidance Updates

- [x] 5.1 Extend `scripts/check-filesystem-boundary.sh` to report local `fn path_exists` helpers and status-only `file_facts(...).is_ok()` probes outside approved modules.
- [x] 5.2 Extend the audit with an explicit approved engine-adapter allowlist for content search and a failure path for new filesystem-walking or file-reading engine imports outside that allowlist.
- [x] 5.3 Keep audit false positives low by excluding domain state methods such as `FileTreeItem::is_dir()` and toolkit search-path APIs that are not filesystem probes.
- [x] 5.4 Update `AGENTS.md`, relevant nested guidance, `.agents/rules/rust.md`, and filesystem-sensitive skills so they describe metadata status helpers and approved engine-adapter exceptions.
- [x] 5.5 Add no-leftovers checks for any helper surfaces introduced by this change so temporary aliases or unused status helpers cannot remain.

## 6. Validation and OpenSpec Closure

- [x] 6.1 Run `./scripts/check-filesystem-boundary.sh` and confirm it catches a temporary local status-probe leftover and a temporary unapproved engine import before reverting those probes.
- [x] 6.2 Run targeted Rust tests for `services::filesystem`, `services::file_tree`, content search, Replace All, search backup, and any migrated UI path-status callers.
- [x] 6.3 Run `cargo fmt --check`.
- [x] 6.4 Run `cargo test -p lushtext-core` with the relevant feature flags used by filesystem/property coverage if needed.
- [x] 6.5 Run `openspec validate future-proof-filesystem-boundary --strict`.
- [x] 6.6 Run `openspec validate --changes --strict` and `openspec validate --specs --strict`.
- [x] 6.7 Run final searches proving no raw filesystem leftovers, local `path_exists` wrappers, status-only full-facts probes, unapproved engine adapters, or stale helper surfaces remain.
