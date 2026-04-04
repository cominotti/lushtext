---
title: 'Hamburger menu restructure with theme selector and fullscreen'
type: 'feature'
created: '2026-04-04'
status: 'done'
baseline_commit: '061b5a1'
context:
  - '.claude/rules/ui.md'
  - '.claude/rules/widget-wiring.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** LushText's hamburger menu doesn't match GNOME Text Editor's layout. It's missing a theme selector (light/system/dark), fullscreen toggle, and proper section grouping. "Open File" and "Open Folder" entries are redundant with header bar buttons.

**Approach:** Restructure `primary_menu` to match GNOME Text Editor's 7-section layout (minus New Window, plus New File). Add a custom theme selector widget (3-state toggle: light/system/dark) via `PopoverMenu::add_child`. Add fullscreen/leave fullscreen menu items with `hidden-when: action-disabled` switching. Remove Open File and Open Folder from the menu. Add `color-scheme` GSettings key to persist theme preference.

## Boundaries & Constraints

**Always:** Match GNOME Text Editor section ordering (theme → new file → save/save-as → find/replace → fullscreen → prefs/shortcuts/about). Use `libadwaita::StyleManager::set_color_scheme()` for theme switching. Existing GtkSourceView dark variant auto-switching must continue working. F11 for fullscreen. Persist theme choice via GSettings.

**Ask First:** If the theme selector visual design needs to deviate from GNOME Text Editor's pattern (e.g., no suitable Adwaita widget available in libadwaita 0.9). If any existing keyboard shortcuts conflict.

**Never:** Implement zoom controls, print, or discard changes (deferred). Add "New Window". Break existing `style-scheme` GtkSourceView handling. Remove Open File/Open Folder header bar buttons (only removing from hamburger menu).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Theme: Light | Click Light toggle | `ColorScheme::ForceLight`; GSettings `color-scheme` = `'force-light'`; GtkSourceView uses base scheme | N/A |
| Theme: System | Click System toggle | `ColorScheme::Default`; follows desktop; GtkSourceView adapts | N/A |
| Theme: Dark | Click Dark toggle | `ColorScheme::ForceDark`; GSettings `color-scheme` = `'force-dark'`; GtkSourceView uses dark variant | N/A |
| Theme: Startup | App launches | `StyleManager::set_color_scheme()` called with persisted value before window present | Invalid key falls back to System |
| Fullscreen: Enter | Menu click or F11 | `window.fullscreen()`; "Fullscreen" item hidden, "Leave Fullscreen" shown | N/A |
| Fullscreen: Leave | Menu click or F11 | `window.unfullscreen()`; items swap back | N/A |
| No tabs open | Empty state | Save, Save As, Find/Replace disabled; New File, Fullscreen, Theme, Prefs enabled | N/A |

</frozen-after-approval>

## Code Map

- `resources/ui/window.ui` -- primary_menu restructure: new section layout, custom widget slots, fullscreen items, remove Open File/Open Folder
- `crates/lushtext-core/src/ui/window/mod.rs` -- fullscreen actions, theme widget creation + `PopoverMenu::add_child`, setup_shortcuts F11
- `crates/lushtext-core/src/ui/window/imp.rs` -- `primary_menu_button` template child access for popover; fullscreen state tracking
- `data/dev.cominotti.lushtext.gschema.xml` -- new `color-scheme` key (string, default `'default'`)
- `resources/ui/shortcuts.ui` -- F11 fullscreen entry
- `crates/lushtext-core/src/services/palette.rs` -- add Fullscreen command to `all_commands()`
- `crates/lushtext-core/src/app.rs` -- apply persisted color-scheme on startup before window creation

## Tasks & Acceptance

**Execution:**
- [x] `data/dev.cominotti.lushtext.gschema.xml` -- add `color-scheme` string key with default `'default'` and allowed values
- [x] `resources/ui/window.ui` -- restructure `primary_menu`: theme custom widget slot, New File section, Save/Save As section, Find/Replace section, Fullscreen/Leave Fullscreen section (hidden-when), Prefs/Shortcuts/About section; remove Open File and Open Folder items
- [x] `crates/lushtext-core/src/ui/window/mod.rs` -- create theme selector widget (GtkBox with 3 toggle buttons: light/system/dark); register via `PopoverMenu::add_child("theme")`; add `win.fullscreen`/`win.unfullscreen` actions with enable toggling based on `is_fullscreen()`; connect `notify::fullscreened` to toggle action enabled states; add F11 to setup_shortcuts; persist color-scheme to GSettings on change. Also extracted session/draft code to `session.rs` (1000-line limit).
- [x] `crates/lushtext-core/src/ui/window/imp.rs` -- add `primary_menu_button` template child for popover access
- [x] `crates/lushtext-core/src/app.rs` -- read `color-scheme` from GSettings in startup and call `StyleManager::set_color_scheme()` before window creation
- [x] `resources/ui/shortcuts.ui` -- add F11 fullscreen shortcut in General group
- [x] `crates/lushtext-core/src/services/palette.rs` -- add "Fullscreen" to `all_commands()`
- [x] Tests -- 10 new tests: fullscreen actions (existence, initial enabled state, toggle always enabled), color-scheme GSettings (key exists, roundtrip), primary_menu_button popover, menu structure (no Open File/Folder, has fullscreen items, has New File), command registry (fullscreen entry)

**Acceptance Criteria:**
- Given the app is running, when clicking the hamburger menu, then the menu sections match GNOME Text Editor's layout (theme → new file → save group → find/replace → fullscreen → prefs group)
- Given the menu is open, when selecting Light/System/Dark in the theme selector, then the app appearance changes immediately and the choice persists across restarts
- Given a non-fullscreen window, when pressing F11 or clicking "Fullscreen", then the window enters fullscreen and the menu shows "Leave Fullscreen" instead
- Given no tabs are open, when opening the menu, then Save, Save As, and Find/Replace are grayed out while New File, Fullscreen, and Preferences remain active
- Given the hamburger menu is open, then "Open File" and "Open Folder" do NOT appear in the menu (they remain as header bar buttons only)

## Design Notes

GNOME Text Editor's theme selector is a custom widget inserted into the `GtkPopoverMenu` via `<attribute name="custom">theme</attribute>` in the menu XML. In gtk4-rs, this maps to `popover_menu.add_child(&widget, "theme")`. The widget itself is a horizontal `GtkBox` with 3 `GtkToggleButton` instances styled as a linked group (CSS class `linked`), each showing an icon: `weather-clear-symbolic` (light), `preferences-desktop-appearance-symbolic` (system), `weather-clear-night-symbolic` (dark). Only one can be active at a time (radio-button behavior via manual `set_active` toggling).

The fullscreen toggle uses two separate menu items with `hidden-when: action-disabled`. `win.fullscreen` is enabled when NOT fullscreen; `win.unfullscreen` is enabled when IS fullscreen. `notify::fullscreened` on the window toggles which action is enabled, causing GTK to show/hide the corresponding menu item.

## Verification

**Commands:**
- `make check` -- expected: clippy + fmt pass
- `make test` -- expected: all existing + new tests pass
- `make run` -- expected: menu matches GNOME Text Editor layout; theme selector works; F11 toggles fullscreen; no GTK warnings on stderr

## Suggested Review Order

**Menu structure (entry point)**

- Restructured primary_menu: 6 sections matching GNOME Text Editor, custom theme slot, fullscreen hidden-when
  [`window.ui:135`](../../resources/ui/window.ui#L135)

**Theme selector**

- Shared string-to-ColorScheme mapping used by startup and theme widget
  [`mod.rs:39`](../../crates/lushtext-core/src/ui/window/mod.rs#L39)

- 3 linked ToggleButtons as radio group, wired to StyleManager + GSettings persistence
  [`mod.rs:835`](../../crates/lushtext-core/src/ui/window/mod.rs#L835)

- PopoverMenu::add_child with error logging on all failure paths
  [`mod.rs:897`](../../crates/lushtext-core/src/ui/window/mod.rs#L897)

- Persisted color scheme applied before first window creation
  [`app.rs:36`](../../crates/lushtext-core/src/app.rs#L36)

**Fullscreen toggle**

- Two actions with hidden-when + F11 toggle, wired via notify::fullscreened
  [`mod.rs:785`](../../crates/lushtext-core/src/ui/window/mod.rs#L785)

**Schema and config**

- New color-scheme GSettings key (string, default 'default')
  [`gschema.xml:34`](../../data/dev.cominotti.lushtext.gschema.xml#L34)

- COLOR_SCHEME config constant
  [`config.rs:14`](../../crates/lushtext-core/src/config.rs#L14)

- primary_menu_button template child for popover access
  [`imp.rs:55`](../../crates/lushtext-core/src/ui/window/imp.rs#L55)

**Supporting changes**

- F11 shortcut entry in shortcuts window
  [`shortcuts.ui:56`](../../resources/ui/shortcuts.ui#L56)

- Fullscreen command added to palette registry
  [`palette.rs:288`](../../crates/lushtext-core/src/services/palette.rs#L288)

- Session/draft code extracted to stay under 1000-line limit (pure refactor)
  [`session.rs:1`](../../crates/lushtext-core/src/ui/window/session.rs#L1)

**Tests (12 new)**

- Fullscreen actions, color scheme, menu structure, parse_color_scheme
  [`window.rs:1414`](../../crates/lushtext/tests/widget/window.rs#L1414)

- Command registry completeness
  [`command_palette.rs:601`](../../crates/lushtext/tests/widget/command_palette.rs#L601)
