## Context

LushText persists desktop preferences through GSettings and app-owned state through pretty JSON under `$XDG_DATA_HOME/lushtext`. Draft and local-history bodies stay as plain UTF-8 files. Recent filesystem-boundary work already gives JSON writes a strong crash-durability contract, and recovery metadata handling already preserves malformed session, draft, sidecar, local-history, Replace All, and migration-ledger state before replacement.

The remaining gap is format evolution. Most JSON files are direct serde values without a shared envelope, document kind, or explicit format version. Some shapes have targeted compatibility, such as workspace legacy normalization and `#[serde(default)]` fields, but those are pre-public conveniences rather than a contract we need to carry forever. A public-era LushText release should be able to explain every long-lived app-data document it sees: what it is, which version it uses, whether it is supported, and whether unsupported or damaged data was preserved.

`docs/next/persistent-format-hardening.md` records the storage direction: keep pretty JSON for app-owned source-of-truth files, keep text bodies as text, and defer SQLite until cross-document indexing or global query features create database-shaped pressure.

## Goals / Non-Goals

**Goals:**

- Define a shared versioned pretty-JSON contract for long-lived app-owned JSON documents.
- Make a clean break from existing bare JSON files in runtime app code.
- Extend recovery-aware loading to `workspaces.json` and `saved-searches.json`.
- Keep low-value recent search history allowed to default to empty while logging diagnostics.
- Use explicit stable hashing for path-backed draft IDs in the new format.
- Allow at most an optional one-shot migration helper under `scripts/migrations/` if pre-public app data needs manual conversion.
- Add golden fixtures and generated malformed-input coverage for format migration and recovery.
- Keep JSON writes on the existing durable filesystem boundary.

**Non-Goals:**

- Do not migrate current persistence to TOML.
- Do not introduce SQLite as part of this change.
- Do not store draft bodies or local-history snapshot bodies inside JSON envelopes.
- Do not change user-facing retention policy for local history.
- Do not add permanent runtime readers for pre-public app-data JSON shapes.
- Do not embed legacy migration logic in normal application startup.
- Do not make recent search history as durable as user-managed saved searches.

## Decisions

### Decision: Use versioned envelopes for long-lived JSON documents

Long-lived app-owned JSON documents should save in this shape:

```json
{
  "kind": "dev.cominotti.lushtext.<document-kind>",
  "version": 1,
  "data": {}
}
```

The exact Rust representation can be generic or per-document, but the on-disk shape needs stable `kind`, integer `version`, and `data` fields. `kind` prevents one file from being parsed as another document type after a support copy or user mistake. `version` gives migrations a precise branch point. `data` keeps the actual domain model isolated from envelope metadata.

Alternative considered: add only `version` inside each existing model. Rejected because it does not identify the document kind and repeats envelope semantics in every model.

### Decision: Make the runtime format a clean break

Runtime loaders should require the v1 envelope for long-lived JSON documents. Bare pre-public JSON should be treated as unsupported metadata, not as a supported compatibility format. Recovery-aware loaders should preserve unsupported old-shape files through quarantine or in-place diagnostics before writing v1 defaults, then keep the app usable with the documented default state for that metadata class.

Alternative considered: keep legacy bare-JSON readers in every service. Rejected because the user explicitly wants a clean break and because carrying pre-public compatibility branches would make the public format harder to reason about.

### Decision: Keep any old-data bridge outside runtime startup

If there is enough value in converting pre-public app data, the bridge should be an optional script under `scripts/migrations/`. The script may read known old shapes and write v1 envelopes, but the app runtime should not depend on it and should not silently run it on startup.

Alternative considered: automatic startup migration. Rejected because it still embeds legacy interpretation in normal app behavior and turns a clean public format into a hidden compatibility layer.

Implementation decision: no one-shot migration helper is added for this change. LushText has not yet announced a public app-data contract, and unsupported pre-public files are preserved by recovery diagnostics before v1 defaults replace them when safe.

### Decision: Keep the domain models mostly envelope-free

Domain structs such as `WorkspacesFile`, `SessionData`, `DraftManifest`, `BookmarkDocument`, `DocumentNoteDocument`, `WorkspaceNoteDocument`, `LocalHistoryDocument`, and saved-search values should remain focused on application semantics. Envelope parsing, version dispatch, and migration belong in persistence helpers or service-local storage modules.

Alternative considered: add `version` and `kind` fields to every domain struct. Rejected because UI and service code would carry persistence metadata unrelated to normal domain behavior.

### Decision: Treat saved searches as durable user-managed state

Recent search history may remain ephemeral: if it is corrupt, the search panel can start with an empty recent list and log a diagnostic. Saved searches are different because the user explicitly named them and expects them to survive. Saved-search persistence should therefore use recovery-aware loading, quarantine before replacement, and visible grouped diagnostics when data cannot be loaded.

Alternative considered: keep both recent history and saved searches as empty-on-corrupt lists. Rejected because it silently discards user-managed saved searches.

### Decision: Bring workspaces into recovery-aware loading

`workspaces.json` is one of the first app-owned files a user notices. A malformed or pre-public workspace file should not silently become an empty sidebar. The runtime workspace loader should use recovery metadata handling, preserve malformed or unsupported paths before replacement, require the public v1 envelope, and surface a grouped recovery diagnostic while keeping the app usable. Any old-shape conversion belongs only in an optional one-shot script under `scripts/migrations/`.

Alternative considered: keep `unwrap_or_default()` at the sidebar edge. Rejected because it hides a recoverable state problem as ordinary empty state.

### Decision: Use explicit stable hashing for persisted identifiers

Persisted IDs must not depend on process-randomized or implementation-defined hashers. Path-backed draft IDs in the v1 draft manifest should use the explicit stable hash helper already used by sidecar identities or an equally explicit documented algorithm.

Alternative considered: leave `DefaultHasher` because the manifest stores the draft ID. Rejected because fallback derivation from path should remain deterministic across Rust versions and process launches.

### Decision: SQLite remains a future index/cache, not source of truth

This change should not add SQLite. If future features need global notes/bookmark search, workspace-wide history timelines, persistent command-palette file indexes, or sync/change journals, SQLite can be introduced as a metadata index or cache while JSON/text files remain inspectable source-of-truth records.

Alternative considered: move all app-owned JSON to SQLite now. Rejected because current state is small-document oriented and recovery/debuggability would get harder before query performance demands it.

### Decision: Golden fixtures are part of the format contract

Each long-lived JSON class should have fixture coverage for valid v1, unknown fields, missing optional fields, malformed input, unsupported old-shape input, unsupported file kind where applicable, oversized input where applicable, and optional migration-script output if such a script is created. These fixtures should live with tests or under a clearly named fixture directory so future migrations can add v2 without rewriting v1 evidence.

Alternative considered: rely on round-trip serde tests only. Rejected because round-trips do not prove compatibility with old files, malformed files, or future unknown fields.

## Risks / Trade-offs

- [Risk] Envelope support creates generic abstraction that hides workflow-specific recovery decisions. -> Mitigation: share only envelope/version dispatch and durable save helpers; keep document-specific migration and repair decisions in owning services.
- [Risk] Clean break resets pre-public app state for users who already have local LushText data. -> Mitigation: preserve unsupported old files with diagnostics before replacement and optionally provide a manual `scripts/migrations/` converter if the old data is worth carrying forward.
- [Risk] Changing path-backed draft IDs could orphan existing pre-public draft files. -> Mitigation: runtime v1 uses stable IDs; optional migration tooling can convert old manifests, while unsupported old manifests are preserved instead of overwritten silently.
- [Risk] Versioned sidecars might make note/bookmark listing slower if every load performs heavy format probing. -> Mitigation: keep the runtime path to one v1 envelope parse plus recovery classification, avoid legacy fallback branches, and retain current bounded scans.
- [Risk] Saved-search corruption now produces visible recovery warnings where it previously disappeared. -> Mitigation: group warnings with existing recovery diagnostics and keep the panel usable for new searches.
- [Risk] Specifying SQLite deferral could be read as "never SQLite." -> Mitigation: document concrete trigger points for SQLite as an index/cache layer when global query pressure appears.

## Migration Plan

1. Add a shared persistence-format helper that reads a versioned envelope, validates `kind`, dispatches on supported `version`, and saves pretty JSON through the existing durable write path.
2. Reject bare or wrong-kind JSON as unsupported metadata through recovery-aware loading instead of parsing it as a legacy runtime format.
3. Convert `workspaces.json` first because it currently has a legacy reader but weak recovery behavior.
4. Convert saved searches while leaving recent search history intentionally low-stakes.
5. Convert session and draft manifest loading/saving, including stable path-backed draft IDs for v1.
6. Convert bookmark, document-note, workspace-note, local-history index, migration-ledger, and Replace All undo metadata where they are long-lived or recovery-owned.
7. Add grouped diagnostics where newly recovery-aware loads can reach UI surfaces.
8. Add golden fixtures, malformed-input tests, and targeted widget/integration coverage for visible recovery paths.
9. Add an optional `scripts/migrations/` converter only if manual pre-public data conversion is judged useful after implementation starts.

Rollback before release is a normal revert of the implementation. After release, rollback must keep v1 envelope readers in place because v1 becomes the public app-data contract.

## Open Questions

- Should every v1 envelope include an optional `written_by_version` field for support triage, or should that stay out of source-of-truth data to reduce churn?
- Should recent search history remain bare JSON permanently, or adopt the envelope while keeping empty-on-corrupt semantics?
- Where should golden fixture files live so they are easy to audit but do not bloat normal source navigation?
