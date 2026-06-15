## 1. Contract And Upstream Reference

- [x] 1.1 Record the GNOME Text Editor source commit and relevant upstream files in implementation notes or code comments where they guide visible behavior.
- [x] 1.2 Resolve the initial policy questions: row-level remove control scope, which successful open sources update recents, and the compact Open button breakpoint.
- [x] 1.3 Identify all affected LushText docs and automation surfaces before implementation: action catalog, automation references, shortcut docs, Blueprint contract, and visual proof manifests.

## 2. Recent Document Model And Service

- [x] 2.1 Add a GTK-free recent-document value model with path identity, display title, subtitle/location text, last-opened ordering metadata, and privacy-safe serialized fields.
- [x] 2.2 Add recent-document service helpers for load, save, add/update, remove, deduplicate, missing-path pruning, unsupported URI rejection, and corrupt-file recovery through `json_store` and the filesystem boundary.
- [x] 2.3 Add search/filter helpers that cover case-insensitive prefix, substring, and fuzzy matching over title and path context.
- [x] 2.4 Add open-tab exclusion helpers so the UI can hide already-open file-backed documents while keeping stale duplicate activation safe through `open_document()`.
- [x] 2.5 Add unit tests for ordering, deduplication, persistence load/save, corrupt JSON recovery, missing-path pruning, unsupported URI handling, search scoring, and open-tab exclusion.

## 3. Open Popover Widget And Resources

- [x] 3.1 Create the `LushtextOpenPopover` module and template with search entry, compact file-chooser button, separator, stack, recent-list view, and empty state.
- [x] 3.2 Create the recent-row presentation with GNOME-style two-line content, ellipsized title/subtitle, optional age/context text, and compact remove control if retained by the policy decision.
- [x] 3.3 Implement the searchable `gio::ListStore`/selection model update path with efficient batch refreshes and no GTK main-thread filesystem work.
- [x] 3.4 Implement popover lifecycle behavior: clear search, reset scroll, focus search on open, switch empty/list states, and pop down exactly once on activation.
- [x] 3.5 Implement keyboard behavior for `Enter`, `Down`, `Up`, `Escape`, search cancellation, and row activation.
- [x] 3.6 Bound the recent-list scroller to 10 default-scale rows while preserving item-region-only scrolling for the full model.
- [x] 3.7 Add Open popover CSS/classes matching GNOME Text Editor's slim open-popover styling without introducing horizontal scrollbars.

## 4. Window Integration

- [x] 4.1 Replace the header direct Open button with a flat `GtkMenuButton` that shows `Open` plus chevron in wide mode and folder icon in constrained mode.
- [x] 4.2 Keep `win.open-file` as the direct file chooser action for `Ctrl+O`, command palette, automation, and the popover chooser button.
- [x] 4.3 Add or wire the recent Open popover action/shortcut so `Ctrl+K` opens the popover and focuses search.
- [x] 4.4 Pop down the Open popover before opening file chooser dialogs or activating recent rows.
- [x] 4.5 Update successful file-backed open paths to record recents according to the resolved policy while avoiding failed loads, unsupported URIs, and private document contents.
- [x] 4.6 Refresh recent-row visibility when tabs open, close, rename, save-as, or duplicate-detection state changes.
- [x] 4.7 Update accessibility metadata for the header Open control, popover controls, list rows, remove controls, empty state, and scrollable list region.

## 5. Documentation, Catalog, And Template Checks

- [x] 5.1 Update action catalog rows, command palette metadata, and automation docs for any new or changed Open popover actions, shortcuts, anchors, or readiness-observable state.
- [x] 5.2 Update user-facing shortcut/help docs so `Ctrl+O` and `Ctrl+K` describe the correct file chooser and recent-search behaviors.
- [x] 5.3 Regenerate Blueprint output after template changes with `make blueprint-generate`.
- [x] 5.4 Run Blueprint drift and template checks with `make check-blueprint`.
- [x] 5.5 Run automation documentation checks with `make check-automation-docs` and update stale references in the same change.

## 6. Widget And Interaction Tests

- [x] 6.1 Add widget tests for empty recents, one recent, representative rows, exactly 10 rows, 11 or more rows, awkward labels, filtered no-results, stale duplicate rows, no active editor, and constrained geometry.
- [x] 6.2 Add widget tests proving the fixed search/chooser header remains visible while only the recent-list region scrolls.
- [x] 6.3 Add keyboard tests for `Ctrl+K`, initial search focus, stale search clearing, top scroll reset, `Enter` first-match activation, `Down` from search, `Up` from first row, `Escape` dismissal, and chooser button routing.
- [x] 6.4 Add activation tests proving pointer/keyboard row activation uses the normal duplicate-safe `open_document()` workflow and closes the popover exactly once.
- [x] 6.5 Add accessibility-oriented widget assertions for stable accessible names, roles, focus order, and constrained-state reachability.
- [x] 6.6 Add file chooser action tests proving `Ctrl+O` still opens the chooser directly and the popover chooser button pops down before the chooser opens.

## 7. Visual And Smoke Coverage

- [x] 7.1 Add or extend visual geometry scenarios for the GNOME-style Open button and popover in empty, representative, dense, awkward-label, and 720p-height states.
- [x] 7.2 Ensure visual proof checks 10 visible rows, item-region-only scrolling, no horizontal scrollbar, readable empty state, preserved header controls, and no clipping with header chrome present.
- [x] 7.3 Add or extend accessibility smoke coverage for the Open popover across empty, representative, dense, and constrained states.
- [x] 7.4 Run the focused widget test target for the new Open popover coverage.
- [x] 7.5 Run `make test-widget-headless` and treat any `FLAKY:` or unexpected-warning output as a blocker.
- [x] 7.6 Run the relevant visual smoke or geometry proof lane, refreshing stale visual proof artifacts before retrying broader gates.
- [x] 7.7 Run `make pre-commit` after the focused gates pass.

## Verification

- `cargo test -p lushtext-core recent_documents --lib`
- `cargo test -p lushtext --test widget open_popover`
- `cargo test -p cargo-gtk-proof --lib`
- `make check-blueprint`
- `make check-automation-docs`
- `make test-widget-headless`
- `make visual-geometry-smoke`
- `make pre-commit`
- `openspec validate match-gnome-open-popover --strict`
- `git diff --check`
