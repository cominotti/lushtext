## Why

Long-line minimap markers currently use a red warning treatment for every line over the minimap threshold, which makes ordinary prose-heavy documents look error-filled even when nothing is wrong. Users need control over this noisy signal, with the calmer default being off.

## What Changes

- Add a persistent preference for long-line minimap markers, defaulting to disabled.
- Keep bookmark, active search-match, and modified-since-save minimap markers unchanged.
- Show long-line markers only when both the minimap is enabled and the new long-line marker preference is enabled.
- Reorganize Preferences so minimap controls live together in an `Editor` page `Minimap` group instead of leaving `Show Minimap` inside the broader `Behavior` group.
- Do not add a new top-level `Minimap` preferences page for this small control set.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-minimap`: long-line warning markers become opt-in, and the minimap preference surface gains a dedicated `Editor > Minimap` group.

## Impact

- GSettings schema and key constants for the new long-line marker preference.
- Preferences UI template and binding code for the new switch row and the `Editor > Minimap` grouping.
- Editor minimap marker collection so long-line markers are skipped unless the preference is enabled.
- Tests covering default-off behavior, enabled behavior, persistence binding, and the preference grouping.
