## Context

Today three user-facing surfaces list workspace entries, and all three hide dotfiles unconditionally:

```
                    DirectoryScanPolicy { include_hidden: false }   (services/filesystem/types.rs)
                                        │
        ┌───────────────────────────────┼───────────────────────────────┐
        ▼                               ▼                               ▼
 Sidebar tree scan             Sidebar "(Empty)" lookahead        Palette FileIndex
 services/file_tree.rs         file_tree::is_dir_empty            services/palette/index.rs
 scan_directory_bounded…       (max_entries: 1)                   (calls the same file_tree scan)

                                              separate mechanism ──▶ Content search
                                                                     services/content_search/search.rs
                                                                     ignore::WalkBuilder::hidden(true)
```

The sidebar and palette are already structurally coupled: `FileIndex::rebuild` calls `file_tree::scan_directory_bounded_with_cancel_and_bytes` per directory, so they cannot disagree about what "hidden" means. Content search uses ripgrep's `ignore` crate and has its own hidden flag, plus an existing user-facing per-search `.gitignore` toggle in the panel's options revealer that is the precedent for a per-search override.

Other `include_hidden: false` sites (drafts, local history, Replace All backup, format upgrade inventory) scan app-owned data directories, not workspaces, and are out of scope.

Constraints from the repository:

- Services stay GTK-free; GSettings is read only in `ui/` and crosses to workers as plain values.
- The workflow readability convention: "is this entry shown" is pure policy and belongs in the owning workflow's `policy.rs`; coordination modules pass the decision through, never re-derive it.
- New actions, menu items, preference rows, and search-panel toggles carry accessibility metadata and must be reflected in the automation and accessibility docs.
- `Ctrl+H` is already `win.begin-replace`; `Ctrl+Shift+H` and `Alt+H` are unbound.

## Goals / Non-Goals

**Goals:**

- One workspace-wide notion of entry visibility that the sidebar, the `(Empty)` lookahead, the palette index, and content search all obey.
- A fast, discoverable way to flip hidden-file visibility (main menu check item plus shortcut) and a durable home for it in Preferences.
- An independent, user-editable always-excluded list so `.git` stays out of the way even with hidden files shown, and so users can add their own names.
- A per-search override for hidden files in the search panel, matching ripgrep's `--hidden` mental model, without a per-search override for the excluded list.
- Notes and bookmarks on files inside hidden folders work with no special casing.
- Cheap enforcement: O(1) per entry, no extra directory I/O.

**Non-Goals:**

- Glob or path-pattern exclusions (`**/*.log`, `build/**`). The key shape allows adding this later.
- Changing the palette's internal `IGNORED_INDEX_DIRS` performance bound or exposing it as a preference.
- Hiding `node_modules`, `target`, or similar from the sidebar by default. They are visible today and users open files inside them.
- A sidebar header button. The header row already holds the workspace selector and New Workspace on a 54 px strip, and the `Small` 20 % preset leaves no comfortable room for a third control.
- Per-window or per-workspace visibility state. GSettings makes the mode global, matching Nautilus.
- Any change to app-data scanners or to `.gitignore` handling.

## Decisions

### D1: Two GSettings keys, two axes

`workspace-show-hidden-files` (`b`, default `false`) and `workspace-excluded-names` (`as`, default `[".git"]`).

Composition, evaluated per directory entry basename:

```
name ∈ excluded-names ──▶ hidden on every surface
otherwise, name starts with '.' ──▶ shown iff show-hidden is on
otherwise ──▶ shown
```

*Why two keys rather than one list with a "dotfiles" pseudo-entry:* the two axes have different lifecycles. The hidden toggle is flipped often and wants a shortcut; the excluded list is edited rarely and wants a durable editor. Conflating them would make the shortcut mutate a list.

*Alternative rejected:* sidebar-local toggle state without persistence. It would let the sidebar and palette disagree and would reset every launch.

### D2: Exact basename matching, case-sensitive, no globs

The excluded set is a `HashSet<OsString>` compared against `DirEntry::file_name()`. This is O(1) per entry, allocation-free on the hot path, and correct for non-UTF-8 names. Names containing `/` are rejected at edit time. If glob support is added later, entries containing glob metacharacters can be interpreted as patterns without a schema change, since a literal `*.log` matches nothing today.

*Alternative rejected:* `globset` from the start. It costs a compiled matcher per scan request and per-entry pattern evaluation on directories the sidebar already bounds at 10,000 rows, and it invites `.gitignore`-style semantics questions this change does not need to answer.


The one nuance in D2 is that GSettings `as` holds UTF-8 only, so a non-UTF-8 entry name can never match an excluded name; the benefit of comparing `OsStr` is avoiding a lossy `String` allocation per entry, and `admits` is applied once at the boundary against the raw `OsStr`, never again downstream where `DirectoryEntryInfo.file_name` is already a `String`.

### D3: One borrowed value object crosses the UI/service seam; `DirectoryScanPolicy` stays `Copy`

A GTK-free `WorkspaceEntryVisibility { show_hidden: bool, excluded_names: Arc<HashSet<OsString>> }` lives in `model/workspace_visibility.rs` with a pure `fn admits(&self, name: &OsStr) -> bool` implementing D1, a `Default` reproducing today's behaviour, a constructor that normalises names, and a fixed `app_data()` value (`show_hidden: false`, empty set) for non-workspace scans.

`DirectoryScanPolicy` is `Copy` with a `const fn visible_workspace()`, is passed by value into every boundary entry point, and is reused twice inside one traversal. It therefore cannot hold an `Arc`. **As implemented:** `services/filesystem/tree.rs` gains `visit_directory_with_visibility(path, policy, &WorkspaceEntryVisibility, visit)`, and every existing entry point becomes a thin wrapper that maps the policy's `Copy` `include_hidden` flag onto one of the two fixed rules (`app_data()` or `everything()`) through a private `fixed_visibility(policy)`. The dot rule is still implemented exactly once, in `admits`; `include_hidden` survives only as the `Copy`-friendly selector for app-data scans, so the 42 app-data call sites compile unchanged and none of them can observe the new keys, because `services/` cannot reach GSettings at all. Workspace scans (`file_tree::scan_directory_bounded*`, `is_dir_empty`) take the visibility explicitly.

The value is built **once per refresh pass or rebuild**, not once per directory: the sidebar builds it in `ui/sidebar/policy.rs` from the two key values and carries it in the pending child-scan request, so `scan_execution.rs` (which today constructs a fresh `gio::Settings` per `populate_child_store`) reads it instead of re-reading settings; the same value reaches both `(Empty)` probes, the in-scan lookahead and the top-level folder-row probe in `folder_execution.rs`. The palette carries it as a new `visibility` field on `services/palette/index.rs::FileIndexBuildRequest`, built in `ui/window/palette_shell.rs` where the request already forms; the shell is a called presentation surface of the palette workflow and only carries the value, it decides nothing. `IGNORED_INDEX_DIRS` remains an additional, unconditional directory predicate applied after `admits`.

`model/` placement is justified by consumer count: the sidebar, palette, and search workflows all call `admits`, and the convention forbids any of the three forking it into a local `policy.rs`. The module joins the cross-cutting mechanism table in the readability matrix with its owning-workflow count and gains mutation coverage from day one (a gain from zero, not parity).

*Alternative rejected:* an `Option<&'a HashSet<OsString>>` lifetime on `DirectoryScanPolicy`. It infects every signature that stores the policy for the same effect as a separate parameter.

### D4: Workspace roots are always visible; the rule governs their contents

A configured workspace folder is visible as a root regardless of its own name. This is already how two of the three surfaces behave (the sidebar's top-level rows come from `WorkspacesFile`, and `ignore` exempts depth-0 roots from `hidden` and from `filter_entry`), so the decision makes the third surface match rather than forcing all three to hide a deliberately added `~/.config` or `node_modules` root.

### D5: Content search receives `hidden` in its options and the visibility by parameter

`ContentSearchOptions` gains `hidden: bool` with `#[serde(default)]`. The service entry points take one `ContentSearchRequest { query, plan, options, visibility }` seam value object rather than four loose parameters, per the convention's rule that `#[expect(clippy::too_many_arguments)]` on a workflow boundary marks an unreified seam; the walker's single per-entry exclusion decision is the named `walker_admits_entry` function. This is mandatory, not optional: the options are `#[serde(flatten)]`-ed into `SearchHistoryEntry` and `SavedSearch`, both files load through the recovery envelope where a deserialize failure quarantines and resets the file, so a non-defaulted field would wipe every user's permanent saved searches on first launch. The new axis is also rendered by `toggle_summary()` so history and saved-search rows show it. The excluded set does **not** enter the options, which are compared by whole-value equality for history dedup and serialised to disk; instead `search_with_plan` gains a `visibility: &WorkspaceEntryVisibility` parameter.

In the walker, `builder.hidden(!options.hidden)` and **one** `filter_entry` closure that combines the existing overlapping-root exclusion with basename rejection. This is a correctness constraint: `WalkBuilder::filter_entry` *replaces* any previously installed filter, so two calls would silently disable the overlapping-root exclusion. With the default `[".git"]` the merged closure becomes unconditional on every search.

### D6: A stateful app action registered and accelerated in `app.rs`

`app.show-hidden-files` is a boolean-stateful action whose state mirrors the GSettings key, created with `gio::Settings::create_action` (new to the repo but compatible with the catalog contract) or the existing `SimpleAction::new_stateful` pattern with a two-way key sync. The accelerator `<Control><Shift>h` is registered with `set_accels_for_action` in `app.rs` beside `app.quit`, **not** in `ui/window/actions.rs::setup_shortcuts`: any diff touching that file triggers the six-case workspace-sidebar animation visual-proof matrix, and the repo has two accelerator mechanisms, so the choice has a gate consequence. The action is an app action because the setting is global and every window's menu item must reflect one state.

Catalog integration is three edits, all machine-checked: the `ACTION_CATALOG` entry (`ActionScope::App`, no parameter, `Bool` state, anchor `action-app-show-hidden-files`, at least one coverage lane), the `BASELINE_APP_ACTIONS` list, and `STATIC_VISIBLE_ACTION_IDS` for the primary-menu surface, plus one reference-doc row modelled on `action-win-toggle-sidebar`. It is also listed in `resources/ui/shortcuts.blp` for discoverability, which is convention rather than a gate.

**Shared toggle-action helper.** `app.show-hidden-files` and the window's `toggle-minimap` are wired by one `register_boolean_setting_toggle_action` in `app.rs`; the window layers its minimap announcement on top instead of keeping a second copy of the action/key sync.

### D7: Preferences editor is an `AdwExpanderRow` with one row per name

In `Preferences > Workspace`, a new `AdwPreferencesGroup` titled **Hidden and Excluded Items** holds an `AdwSwitchRow` **Show Hidden Files** bound two-way to the key, and an `AdwExpanderRow` **Always Excluded Names** whose children are the template's `AdwEntryRow` "Add a name" first (apply/Enter appends the trimmed name), then one `AdwActionRow` per current name with a trailing flat remove button, and a **Reset to Defaults** suffix button. **As implemented** the add row stays first and is never removed: `AdwExpanderRow::remove` rejects rows added through the template's `[row]` child type, so the projection recycles only the rows it appends itself. The empty state is an explicit "No excluded names" row.

This is a deliberate, narrow divergence from the ui rule that prefers flat `AdwPreferencesGroup` + `AdwActionRow` layouts in Preferences: the list is rarely edited, usually one entry long, and collapsing it keeps the Workspace page scannable; the expander's children *are* the grouped `AdwActionRow`s the rule asks for, built with the dynamic-row pattern already used by `ui/preferences/data_page.rs` and passing each through `accessibility::apply_row_accessibility`. `AdwExpanderRow` and `AdwEntryRow` are new to the repo, so both are `ensure_type()`-registered in `class_init` before `bind_template()` and covered by widget-harness template validation, since `gtk4-builder-tool validate` does not load Libadwaita-only types.

Validation on add: trim; reject empty, names containing `/`, and duplicates (focus the existing row), with an accessible error description. Reset calls `gio::Settings::reset` so the schema default stays the single source of truth. Every mutation writes the complete array; rows re-project from the key's `changed` signal so external edits stay consistent without duplicating rows.

*Alternative rejected:* one comma-separated `AdwEntryRow`. Cheaper, but the normalisation rules become subtitle prose. *Alternative kept in reserve:* a second flat group with the same dynamic rows and add/reset in the group header, if the expander proves awkward in the visual smoke.

### D8: Key changes trigger a silent automatic refresh through one sidebar-level subscription

The sidebar subscribes **once**, at the `LushtextSidebar` level, to `changed::workspace-show-hidden-files` and `changed::workspace-excluded-names`, owns both handlers in a `gtk_lush_signals::SignalBag` cleared on dispose, and fans out to live sections. Per-section subscriptions are rejected because sections are created and destroyed on workspace mutations and there are currently no settings subscriptions anywhere under `ui/sidebar/`.

Each section runs the **automatic** full-refresh path (`schedule_refresh(true, Vec::new(), false)`), not the manual one: the manual path emits a "Refreshing workspace folders" status message and an announcement, which a view toggle must not produce. The automatic path still reconciles child stores with bounded `splice()`, keeps the `TreeListModel` mounted when the folder-row set is unchanged, and re-evaluates both `(Empty)` probes. The palette routes both keys to its existing debounced rebuild. The `workspace-refresh-complete` readiness predicate already covers both the sidebar refresh and the palette index blocker.

A dot-name created or renamed in place while the mode is off (New File → `.env`, inline rename, DnD) keeps its row until the next refresh, because those flows update the `FileTreeItem` in place; the next scan applies the rule. This is specified as intended so a user never loses sight of a file they just created.

**Pre-existing defect fixed while proving D8.** `GtkTreeListModel` emits `notify::expanded = false` on a row it is destroying during a splice while that row still reports a stale valid `position()` (observed `0`, not `INVALID_LIST_POSITION`). The expansion recorder treated it as a user collapse and pruned the intent the deferred restore was about to re-apply, so any refresh that spliced the segment containing an expanded folder collapsed it. The recorder now trusts a row only if the model still returns it at that position (via `try_borrow`, falling back to the old behaviour while a replacement model is being installed).

### D9: The search panel seeds its Hidden files toggle from the key when it maps

The gitignore, case, regex, and whole-word toggles are each two-way bound to their own key. The new toggle deliberately is not: the global key is the default and the toggle is a per-search override, so there is no `search-hidden` key. **As implemented** the panel owns the whole sync: `LushtextSearchPanel::seed_hidden_toggle()` runs from the panel's own `map` signal (which fires at both window reveal sites, startup restore and the toggle action, without the window having to remember) and again when the global key changes while the panel is mapped and idle. The toggle is captured in history and saved searches like its siblings, restored under the `restoring_history` guard, reported through `evidence.rs`, and given accessibility metadata. Its wiring lands in `imp.rs`, `execution.rs`, and `evidence.rs`; the search-panel facade `mod.rs` sits at 369 of its 370-line budget and gains zero lines.

### D10: Bounded automation snapshot fields

The workspace snapshot gains `show_hidden_files: bool` and `excluded_names: Vec<String>`, bounded to at most 64 entries, each name passed through the existing `bounded_snapshot_text` cap rather than a second truncation helper. Names are user-configured metadata the UI already surfaces, so they are admissible under the snapshot privacy rule; the bound satisfies the free-form-text cap. Both fields get `snapshot-field-*` anchors, projection-map rows if they project from the sidebar evidence surface, and the "all ten of its fields" prose in the reference is corrected.

### D11: Hidden-folder notes and bookmarks need no code change, only proof

`note_storage.rs` scopes sidecars to a workspace by canonical-path prefix. Bookmarks and document notes for a file under `.github/` already belong to the workspace and open from the Notes browser and palette. The change adds tests: bookmark a line inside a hidden folder with the mode off, browse and activate it, turn the mode on, and assert the sidebar row and gutter mark.

## Risks / Trade-offs

- [Users add `node_modules` expecting search speedups, then wonder why files are missing] → The expander lists every name in one place; Reset to Defaults is one click; the names appear in the automation snapshot.
- [Hidden-on in a home-directory workspace reveals thousands of dotfiles] → Sidebar 10,000-row cap, 256-row batching, and `ViewportSliceBin` virtualisation bound rendering; palette file/directory/byte caps bound indexing. A hidden-on fixture joins the performance smoke.
- [User clears the list and `.git` floods the tree] → Explicit empty state plus Reset to Defaults.
- [Non-defaulted `hidden` wipes saved searches] → D5 mandates `#[serde(default)]` and a fixture round-trip test over a file lacking the field.
- [A second `filter_entry` replaces the first] → D5 mandates one merged closure and a test that overlapping-root exclusion still holds with excluded names present.
- [Mid-rebuild key change yields a mixed tree] → The value is built once per pass and captured by the worker; a later change starts a new generation and existing checks discard the stale result.
- [Widget tests leak the new keys across the shared memory backend] → Every new test resets both keys, following the existing `settings.reset` precedent in the preferences tests; a readiness assertion arms its blocker on the same path that writes the key rather than racing the `changed` dispatch.
- [The `*_for_test` ceiling is at 165/165] → New observable facts land on existing evidence surfaces; if the ceiling must rise, it rises in this change with a stated reason.
- [`Ctrl+Shift+H` collides later] → Verified unbound; the catalog docs gate flags later collisions.
- [Case-sensitive matching surprises on case-insensitive filesystems] → Documented in the subtitle; it is what the boundary sees.

## Migration Plan

No data migration. Both keys default to today's behaviour. `search-history.json` and `saved-searches.json` written before the change load with `hidden = false`. Rollback is removing the keys and the defaulted field; nothing else persists.

## Open Questions

- Whether to surface the excluded list as a read-only hint in the search panel ("Excluding: .git"). Deferred; the panel status line could carry it later.
