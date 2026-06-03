## Baseline Direct Filesystem Inventory

Captured while applying `adopt-internal-filesystem-abstractions`.

Search pattern:

```sh
rg -n "std::fs::|use std::fs|std::os::unix::fs|std::os::unix::io|libc::|rustix::|\\.canonicalize\\(" \
  crates/lushtext-core/src crates/lushtext/src crates/lushtext-core/tests crates/lushtext/tests \
  crates/lushtext-core/benches crates/lushtext/benches AGENTS.md crates/**/AGENTS.md \
  .agents/rules .agents/skills
```

## Scope Counts

- `crates/lushtext-core/src`: 336 hits
- `crates/lushtext/src`: 0 hits
- `crates/lushtext-core/tests`: 13 hits
- `crates/lushtext/tests`: 167 hits
- `crates/lushtext-core/benches`: 26 hits
- `crates/lushtext/benches`: 0 hits
- `.agents/rules`: 0 hits
- `.agents/skills`: 21 hits
- `AGENTS.md`: 3 hits

## Highest-Volume Files

- `crates/lushtext-core/src/services/durable_write.rs`: 93 hits
- `crates/lushtext/tests/widget/window.rs`: 80 hits
- `crates/lushtext-core/src/services/editor_io.rs`: 38 hits
- `crates/lushtext-core/src/services/file_tree.rs`: 36 hits
- `crates/lushtext-core/src/services/palette/tests.rs`: 29 hits
- `crates/lushtext-core/benches/benchmarks.rs`: 26 hits
- `crates/lushtext/tests/widget/workspace_section.rs`: 24 hits
- `crates/lushtext/tests/widget/app.rs`: 19 hits
- `crates/lushtext-core/src/services/draft_service.rs`: 18 hits
- `crates/lushtext-core/src/services/local_history_service.rs`: 15 hits

## Operation-Family Classification

- Backend-only exception candidate: `durable_write.rs` currently owns temp-file replacement, metadata copy, xattr/ACL preservation, parent-directory sync, and write coordination. The migration should move or privatize this under `services::filesystem::write` rather than weaken it.
- Read and metadata: `editor_io`, `json_store`, `draft_service`, `file_peek`, `content_search`, local history, notes/bookmarks sidecars, window metadata refresh, and tests use direct reads, metadata, and `read_to_string`.
- Canonical identity: editor save/Save As, note storage, bookmark/workspace-note identity, palette indexing, local history, and tests use direct `canonicalize()`.
- Traversal: file tree scanning, palette indexing, content search, draft orphan cleanup, local-history listing, sidecar listing, and tests use direct `read_dir`.
- Creation and durable writes: JSON stores, drafts, session/workspace persistence, sidecars, local history snapshots, Replace All, sidebar new-file/new-folder flows, tests, and benches create files and directories directly.
- Rename/remove: sidebar actions, sidecar migration, local history migration, Replace All undo, tests, and durable copy fallback use direct rename/remove calls.
- Test/bench fixture setup and assertions: most external direct calls are fixtures, sparse files, permission/symlink setup, and disk-content assertions.
- Guidance/skill leftovers: root `AGENTS.md` and several skill reference files preserve stale `std::fs` examples that must be rewritten after the boundary exists.

## Migration Implication

The migration must add both a production filesystem boundary and fixture helpers. Treating tests as an exception would leave too many direct-call examples for future work to copy.
