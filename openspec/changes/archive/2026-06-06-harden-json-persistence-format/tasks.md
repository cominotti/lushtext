## 1. Shared Format Contract

- [x] 1.1 Add a GTK-free JSON format helper that reads a `kind`/`version`/`data` envelope and rejects bare, wrong-kind, or unsupported-version JSON as unsupported format state.
- [x] 1.2 Add durable save helpers that write pretty v1 envelopes through the existing filesystem write boundary without bypassing `json_store`/recovery safety rules.
- [x] 1.3 Define stable document-kind constants and v1 payload wrappers for each long-lived JSON class covered by this change.
- [x] 1.4 Extend recovery diagnostics with unsupported-format and unsupported-version categories while preserving existing malformed, unreadable, unsupported-kind, and oversized behavior.
- [x] 1.5 Ensure unsupported old-shape JSON is preserved or left untouched before any v1 replacement is written.

## 2. Workspace And Search State

- [x] 2.1 Convert `workspaces.json` save/load to the v1 workspace envelope and remove runtime legacy multi-root/bare-JSON readers.
- [x] 2.2 Route workspace load failures through recovery-aware diagnostics instead of `unwrap_or_default()` at the sidebar edge.
- [x] 2.3 Preserve unsupported workspace metadata before resetting to empty v1 workspace state.
- [x] 2.4 Convert `saved-searches.json` to the v1 saved-searches envelope with recovery-aware load and replacement safety.
- [x] 2.5 Keep recent search history intentionally low-stakes while adding diagnostics for malformed or unsupported recent-history data.

## 3. Session And Draft State

- [x] 3.1 Convert `session.json` to the v1 session envelope while preserving current ordered-save and startup recovery behavior.
- [x] 3.2 Convert `drafts/manifest.json` to the v1 draft-manifest envelope while preserving bounded preload, repair diagnostics, and cleanup safety.
- [x] 3.3 Replace path-backed draft ID derivation with an explicit stable hash algorithm for v1 draft state.
- [x] 3.4 Ensure unsupported pre-public session or draft manifest JSON is preserved before replacement and is not parsed by permanent runtime legacy readers.
- [x] 3.5 Update grouped startup recovery feedback so unsupported session/draft format diagnostics remain visible without blocking unaffected tabs.

## 4. Sidecars, History, And Journals

- [x] 4.1 Convert bookmark sidecars to v1 envelopes while preserving empty-sidecar deletion, in-app rename migration, duplicate reconciliation, and corruption isolation.
- [x] 4.2 Convert document-note sidecars to v1 envelopes while preserving empty-note deletion, markdown body persistence, in-app rename migration, and corruption isolation.
- [x] 4.3 Convert workspace-note sidecars to v1 envelopes while preserving root-identity behavior, remove-and-readd restore, root rename migration, and corruption isolation.
- [x] 4.4 Convert local-history `index.json` files to v1 envelopes while keeping snapshot bodies as plain `.txt` files.
- [x] 4.5 Convert migration-ledger JSON and Replace All undo journal metadata to v1 envelopes where they are recovery-owned long-lived metadata.
- [x] 4.6 Ensure unsupported old-shape sidecars, history indexes, ledgers, and journals are preserved before replacement and do not block unrelated valid state.

## 5. Optional Migration Tooling And Documentation

- [x] 5.1 Decide whether current pre-public app data warrants a one-shot migration helper; record the decision in the change notes or tasks.
  - Decision: no one-shot script is added for this clean-break pass; unsupported pre-public metadata is preserved by recovery diagnostics before v1 defaults replace it when safe.
- [x] 5.2 If needed, add the one-shot converter under `scripts/migrations/` and keep it out of runtime startup paths.
  - Not needed for this change; no runtime or script migration path was added.
- [x] 5.3 If a migration helper is added, cover its output with fixtures and document how to run it safely.
  - Not applicable because no migration helper was added.
- [x] 5.4 Update `docs/next/persistent-format-hardening.md` if implementation decisions change the clean-break contract, SQLite deferral guidance, or migration-script stance.
- [x] 5.5 Update developer docs or agent guidance only if new format-helper rules would otherwise be easy to miss.
  - No agent guidance update was needed beyond the OpenSpec artifacts and `docs/next` note.

## 6. Fixtures And Tests

- [x] 6.1 Add golden v1 fixtures for workspace, saved searches, session, draft manifest, bookmark sidecar, document-note sidecar, workspace-note sidecar, local-history index, migration ledger, and Replace All undo metadata.
- [x] 6.2 Add unsupported old-shape, wrong-kind, unsupported-version, malformed, missing optional field, unknown field, and oversized fixtures where each metadata class applies.
- [x] 6.3 Add service tests proving valid v1 loads, unsupported old-shape preservation, replacement safety, and v1 save output for each converted class.
- [x] 6.4 Add generated malformed-input coverage proving the envelope and recovery loaders return diagnostics without panics.
- [x] 6.5 Add targeted integration or widget coverage for visible workspace, saved-search, and startup recovery diagnostics introduced by this change.
- [x] 6.6 Add tests proving stable path-backed draft IDs do not depend on process-randomized hash seeds.

## 7. Validation

- [x] 7.1 Run formatting for all touched Rust code.
- [x] 7.2 Run targeted service tests for JSON format helpers, recovery metadata, workspaces, saved searches, session/drafts, sidecars, local history, migration ledger, and Replace All undo metadata.
- [x] 7.3 Run any targeted widget tests covering newly visible grouped diagnostics.
- [x] 7.4 Run the filesystem-boundary audit if persistence call paths or write helpers changed.
- [x] 7.5 Run `cargo test -p lushtext-core --lib` or the closest broader Rust test gate justified by the implementation scope.
- [x] 7.6 Run `openspec validate harden-json-persistence-format --strict`.
- [x] 7.7 Run `openspec validate --all --strict` before archive or publication.
