# workspace-entry-visibility Specification

## Purpose
Define the single workspace-wide rule for which directory entries are visible — the hidden-files view mode and the always-excluded names list — and the surfaces, controls, refresh behavior, and automation projection that honor it.

## Requirements

### Requirement: Workspace entry visibility is one app-wide rule
The application SHALL decide whether a workspace directory entry is visible using exactly one GTK-free rule, evaluated per entry basename: an entry whose basename is on the always-excluded names list is never visible; otherwise an entry whose basename begins with `.` is visible only while the hidden-files view mode is on; otherwise the entry is visible. The rule SHALL be applied identically by the workspace sidebar tree, the sidebar empty-folder detection, the command palette file index, and workspace content search.

#### Scenario: Default configuration reproduces current behavior
- **WHEN** the hidden-files view mode is off and the excluded list holds its default `.git`
- **THEN** no entry whose basename begins with `.` appears in the sidebar tree, the palette file index, or content search results, matching the behavior before this change

#### Scenario: Hidden mode reveals dotfiles that are not excluded
- **WHEN** the hidden-files view mode is on and a workspace folder contains `.env`, `.github/workflows/ci.yml`, and `.git/HEAD`
- **THEN** `.env` and `.github/workflows/ci.yml` are visible on every governed surface and `.git` and its contents are visible on none of them

#### Scenario: Excluded names win over hidden mode
- **WHEN** the hidden-files view mode is on and `.git` is on the excluded list
- **THEN** `.git` is not visible on any governed surface

#### Scenario: Excluded names also hide non-dot entries
- **WHEN** the user adds `node_modules` to the excluded list
- **THEN** every `node_modules` directory is absent from the sidebar tree, the palette index, and content search regardless of the hidden-files mode

#### Scenario: Matching is exact basename and case-sensitive
- **WHEN** the excluded list contains `.git`
- **THEN** an entry named `.gitignore`, `.Git`, or `git` is not treated as excluded by that entry

### Requirement: Configured workspace folders are always visible as roots
A configured workspace folder SHALL always be visible as a root row and SHALL always be indexed and searched as a root, regardless of its own basename or the excluded list. The visibility rule SHALL govern only entries discovered inside it.

#### Scenario: Dot-named workspace folder stays visible
- **WHEN** the user adds `~/.config` as a workspace folder while the hidden-files mode is off
- **THEN** the folder row appears in the sidebar, its non-dot children are visible, and its files are indexed and searchable

#### Scenario: Excluded-named root stays visible
- **WHEN** `node_modules` is on the excluded list and a workspace folder is itself named `node_modules`
- **THEN** that root remains visible while nested `node_modules` directories inside it are hidden

### Requirement: Visibility settings persist in GSettings
The application SHALL persist the hidden-files view mode in the boolean GSettings key `workspace-show-hidden-files` (default `false`) and the always-excluded names in the string-array GSettings key `workspace-excluded-names` (default `[".git"]`). Both keys SHALL be global to the application, not per window or per workspace.

#### Scenario: Mode survives restart
- **WHEN** the user turns the hidden-files mode on and restarts the application
- **THEN** the mode is still on and dotfiles are visible in every window

#### Scenario: Defaults are restored by reset
- **WHEN** the user activates Reset to Defaults in the excluded-names editor
- **THEN** the key holds exactly `[".git"]`

### Requirement: The hidden-files mode is a stateful app action with menu and shortcut exposure
The application SHALL expose the hidden-files view mode as a boolean-stateful action `app.show-hidden-files` whose state mirrors the GSettings key, rendered as a check item labelled "Show Hidden Files" in the main menu, and bound to the accelerator `Ctrl+Shift+H`. The action SHALL appear in the action catalog and the automation documentation. No control for the mode SHALL be added to the workspace sidebar header row.

#### Scenario: Menu item toggles and reflects state
- **WHEN** the user activates "Show Hidden Files" in the main menu
- **THEN** the check item shows the new state, the GSettings key changes, and the sidebar and palette refresh to the new visibility

#### Scenario: Shortcut toggles the mode
- **WHEN** the user presses `Ctrl+Shift+H` with an editor focused
- **THEN** the hidden-files mode toggles and the Find and Replace bar is not opened

#### Scenario: Automation can flip the mode through the action
- **WHEN** an automation client activates `app.show-hidden-files` through the catalog-checked action path
- **THEN** the state changes, the menu item and Preferences switch reflect it, and the change is observable in the read-only snapshot

#### Scenario: Sidebar header row is unchanged
- **WHEN** the sidebar is shown at the `Small` width preset
- **THEN** the header row contains only the workspace selector and the New Workspace button

### Requirement: Preferences expose both axes in one group
`Preferences > Workspace` SHALL contain a group titled "Hidden and Excluded Items" holding a switch row "Show Hidden Files" bound two-way to `workspace-show-hidden-files`, and an expander row "Always Excluded Names" that lists one row per excluded name with a remove control, an entry row for adding a name, and a Reset to Defaults control. Adding SHALL trim whitespace and SHALL reject empty names, names containing `/`, and duplicates. Every mutation SHALL write the complete array to GSettings, and the rows SHALL re-project from the key so external edits are reflected.

#### Scenario: Add a name
- **WHEN** the user types ` build ` into the add entry and presses Enter
- **THEN** a row labelled `build` appears, the key holds the previous names plus `build`, and the entry clears

#### Scenario: Reject an invalid name
- **WHEN** the user submits an empty string or `a/b`
- **THEN** no row is added, the key is unchanged, and the entry shows an error state with an accessible description explaining the rule

#### Scenario: Duplicate submission does not add a row
- **WHEN** the user submits a name that is already listed
- **THEN** no new row is added and the existing row is focused or highlighted

#### Scenario: Remove a name
- **WHEN** the user activates the remove control on the `.git` row
- **THEN** the row disappears, the key no longer contains `.git`, and the expander shows an explicit "No excluded names" state when the list becomes empty

#### Scenario: External edits are reflected
- **WHEN** the key is changed outside the dialog while Preferences is open
- **THEN** the expander rows re-project to the new array without duplicating rows

#### Scenario: Preferences switch and menu item agree
- **WHEN** the user turns the "Show Hidden Files" switch row on
- **THEN** the main menu check item shows checked without reopening the window

### Requirement: Sidebar reflects visibility changes by refreshing in place
The workspace sidebar SHALL treat a change to either key as a workspace-wide automatic refresh that reuses the existing reconciling refresh path without emitting the manual-refresh status message or announcement. The subscription SHALL be owned once at the sidebar level and released on dispose so destroyed sections never receive it. Expanded directories SHALL remain expanded when their folder rows still exist, and the `(Empty)` label and disabled expander for a folder SHALL be recomputed under the new rule.

#### Scenario: Toggling on preserves expansion
- **WHEN** a directory is expanded in the sidebar and the user turns hidden files on
- **THEN** that directory stays expanded, its dot-children appear in sorted position, and the `TreeListModel` is not remounted

#### Scenario: Dot-only folder stops being empty
- **WHEN** a folder contains only `.gitkeep` and the user turns hidden files on
- **THEN** its `(Empty)` label is removed, its expander becomes available, and Focus Folder is enabled

#### Scenario: Top-level workspace folder with only dotfiles stops being empty
- **WHEN** a configured workspace folder contains only `.gitkeep` and the user turns hidden files on
- **THEN** the top-level folder row's `(Empty)` label, which is computed by a separate probe, is removed and the row becomes expandable

#### Scenario: Refresh readiness covers the toggle
- **WHEN** either key is written and the refresh blocker is armed on that same write path
- **THEN** the `workspace-refresh-complete` readiness predicate does not report ready until the resulting refresh has published

#### Scenario: Toggle does not announce a manual refresh
- **WHEN** the user flips the hidden-files mode
- **THEN** no "Refreshing workspace folders" status message or screen-reader announcement is produced

#### Scenario: Destroyed sections are not refreshed
- **WHEN** a workspace is removed and then either key changes
- **THEN** only live sections refresh and no handler runs for the removed section

#### Scenario: A dot-name created in place stays visible until the next refresh
- **WHEN** the hidden-files mode is off and the user creates or renames a file to `.env` in the sidebar
- **THEN** its row remains visible and openable until the next refresh of that directory, after which the rule applies

### Requirement: Palette index reflects visibility changes and keeps its performance bound
The command palette file index SHALL rebuild through its existing debounced, generation-guarded path when either key changes, applying the visibility rule per entry. The internal ignored-directory performance list SHALL continue to apply in addition to the rule and SHALL NOT be exposed as a preference.

#### Scenario: Palette finds a revealed dotfile
- **WHEN** hidden files are turned on and the user searches the palette for `ci.yml`
- **THEN** `.github/workflows/ci.yml` appears among file results after the rebuild settles

#### Scenario: Excluded name removes directory from index
- **WHEN** the user adds `vendor` to the excluded list
- **THEN** no file under any `vendor` directory appears in palette file results

#### Scenario: Internal skip list is unaffected by the mode
- **WHEN** hidden files are turned on
- **THEN** files under `node_modules` and `target` are still absent from the palette index while remaining visible in the sidebar

### Requirement: Content search honors the rule with a per-search hidden override
Workspace content search SHALL exclude entries whose basename is on the excluded list and SHALL skip dotfiles unless hidden files are enabled for that search. The search panel SHALL show a "Hidden files" toggle in its options area beside the gitignore toggle, seeded from `workspace-show-hidden-files` each time the panel is opened; the toggle's value SHALL be captured in search history and saved searches and restored with them. The excluded list SHALL NOT be overridable per search.

#### Scenario: Toggle is seeded on open
- **WHEN** the global mode is on and the user opens the search panel
- **THEN** the "Hidden files" toggle is active

#### Scenario: Per-search override does not change the global key
- **WHEN** the user deactivates the "Hidden files" toggle in the panel and runs a search
- **THEN** dotfiles are not searched, and `workspace-show-hidden-files` remains `true`

#### Scenario: Excluded names are never searched
- **WHEN** `.git` is on the excluded list and the "Hidden files" toggle is active
- **THEN** no match is reported from any path under a `.git` directory

#### Scenario: Saved search restores the toggle
- **WHEN** the user saves a search with the "Hidden files" toggle active and later selects that saved search
- **THEN** the toggle is restored active before the search runs, and the row's toggle summary shows the hidden axis

#### Scenario: Pre-existing history and saved searches still load
- **WHEN** `search-history.json` or `saved-searches.json` written before this change, lacking the `hidden` field, is loaded
- **THEN** every entry loads with hidden files off and no file is quarantined or reset

#### Scenario: Overlapping-root exclusion survives excluded names
- **WHEN** two configured folders overlap and `.git` is on the excluded list
- **THEN** results from the overlapping child root are still reported once, under the first folder, and no `.git` path is searched

#### Scenario: Global key change re-seeds an idle open panel
- **WHEN** the panel is open with no search running and the global mode changes
- **THEN** the "Hidden files" toggle follows the new global value

### Requirement: Notes and bookmarks work for files inside hidden folders
Document notes, line bookmarks, and the Notes browser SHALL treat a saved file inside a hidden or previously hidden folder exactly like any other workspace file, because sidecar scoping is by canonical path prefix.

#### Scenario: Bookmark in a hidden folder is browsable and activatable
- **WHEN** the user bookmarks a line in `.github/workflows/ci.yml` with hidden files off, then opens Browse Bookmarks
- **THEN** the bookmark is listed under the workspace and activating it opens the file at that line

#### Scenario: Revealing the folder shows the bookmarked file row
- **WHEN** the user then turns hidden files on
- **THEN** the sidebar shows the `.github` tree and the file row, and the editor gutter still shows the bookmark mark

### Requirement: App-data scanners are unaffected
Scans of app-owned data directories (drafts, local history, Replace All backups, format-upgrade inventory, bookmark and folder-note sidecars, workspace quarantine, and style-scheme directories) SHALL keep a fixed visibility that hides dotfiles and excludes nothing, and SHALL NOT observe either key.

#### Scenario: Draft cleanup ignores the workspace mode
- **WHEN** hidden files are turned on
- **THEN** draft orphan cleanup, local-history listing, sidecar scans, quarantine, style-scheme discovery, and backup inspection produce the same results as before the change and pass the fixed app-data visibility to the boundary

### Requirement: Automation snapshot exposes bounded visibility state
The read-only automation workspace snapshot SHALL include the hidden-files mode as a boolean and the excluded names as an array bounded to at most 64 entries, each name bounded by the same shared snapshot text cap and truncation marker as other capped snapshot text. Both fields SHALL be documented with stable anchors in the automation reference.

#### Scenario: Snapshot reflects both axes
- **WHEN** the mode is on and the excluded list is `[".git", "vendor"]`
- **THEN** the snapshot reports the mode as true and the two names in order

#### Scenario: Oversized list is truncated, not dropped
- **WHEN** the excluded list holds more than 64 names
- **THEN** the snapshot reports the first 64 and flags truncation

### Requirement: Cross-surface parity is proven by one test
The test suite SHALL include one widget test that, over a shared fixture containing hidden, excluded, and ordinary entries, asserts that the set of visible files is identical across the sidebar tree, the palette file index, and content search for each combination of the mode being on or off.

#### Scenario: Parity holds for both modes
- **WHEN** the parity test runs with the mode off and then on
- **THEN** in each case the three surfaces report the same visible file set, and that set matches the rule applied directly to the fixture
