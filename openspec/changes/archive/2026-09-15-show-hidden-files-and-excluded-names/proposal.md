## Why

LushText silently hides every dotfile and dotfolder from the workspace sidebar, the command palette file index, and workspace content search, with no way to reveal them. Users who edit `.github/workflows/*.yml`, `.env`, `.editorconfig`, or `.config/` trees cannot see, open, or search those files from the workspace surfaces at all. The filter is a hardcoded `include_hidden: false` in one filesystem-boundary policy plus an `ignore::WalkBuilder::hidden(true)` call in content search, so the fix is small in mechanism but needs a deliberate, cross-surface UX contract so that "what the workspace shows" stays one concept rather than three drifting ones.

## What Changes

- Add a global **Show Hidden Files** view mode backed by a new GSettings key `workspace-show-hidden-files` (boolean, default `false`). When on, dotfiles and dotfolders appear in the workspace sidebar tree, count toward the sidebar's `(Empty)` folder detection, are indexed by the command palette, and are searched by workspace content search by default.
- Expose the view mode as a stateful app action `app.show-hidden-files` rendered as a check item in the main (hamburger) menu, with the keyboard shortcut `Ctrl+Shift+H`, and as a switch row in `Preferences > Workspace`. No new button is added to the sidebar header row.
- Add a second, independent axis: an **Always Excluded Names** list backed by a new GSettings key `workspace-excluded-names` (string array, default `[".git"]`). Names on this list are never shown or searched on any workspace surface, regardless of the hidden-files mode. Matching is exact basename, case-sensitive; no glob interpretation in this change, but the key shape leaves room for it.
- Add an editable list UI in `Preferences > Workspace` under a new **Hidden and Excluded Items** group: an expander row with one removable entry row per excluded name, an add-name entry, and a reset-to-defaults action.
- Add a per-search **Hidden files** toggle to the workspace search panel's options revealer beside the existing gitignore toggle, seeded from the global key whenever the panel is opened. The excluded-names list is not overridable per search.
- Thread a GTK-free `WorkspaceEntryVisibility` value through the filesystem boundary's directory traversal as a borrowed parameter alongside the existing `Copy` `DirectoryScanPolicy`, so workspace scans (sidebar tree, both `(Empty)` probes, palette index, notes sources) apply the rule, while every app-data scanner (drafts, local history, Replace All backup, format-upgrade inventory, bookmark and folder-note sidecars, quarantine, style schemes) keeps its existing fixed policy.
- A configured workspace folder is always visible as a root; the rule governs entries discovered inside it.
- Add `hidden` to the serialized search options with a serde default so existing `saved-searches.json` and `search-history.json` keep loading.
- Changing either key triggers the existing full sidebar refresh (reconciling, expansion-preserving) and the existing debounced palette index rebuild.
- Confirm and test that notes and bookmarks attached to files inside hidden folders work end to end, since sidecar scoping is by canonical path prefix and never depended on directory scans.

## Capabilities

### New Capabilities
- `workspace-entry-visibility`: the single cross-surface contract for which workspace entries are visible: the hidden-files view mode, the always-excluded names list, their composition rule, the surfaces they govern (sidebar tree, `(Empty)` lookahead, palette index, content search), the app action / shortcut / menu / preferences exposure, the search-panel per-search override, refresh behaviour on change, and hidden-folder notes and bookmarks.

### Modified Capabilities
<!-- No existing requirement text changes. Existing specs describe scope, refresh, and search options, none of which state a hidden-file rule; the new capability adds the rule alongside them. -->

## Impact

- **GSettings schema** (`data/dev.cominotti.lushtext.gschema.xml`): two new keys and their `config::keys` constants.
- **Filesystem boundary** (`services/filesystem/tree.rs`): the three traversal entry points gain a borrowed `&WorkspaceEntryVisibility` parameter; `DirectoryScanPolicy` stays `Copy` and `visible_workspace()` stays `const`. All 18 `visible_workspace()` and 24 struct-literal callers are touched mechanically to pass the fixed app-data visibility.
- **Sidebar** (`ui/sidebar/policy.rs`, `workspace_section/scan_execution.rs`, `workspace_section/folder_execution.rs`, `services/file_tree.rs`): scan requests and both `(Empty)` probes (in-scan lookahead and the top-level folder-row probe) receive a visibility value built once per refresh pass on the GTK thread.
- **Command palette** (`ui/window/palette_shell.rs`, `services/palette/index.rs`): `FileIndexBuildRequest` gains a `visibility` field carried by the palette shell; the internal `IGNORED_INDEX_DIRS` performance bound is unchanged and applies in addition.
- **Content search** (`model/content_search.rs`, `services/content_search/search.rs`, `ui/search_panel/`): `ContentSearchOptions` gains `hidden` (serde-defaulted, included in `toggle_summary`); `search_with_plan` gains a borrowed visibility parameter; the single `filter_entry` closure is extended; the panel gains a toggle, an evidence field, history/saved-search capture, and accessibility metadata.
- **App actions** (`app.rs`, `services/action_catalog/`): new stateful app action registered and accelerated in `app.rs` (not `ui/window/actions.rs`), catalog entry, baseline and static-visible list updates, primary-menu item, and two bounded snapshot fields.
- **Preferences** (`resources/ui/preferences.blp`, `ui/preferences/`): new group, switch row, and the excluded-names expander with row add/remove/reset wiring.
- **Docs**: `README.md` features, `AGENTS.md` key design decision, `docs/automation.md` and `docs/automation-reference.md` for the action and any snapshot fields, `docs/accessibility.md` and `docs/accessibility-matrix.md` for new rows and the menu item, `docs/workflow-readability-matrix.md` if the sidebar or palette row file sets change.
- **Tests**: unit tests for the policy composition and boundary filtering, palette and search service tests for excluded names, widget tests for the preferences list editor, menu/shortcut toggling, sidebar refresh on change, search-panel seeding, hidden-folder notes and bookmarks, and one cross-surface parity test that asserts the sidebar, palette, and search agree on a shared fixture.
- **Persistence**: `search-history.json` and `saved-searches.json` gain an optional `hidden` field. Because both files load through the recovery envelope, where a deserialize failure quarantines and resets the file, the field MUST be serde-defaulted and covered by a backward-compatibility test. No other persisted format changes.
