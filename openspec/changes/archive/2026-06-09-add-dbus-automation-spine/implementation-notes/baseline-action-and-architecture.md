## Baseline Action And Architecture Inventory

This note records the implementation evidence for tasks 1.1 through 1.6.
It is intentionally descriptive: later tasks will turn this inventory into the
checked action catalog, automation reference, and drift gates.

## 1.1 Live `org.gtk.Actions` Baseline

Captured on 2026-06-09 with `target/debug/lushtext` in a private
`dbus-run-session`, isolated `XDG_*` directories, `GSETTINGS_BACKEND=keyfile`,
`GSETTINGS_SCHEMA_DIR=$PWD/data`, `GDK_BACKEND=wayland`, headless Mutter, and
a file argument inside the isolated fixture directory.

The smoke command introspected:

- app object path: `/dev/cominotti/lushtext`
- window object path: `/dev/cominotti/lushtext/window/1`
- interface: `org.gtk.Actions`

Live app actions:

```text
preferences
quit
about
```

Live window actions:

```text
show-notes
open-folder
close-tabs-right
close-tab
save-as
unfullscreen
search-prev-match
toggle-bookmark
notes-toggle-bookmark
discard-changes
close-other-tabs
prev-match
show-encoding-controls
toggle-fullscreen
show-line-ending-controls
show-file-health
prev-bookmark
toggle-sidebar
move-tab-right
zoom-reset
begin-replace
toggle-minimap
toggle-focus-mode
notes-open-folder-note
show-local-history
open-file
show-bookmarks
print
toggle-preview-mode
begin-search
notes-open-document-note
cycle-invisible-characters
open-folder-note
toggle-preview-pane
toggle-command-palette
toggle-properties
toggle-search-panel
zoom-in
zoom-out
save
next-bookmark
fullscreen
toggle-tab-pinned
search-next-match
notes-show-notes
new-tab
move-tab-left
next-match
edit-bookmark-label
open-document-note
```

Representative `Describe` calls:

```text
toggle-sidebar: enabled=true, parameter='', state=true
toggle-properties: enabled=true, parameter='', state=false
toggle-minimap: enabled=true, parameter='', state=false
toggle-focus-mode: enabled=true, parameter='', state=false
toggle-preview-mode: enabled=true, parameter='', state=false
begin-search: enabled=true, parameter='', no state
```

Post-task 3.1 implementation update: the action catalog and window action
registration now include `win.set-search-query` with string parameter type
`s`, no action state, active-tab enablement, and D-Bus action exposure. The
focused widget proof
`test_parameterized_search_action_updates_visible_search_workflow` verifies
that activating this action updates the visible search entry, result count,
minimap search markers, focus restoration, and close behavior through the same
workflow as visible typing.

Post-task 3.3/3.4 implementation update: the action catalog and window action
registration now include boolean target-state actions for deterministic
scenario setup:

- `win.set-sidebar-visible(b)`
- `win.set-properties-visible(b)`
- `win.set-minimap-visible(b)`
- `win.set-search-panel-visible(b)`
- `win.set-focus-mode(b)`
- `win.set-preview-pane-visible(b)`
- `win.set-preview-mode(b)`

These actions do not own state; they delegate to existing user-visible toggle
workflows and leave the existing stateful toggle actions as the source of
settled state. The focused widget proof
`test_target_state_actions_drive_visible_surfaces_without_toggle_parity`
verifies parameter type export, active-tab gating for preview/search-query
actions, shell surface updates, stateful toggle synchronization, and preview
pane/preview-only mutual exclusion. The catalog proof
`observed_action_audit_rejects_wrong_parameter_type_for_each_parameterized_action`
covers invalid-parameter drift for every new parameterized action without
emitting GLib criticals in the warning-gated widget harness.

## 4.1-4.5 Read-Only Automation Interface Update

The first app-owned automation object is registered from
`LushtextApplication::dbus_register()` under the stable child object path
`/dev/cominotti/lushtext/Automation`, using GIO's `DBusConnection` object
registration API rather than adding a D-Bus dependency. It exposes interface
`dev.cominotti.lushtext.Automation1` with:

- read-only `InterfaceVersion: u = 1`
- read-only `Enabled: b = true`
- read-only `BuildProfile: s`
- `GetActionCatalog() -> (s json)`
- `GetSnapshot() -> (s json)`
- `WaitForIdle(u timeout_msec) -> (b ok, s detail)`

The snapshot is intentionally bounded and read-only. It includes app identity,
tab metadata, active tab index, file/draft identity class, optional file path,
modified/saving/loading/failed-load/pinned/evicted state, requested/rendered
secondary surfaces, compact-surface owner, command palette/search-panel/editor
search visibility, preview state, minimap preference, status bar visibility,
workspace search counts, and editor search counts. It does not expose document
text.

`WaitForIdle` currently observes the implemented app-owned blockers that are
cheap to query on the GTK main context: session restore, close-safety,
draft autosave, preview animation, workspace search, command-palette index
debounce, Replace All preview generation, file load, save, and editor search
scans. It yields one GTK frame after the blocker clears before returning
success. Broader readiness predicates for workspace refresh, recovery restore,
and workflow events remain open in section 5.

Proofs added:

- `ui::automation::tests::introspection_xml_declares_version_properties_and_snapshot_methods`
- `ui::automation::tests::automation_object_path_is_child_of_application_object_path`
- `test_automation_snapshot_reports_bounded_live_window_state`
- manual private-session smoke on 2026-06-09 using `dbus-run-session`,
  headless Mutter, `gdbus call GetSnapshot`, `gdbus call GetActionCatalog`,
  `gdbus call WaitForIdle 5000`, and `gdbus introspect` against
  `/dev/cominotti/lushtext/Automation`; the snapshot returned bounded JSON,
  `WaitForIdle` returned `(true, 'idle')`, and the catalog contained
  `win.set-search-query`.

Observed diagnostic noise: the private session still activated
`org.freedesktop.portal.Desktop`, `org.freedesktop.portal.Documents`,
`org.a11y.Bus`, and portal implementation helpers even with
`GTK_USE_PORTAL=0` and `GDK_DEBUG=no-portals`. Future smoke warning filters
should treat host portal/a11y activation logs separately from the action
baseline itself.

## 4.6 Extended Bounded Snapshot Update

The active-window snapshot now includes state summaries for the remaining major
automation surfaces:

- `workspace`: current scope kind/id/name, workspace/folder counts, scoped
  folder count, empty-workspace state, persistence in-flight/dirty flags, and
  workspace-filter animation state.
- `command_palette`: visibility, query text, search mode, rendered row count,
  indexed file count, open-tab source count, and queued index updates.
- `notes`: notes-menu visibility, file-backed active-document state, live
  bookmark count, active-line bookmark presence, and document/folder note
  availability.
- `local_history`: active-document file-backed state, policy availability, and
  whether browsing or automatic capture can run.
- `content_search`: workspace search options, query/count/cap state, Replace
  All preview/undo summaries, history/saved-search counts, and flat navigation
  counters.
- `notifications`: current status-bar text/severity, notification generation,
  and delayed workspace-search progress visibility.

All values are collected from already-mounted UI state on the GTK main context.
The adapter does not scan workspaces, read sidecars, open local-history files,
or expose document text, note bodies, bookmark labels, command-palette result
bodies, or search result bodies. Free-form snapshot text is capped at 4 KiB so
large pasted queries or status messages cannot make `GetSnapshot` unbounded.
The content-search backing data is bounded at the match model too: each
`SearchMatch` retains at most 4 KiB of source-line excerpt around the match.
Matches from longer source lines stay visible as search results but are marked
truncated and skipped by Replace All preview generation, avoiding unbounded RAM
retention without applying replacements from partial-line data.

Proofs added or extended:

- `test_automation_snapshot_reports_bounded_live_window_state` now asserts the
  default bounded state for workspace, command palette, notes, local history,
  content search, and notifications, covers command-palette debounce and
  Replace All preview readiness blockers, and retains the document-text
  redaction check.
- `ui::automation::tests::bounded_snapshot_text_caps_free_form_fields_without_splitting_utf8`
  covers the shared 4 KiB free-form text cap.
- `model::content_search::tests::search_match_bounds_long_lines_around_match_and_skips_replace_preview`
  and
  `services::content_search::search::tests::literal_search_bounds_long_matching_lines`
  cover bounded search-result line retention.
- `docs/automation-reference.md` documents every new snapshot field and
  `docs/automation.md` explains the expanded exposure/privacy boundary.

Proof run:

```text
cargo test -p lushtext-core automation
cargo test -p lushtext-core content_search
./scripts/run-widget-tests.sh --headless -- test_automation_snapshot_reports_bounded_live_window_state
make check-automation-docs
cargo fmt --all -- --check
```

## 9.1-9.10 Documentation And Drift Gate Update

The first user/developer documentation pass added:

- `docs/automation.md`: supported use cases, `gdbus` examples, action-driving
  guidance, safety rules, full-filesystem/portal caveats, troubleshooting, and
  maintenance rules.
- `docs/automation-reference.md`: stable D-Bus object/interface contract,
  property and method table, snapshot schema, readiness blockers, exposure
  vocabulary, complete action catalog, and scenario-helper flag marker.
- `scripts/check-automation-docs.py`: source-driven drift check that parses
  `services::action_catalog`, the automation introspection XML, automation
  snapshot model fields, and readiness blocker literals, then requires matching
  anchors in the developer reference and baseline terms in the user guide.

The Makefile target `make check-automation-docs` runs the checker with
`--self-test`. The self-test mutates representative in-memory copies of the
reference to prove missing action, D-Bus member, snapshot field, readiness
blocker, and helper-flag documentation fails the gate. `check-policy`,
`pre-commit`, and `check` include the target because it is deterministic and
does not require a display, D-Bus session, or app launch.

Proof run:

```text
make check-automation-docs
Checking automation documentation...
./scripts/check-automation-docs.py --self-test
automation docs are current; self-test caught representative drift
```

## 1.2 Action Registration Inventory

App-level actions are registered in `crates/lushtext-core/src/app.rs`:

- `app.quit`
- `app.preferences`
- `app.about`

Window construction in `crates/lushtext-core/src/ui/window/mod.rs` installs the
major action groups through workflow modules:

- `actions.rs`: core file, editor search, workspace search, bookmark, notes,
  sidebar/properties/minimap, shortcut, and fullscreen action wiring.
- `preview.rs`: stateful `win.toggle-preview-pane` and
  `win.toggle-preview-mode`.
- `focus_mode.rs`: stateful `win.toggle-focus-mode`.
- `print.rs`: `win.print`.
- `zoom.rs`: `win.zoom-in`, `win.zoom-out`, `win.zoom-reset`, plus the menu
  zoom widget.
- `tabs.rs`: tab context actions `win.toggle-tab-pinned`,
  `win.close-tabs-right`, `win.close-other-tabs`, `win.move-tab-left`, and
  `win.move-tab-right`.
- `search.rs`: workspace search panel callbacks and navigation enablement for
  `win.search-next-match` and `win.search-prev-match`.
- `notes.rs`: notes/bookmark menu enablement and dynamic labels for
  `win.notes-*` menu-scoped actions.
- `documents.rs`: active-tab enablement for file/edit/preview/print actions.
- `local_history.rs`: enablement policy for `win.show-local-history`.
- `imp.rs`: command-palette activation dispatch strips `win.`/`app.` prefixes
  and activates the corresponding action group.

Additional scoped action groups:

- `crates/lushtext-core/src/ui/search_bar/imp.rs` registers the
  `search-options` group: `regex`, `case-sensitive`, `whole-word`.
- `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` registers
  file/folder context actions under `section`: `focus-folder`,
  `local-history`, `document-note`, `folder-note`, `move-folder-up`,
  `move-folder-down`, `remove-folder`, `new-file`, `new-dir`, `rename`,
  `delete`.
- The same sidebar module registers workspace-header actions under
  `ws-header`: `open-folder-note`, `add-folder`, `rename`, `unlist`.

The future catalog therefore needs scopes beyond `app` and `win`; it must also
represent widget-local action groups used by visible context menus.

## 1.3 Command Palette Mapping

The command palette registry lives in
`crates/lushtext-core/src/services/palette/commands.rs`. The GTK adapter in
`crates/lushtext-core/src/ui/window/imp.rs` activates `win.*` actions on the
window and `app.*` actions on the application.

Current command registry mapping:

| Command ID | Label | Registered Today |
| --- | --- | --- |
| `win.new-tab` | New File | yes |
| `win.open-file` | Open File | yes |
| `win.open-folder` | Open Folder | yes |
| `win.save` | Save | yes |
| `win.save-as` | Save As | yes |
| `win.show-local-history` | Local History | yes |
| `win.print` | Print | yes |
| `win.begin-search` | Find and Replace | yes |
| `win.toggle-bookmark` | Toggle Bookmark | yes |
| `win.edit-bookmark-label` | Edit Bookmark | yes |
| `win.next-bookmark` | Next Bookmark | yes |
| `win.prev-bookmark` | Previous Bookmark | yes |
| `win.open-document-note` | Open Document Note | yes |
| `win.open-folder-note` | Open Folder Note | yes |
| `win.close-tab` | Close Tab | yes |
| `win.show-bookmarks` | Browse Bookmarks | yes |
| `win.show-notes` | Browse Notes | yes |
| `win.toggle-sidebar` | Toggle Sidebar | yes |
| `win.toggle-properties` | Document Properties | yes |
| `win.toggle-fullscreen` | Fullscreen | yes |
| `win.toggle-focus-mode` | Focus Mode | yes |
| `win.zoom-in` | Zoom In | yes |
| `win.zoom-out` | Zoom Out | yes |
| `win.zoom-reset` | Reset Zoom | yes |
| `win.show-help-overlay` | Keyboard Shortcuts | no live/static registration found |
| `app.preferences` | Preferences | yes |
| `app.about` | About LushText | yes |
| `app.quit` | Quit | yes |

Current catalog gap: `win.show-help-overlay` is referenced by the command
palette and primary menu, but the live window action list does not export it
and no registration point appeared in the static inventory. The later action
catalog audit should fail on this until the action is either registered,
removed from visible surfaces, or documented as an unsupported placeholder with
a follow-up.

## 1.4 Visible Command Surface Inventory

Template and runtime-visible surfaces that should feed the action catalog or a
documented non-action exception:

- Header bar:
  - `win.new-tab` button
  - `win.open-file` button
  - `win.toggle-properties` button
  - `win.toggle-focus-mode` Leave button while Focus Mode affordance is visible
  - Main menu and Notes menu popovers
- Primary menu:
  - custom theme selector: GSettings-backed UI, no action today
  - custom zoom row: direct GSettings buttons plus equivalent `win.zoom-*`
    actions
  - `win.new-tab`
  - `win.save`
  - `win.save-as`
  - `win.show-local-history`
  - `win.discard-changes`
  - `win.print`
  - `win.begin-search`
  - `win.toggle-minimap`
  - `win.toggle-preview-mode`
  - `win.fullscreen`
  - `win.unfullscreen`
  - `app.preferences`
  - `win.show-help-overlay` (currently unresolved)
  - `app.about`
- Notes menu:
  - `win.notes-show-notes`
  - `win.notes-toggle-bookmark`
  - `win.notes-open-document-note`
  - `win.notes-open-folder-note`
- Status bar:
  - `win.toggle-sidebar`
  - `win.show-line-ending-controls`
  - `win.show-encoding-controls`
- Document properties:
  - `win.show-file-health`
- Editor context menu:
  - `win.toggle-bookmark`
  - `win.edit-bookmark-label`
  - `win.open-document-note`
  - `win.show-local-history`
- Search-bar options popover:
  - `search-options.regex`
  - `search-options.case-sensitive`
  - `search-options.whole-word`
- Tab context menu:
  - `win.toggle-tab-pinned`
  - `win.close-tabs-right`
  - `win.close-other-tabs`
  - `win.move-tab-left`
  - `win.move-tab-right`
- Sidebar file context menu:
  - `section.focus-folder`
  - `section.local-history`
  - `section.document-note`
  - `section.new-file`
  - `section.new-dir`
  - `section.rename`
  - `section.delete`
- Sidebar folder context menu:
  - `section.folder-note`
  - `section.move-folder-up`
  - `section.move-folder-down`
  - `section.remove-folder`
  - `section.new-file`
  - `section.new-dir`
- Workspace header context menu:
  - `ws-header.add-folder`
  - `ws-header.open-folder-note`
  - `ws-header.rename`
  - `ws-header.unlist`
- Shortcuts in `actions.rs` cover file, editor search, workspace search,
  bookmark navigation, notes browser, properties, focus/fullscreen, preview,
  zoom, minimap, command palette, and tab close.
- Command palette covers the static `CommandDef` list above, plus file and open
  tab results that are not action commands.

Important classification points for the future catalog:

- `win.toggle-preview-pane` is exported live but has no obvious visible menu,
  shortcut, or palette entry today; classify whether it is supported public
  automation, internal state, or a visible-control gap.
- Theme controls and zoom menu buttons are visible UI that mutate settings
  directly. Zoom has equivalent actions; theme does not currently have
  action-backed parity.
- Search options are visible commands but scoped below the search-bar widget,
  not exported on app/window `org.gtk.Actions`.

## 1.5 D-Bus Dependency Decision

Decision for the first implementation slice: use GLib/GIO D-Bus registration
for the app-owned read-only automation interface and do not add `zbus` yet.

Rationale:

- LushText already owns the session bus name through `GtkApplication`/
  `GApplication`, and GTK already exports app/window actions via GIO.
- A GIO-first adapter keeps all D-Bus callbacks on the GLib main-context model
  the app already uses, avoiding a second async runtime or thread-affinity
  bridge.
- The architecture stays within the existing dependency direction:
  `ui/app adapter -> services -> model`. Snapshot gathering can live at the
  application/window adapter boundary while services and models remain GTK-free.
- The initial interface is read-only and low volume. Stronger typed proxy
  generation is less valuable than minimizing dependency and packaging churn.

Revisit `zbus` only if a later slice proves that typed interface generation,
proxy tests, or signal ergonomics materially outweigh the extra dependency and
main-context integration cost. If that happens, isolate it in the automation
adapter and regenerate Flatpak cargo sources in the same change.

## 1.6 Dependency And Packaging Impact

No dependency was added for this baseline architecture decision. Therefore:

- no Cargo manifest updates are required;
- no Flatpak cargo source regeneration is required;
- no dependency-policy validation is required for this slice.

The Flatpak full filesystem permission posture remains unchanged by this
baseline work.
