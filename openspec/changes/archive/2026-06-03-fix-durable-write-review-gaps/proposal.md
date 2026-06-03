## Why

The completed `harden-durable-file-writes` implementation materially improved save durability, but review found several edge cases where the new guarantees can still be broken: metadata is changed after the only temp-file sync, path locks are tied to replaceable inodes, symlink-backed saves can rewrite the link, cross-filesystem copy fallback loses source identity, and Replace All can still exhaust memory or spend excessive time rewriting its undo journal. These are all in the same data-safety family and should be fixed as one hardening pass, not left as follow-up debt.

## What Changes

- Tighten the atomic write helper so temp-file permissions are safe from creation, destination metadata changes are included in the final temp-file sync, and every write path still reports before-rename vs after-rename failures honestly.
- Replace inode-scoped `flock` coordination with stable path-level coordination that survives temp-file rename and covers editor save, Replace All, and undo writes for the same canonical target.
- Preserve symlink semantics for file-backed saves: saving a document opened through a symlink updates the resolved target instead of replacing the symlink, with explicit regression coverage.
- Make cross-filesystem durable copy fallback match rename semantics by preserving the source file's identity metadata on the destination.
- Make file load/save adapters safer under scale and concurrency: choose chunked save snapshots from the live buffer size, prevent stale async load results from applying after a newer load starts, and bound startup draft preload memory.
- Close the Replace All scale gaps: bound replaceable file sizes / total undo bytes, reduce full-file memory amplification, use the project's SIMD UTF-8 validation pattern, and persist undo journal state incrementally instead of rewriting the entire growing backup after every file.
- Add streaming durable-write support so JSON state and per-file Replace All journals do not need to materialize complete serialized documents before writing.
- Add realistic benchmarks and regression tests for the risky sizes: large single-file replace, 1,000+ touched files, journal-enabled Replace All, symlink saves, path-lock coordination, and metadata-sync ordering.
- Update docs, rules, and tests so the stronger durability, locking, symlink, copy-fallback, and Replace All scale contracts are captured and validated.

## Capabilities

### New Capabilities
- `durable-file-write-contract`: Low-level durable-write service contract for temp-file metadata ordering, stable path coordination, and durable copy fallback semantics.

### Modified Capabilities
- `document-save-safety`: File-backed saves must handle symlink-backed documents safely and retain the stronger dirty-state behavior for durability-unconfirmed writes.
- `draft-session-recovery`: Draft restore preloading must stay bounded so stale or very large draft files cannot exhaust memory before editor buffer accounting begins.
- `search-replace-safety`: Replace All must use the stable write coordination and enforce memory / journal bounds rather than relying on unbounded full-file and whole-backup rewrites.

## Impact

- Affected Rust services: `crates/lushtext-core/src/services/durable_write.rs`, `editor_io.rs`, `content_search/replace.rs`, `json_store.rs`, `draft_service.rs`, and related tests / property tests.
- Affected UI adapters: `crates/lushtext-core/src/ui/editor_page/load_save.rs`, window save / Save As handling, and any open-path bookkeeping that consumes canonical file identity.
- Affected docs and rules: `docs/durable-writes.md`, `AGENTS.md`, `.agents/rules/rust.md`, README durability wording if needed, and OpenSpec canonical specs before archive.
- No dependency change is required by default. If stable path locking needs an additional small synchronization helper crate, it must follow the workspace dependency, cargo-hakari, and Flatpak cargo-sources rules.
