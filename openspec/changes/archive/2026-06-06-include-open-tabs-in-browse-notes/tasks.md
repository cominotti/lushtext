## 1. Entry Model And Scope Classification

- [x] 1.1 Extend the notes-browser entry/source model so bookmark and document-note rows can represent either workspace-owned rows or open-tab rows without fake workspace metadata.
- [x] 1.2 Add helper logic that snapshots saved open editor paths and live bookmark records on the GTK main thread.
- [x] 1.3 Add helper logic that classifies each saved open path as inside the current scope, inside another restored workspace, or outside all restored workspaces.
- [x] 1.4 Keep workspace rows strict to the current scope while collecting supplemental open-tab candidates only for saved open paths outside that scope.

## 2. Browser Loading And Deduplication

- [x] 2.1 Update `show_notes_dialog` loading so it does not return early when workspace roots are empty but eligible saved open-tab entries exist.
- [x] 2.2 Load workspace-scoped bookmarks, workspace notes, and document notes through the existing background listing flow.
- [x] 2.3 Build open-tab bookmark rows from live editor bookmark snapshots, including changes not yet flushed to bookmark sidecars.
- [x] 2.4 Load existing document notes for supplemental open-tab paths by explicit saved path instead of scanning outside workspace roots.
- [x] 2.5 Deduplicate rows by saved-document identity so an open tab already represented in current workspace results is not duplicated in `Open Tabs`.
- [x] 2.6 Preserve existing diagnostics and empty-state behavior when neither workspace rows nor open-tab rows can be shown.

## 3. Notes Browser Presentation And Actions

- [x] 3.1 Add a dedicated `Open Tabs` sidebar section for supplemental bookmark and document-note rows.
- [x] 3.2 Update row titles, subtitles, preview metadata, and bookmark preview copy to expose `Open tab`, known out-of-scope workspace names, or `Outside workspace` as appropriate.
- [x] 3.3 Update search matching so open-tab bookmark and document-note rows match source metadata, saved file metadata, bookmark line/label, and document-note body text.
- [x] 3.4 Change the unified notes-browser search placeholder to `Search Notes...`.
- [x] 3.5 Update markdown preview context so open-tab document notes render without requiring a workspace root.
- [x] 3.6 Update Open behavior so open-tab bookmarks focus the saved file at the bookmarked line and open-tab document notes focus the saved file before opening its document-note surface.

## 4. Automated Coverage

- [x] 4.1 Add widget coverage for a bookmark on a saved open tab outside every workspace root appearing in `Open Tabs`.
- [x] 4.2 Add widget coverage for an existing document note on a saved open tab outside every workspace root appearing in `Open Tabs`.
- [x] 4.3 Add widget coverage for a concrete workspace scope where workspace rows stay scoped while another saved open tab appears only in `Open Tabs`.
- [x] 4.4 Add widget coverage for opening `Browse Notes...` with no restored workspaces but eligible open-tab rows.
- [x] 4.5 Add widget coverage for the no-workspace/no-open-tab empty or warning state.
- [x] 4.6 Add search coverage for open-tab source metadata and note/bookmark content.

## 5. Validation

- [x] 5.1 Run `cargo fmt --check`.
- [x] 5.2 Run `cargo check -p lushtext-core -p lushtext`.
- [x] 5.3 Run focused notes-browser widget tests.
- [x] 5.4 Run `cargo clippy -p lushtext-core -p lushtext --all-targets -- -D warnings`.
- [x] 5.5 Run `openspec validate include-open-tabs-in-browse-notes --strict`.
- [x] 5.6 Run `git diff --check`.
