## Why

LushText already relies on a broad set of persistence and recovery behaviors under `$XDG_DATA_HOME/lushtext`, but that contract is only partially represented in OpenSpec today. Some storage-backed capabilities already have living specs, some have narrow or placeholder coverage, and several important persistence surfaces still exist only in code and tests, which makes the app's reliability story harder to review, evolve, and protect.

## What Changes

- Add a comprehensive OpenSpec pass for every persistence surface that writes under `$XDG_DATA_HOME/lushtext`, including drafts, session data, workspaces, local history, bookmark and annotation sidecars, search history, saved searches, Replace All undo backups, and derived transparency style-scheme files.
- Introduce missing capability specs for draft and session recovery, document save and close safety, workspace state persistence, search history and saved searches, and search replace safety.
- Refresh existing living specs so they describe current shipped behavior instead of only the narrower change that originally created them.
- Record the storage layout, capability boundaries, and cross-cutting safety rules in one umbrella design so future persistence work has a clear source of truth.

## Capabilities

### New Capabilities
- `draft-session-recovery`: Autosaved drafts, close-time draft flushing, session snapshot persistence, startup restore, untitled draft recovery, and draft cleanup rules.
- `document-save-safety`: Atomic document writes, safe Save As state transitions, close and discard protections, external-change handling, and large-file-aware save behavior.
- `workspace-state-persistence`: Persisted workspace collections and active workspace state stored in `workspaces.json`, including startup restore and debounced save behavior.
- `search-history-and-saved-searches`: Persisted recent search history and user-managed saved searches stored under the app data directory.
- `search-replace-safety`: Replace All safety behavior, including persisted undo backup lifetime, rollback expectations, atomic writes, and undo restoration rules.

### Modified Capabilities
- `draft-restore-validation`: Refresh the stale file-backed draft validation contract so it fits the broader draft and session recovery model and current shipped warnings and cleanup behavior.
- `local-history`: Expand the living spec to reflect the current storage-backed lineage model under app data, restore-safety snapshots, retention behavior, and current browse and restore guarantees.
- `line-bookmarks`: Refresh the bookmark spec to capture the current sidecar identity and persistence behavior under the app data directory.
- `sidecar-annotations`: Refresh the annotation spec to capture the current sidecar identity, persistence, and export behavior under the app data directory.
- `tab-content-transparency`: Extend the transparency spec to cover the persisted derived style-scheme cache stored under the app data directory.

## Impact

- Affected code: `crates/lushtext-core/src/services`, `crates/lushtext-core/src/model`, `crates/lushtext-core/src/ui/window`, `crates/lushtext-core/src/ui/editor_page`, `crates/lushtext-core/src/ui/sidebar`, and `crates/lushtext-core/src/ui/search_panel`.
- Affected systems: `$XDG_DATA_HOME/lushtext` storage layout, startup restore flows, document save and close safety flows, sidecar migration on rename and Save As, workspace persistence, search persistence, and Replace All undo safety.
- Dependencies and APIs: no new runtime dependency is expected; the change formalizes existing JSON, text-snapshot, and generated-XML persistence patterns already used in the app.
