# LushText Accessibility Guide

LushText is a GTK4, Libadwaita, and GtkSourceView text editor. Its
accessibility goal is to behave like a native GNOME application: controls use
GTK accessibility metadata, keyboard workflows follow normal GTK action paths,
and screen readers observe the same visible surfaces that pointer users see.

This guide describes the product contract and the proof lanes used by
maintainers. It is not a legal accessibility certification, and it does not
claim behavior that the current GTK, desktop, or screen-reader stack cannot
expose. When a host-sensitive check skips, that skip is evidence about the
host, not proof that the skipped behavior works.

## Keyboard Operation

Most workflows are available through the menu, header controls, command
palette, or a direct shortcut. The complete in-app list lives under
**Main Menu > Keyboard Shortcuts**.

| Workflow | Keyboard path |
| --- | --- |
| New file | `Ctrl+N` |
| Open file | `Ctrl+O` |
| Open recent documents | `Ctrl+K` |
| Save / Save As | `Ctrl+S` / `Ctrl+Shift+S` |
| Close tab | `Ctrl+W` |
| Print | `Ctrl+P` |
| In-tab find / replace | `Ctrl+F` / `Ctrl+H` |
| Next / previous in-tab find match | `Ctrl+G` / `Ctrl+Shift+G` |
| Command palette | `Ctrl+Shift+P` |
| Workspace search | `Ctrl+Shift+F` |
| Workspace search next / previous result | `F4` / `Shift+F4` |
| Toggle bookmark | `Ctrl+F2` |
| Edit bookmark label | `Ctrl+Shift+F2` |
| Next / previous bookmark | `F2` / `Shift+F2` |
| Browse bookmarks | `Ctrl+Alt+B` |
| Browse notes | `Ctrl+Alt+A` |
| Local History | `Ctrl+Alt+L` |
| Document properties | `F9` |
| Fullscreen | `F11` |
| Focus Mode | `Ctrl+Shift+F11` |
| Markdown preview-only mode | `Alt+P` |
| Toggle minimap | `Ctrl+Shift+M` |
| Cycle invisible-character display | `Ctrl+Shift+I` |
| Zoom in / out / reset | `Ctrl++`, `Ctrl+-`, `Ctrl+0` |

Common GTK navigation also applies:

- `Tab` and `Shift+Tab` move between focusable controls.
- `Enter` or `Space` activates focused buttons, rows, and toggles when the
  focused widget supports activation.
- `Escape` dismisses the topmost transient surface, such as the command
  palette, search bar, Open popover, context menu, dialog, or bottom sheet,
  then restores focus to the documented workflow target when possible.
- Arrow keys navigate list-like controls, menus, popovers, search results, and
  file-tree rows according to GTK behavior.
- Context-menu actions should be available through the keyboard context-menu
  key or the desktop's equivalent shortcut, commonly `Shift+F10`, where the
  session provides one.
- Sidebar file peek opens from the selected file row with `Space` and promotes
  to a real editor tab through the same open-document path as pointer
  activation.

Pointer-only and hover-only affordances are not considered sufficient. Row
actions that appear on hover must have an equivalent keyboard, context-menu,
menu, or command-palette path.

## Screen-Reader Expectations

LushText exposes product-facing accessible names, roles, descriptions, states,
and relations through GTK rather than through hidden automation-only labels.
Stable names such as `Open recent documents`, `Command palette query`,
`Workspace search results`, `Document properties`, and `Markdown preview` are
intended to be meaningful to assistive technology users as well as useful for
smoke tests.

Expected screen-reader behavior includes:

- The main window exposes the tab strip, shell buttons, workspace sidebar
  toggle, document-properties toggle, status metadata, editor region, and
  transient surfaces with useful roles and names.
- The active GtkSourceView editor exposes an editing surface identified by the
  active document's bounded display name, not by document contents.
- Editor read-only states are surfaced when loading, saving, large-file policy,
  preview-only mode, or a failure state temporarily prevents editing.
- Search fields expose names, options, invalid/no-result states, match counts,
  and debounced result announcements without announcing every keystroke.
- Open popover, command palette, workspace search, file tree, notes, bookmarks,
  local history, preferences, and properties rows refresh accessible metadata
  when GTK recycles row widgets.
- Dialogs, popovers, bottom sheets, file peek, search surfaces, and menus stop
  exposing hidden controls as visible focus targets after dismissal settles.
- Alerts, durability warnings, failed loads, destructive confirmations, Replace
  All completion, undo availability, recovery warnings, and user-initiated
  long-running operations use bounded announcements or alert semantics.

The current proof stack uses AT-SPI and Orca-oriented GNOME behavior as the
reference path. The AT-SPI smoke helper records tree, focus, and text-interface
evidence where the host exposes it. Some GTK or host combinations may omit a
focused node, expose a combo box by selected value instead of control label, or
limit automated caret and selection detail for GtkSourceView. Those cases must
be documented as caveats and covered by manual screen-reader checks before a
release claims that behavior.

## Visual Accessibility

LushText inherits the GTK and Libadwaita visual accessibility stack and adds
app-level checks for surfaces that are easy to break in a text editor:

- Keyboard focus must remain visible on shell controls, editor surfaces, rows,
  dialogs, popovers, bottom sheets, context menus, search controls, and compact
  layouts.
- Important state must not be communicated by color alone. Warnings, errors,
  destructive actions, disabled controls, search matches, selected rows,
  modified tabs, bookmarks, file-health states, and local-history restore state
  need text, iconography, role/state metadata, shape, or position in addition
  to hue.
- Dark mode and high-contrast variants should keep focus, selection, warnings,
  errors, disabled controls, and destructive actions distinguishable where the
  host supports those variants.
- Large text and constrained geometry must keep primary actions, close/back
  controls, persistent chrome, and item scrolling regions reachable.
- Reduced-motion checks record the host setting and verify that keyboard paths
  and focus restoration remain semantic equivalents of the normal-motion path
  where the desktop stack supports the setting.
- The editor and Markdown preview `Background Opacity` preference affects only
  document content surfaces. Header bars, side panels, status/search chrome,
  minimap, empty states, and other shell surfaces remain opaque so controls
  stay readable.

## Smoke Coverage

Accessibility proof is intentionally layered:

| Lane | Command | What it proves |
| --- | --- | --- |
| Widget tests | `make test-widget-headless` | GTK state, focus restoration, row recycling, metadata helper behavior, and keyboard wiring while the accessibility bridge is disabled. |
| Accessibility smoke | `make accessibility-smoke` | AT-SPI-visible names, roles, focus paths, editor text evidence, scenario manifests, warning scans, and unsupported-host reasons. |
| Visual geometry smoke | `make visual-geometry-smoke` | Same-session protected regions, focus/fixed-control geometry, screenshot-derived pixel anchors, and current visual-proof policy summaries. |
| Visual smoke | `make visual-smoke` | Rendered desktop screenshots for representative states, compact layouts, dense rows, dialogs, dark/high-contrast/large-text/reduced-motion variants, transparency/readability, and other user-visible surfaces. |
| Policy checks | `make check-accessibility-policy`, `make check-visual-proof-policy`, `make check-automation-docs` | Drift checks for helper use, stable AT-SPI anchors, smoke helper flags, automation docs, and current visual evidence. |

`make accessibility-smoke` writes bounded artifacts under
`build/smoke/accessibility` by default:

- `summary.json` and `summary.txt`
- `accessibility-assertions.jsonl`
- per-scenario manifests under `assertions/*-manifest.json`
- AT-SPI tree and focus excerpts
- warning scans and environment reports

Use focused scenario filters when debugging one surface:

```sh
scripts/run-accessibility-smoke.sh --list-cases
scripts/run-accessibility-smoke.sh --case command-palette-no-results
scripts/lushtext-automation.py artifact-summary build/smoke/accessibility --json
scripts/run-visual-smoke.sh --list-cases
scripts/run-visual-smoke.sh --case large-text-constrained
scripts/lushtext-automation.py artifact-summary build/smoke/visual --json
```

Artifacts must stay bounded. They may contain committed fixture names, visible
paths, counts, roles, states, short status strings, and fixture text created for
the smoke run. They must not dump private user document contents, note bodies,
draft bodies, complete search result text, local-history contents, or private
persistence identifiers.

## Release Reference Checks

Before a public release that changes UI, shortcuts, accessibility metadata,
search/list surfaces, visual styling, or smoke tooling, maintainers should use
both automated and manual evidence:

1. Run the normal release preflight and preserve the end-user smoke artifacts
   described in `docs/end-user-coverage.md`.
2. Run `make accessibility-smoke` on a host with AT-SPI, D-Bus, Mutter, and the
   required Python accessibility bindings. Review `summary.json` and do not
   count unsupported cases as verified coverage.
3. Run `make visual-geometry-smoke`, `make visual-smoke`, and
   `make check-visual-proof-policy` when the release changes focus styling,
   row factories, transient surfaces, CSS, visual smoke tooling, or geometry
   that affects keyboard or low-vision users.
4. Perform a manual Orca check in a normal GNOME session for the changed
   workflows. At minimum, verify shell navigation, editor focus, typing,
   caret/selection feedback where available, in-tab search, command palette,
   Open popover, workspace search, workspace sidebar/file tree, document
   properties, preferences, Markdown preview, notes/bookmarks, local history,
   and destructive or close dialogs affected by the release.
5. Record any host limitation, skipped smoke lane, or screen-reader caveat in
   the release validation notes along with the runner or manual environment
   that covered it.

## Known Platform Caveats

- Widget tests intentionally run with `NO_AT_BRIDGE=1`; they are valuable for
  metadata and GTK behavior, but they are not real screen-reader proof.
- AT-SPI behavior depends on the desktop session, accessibility bridge, screen
  reader, compositor, and Python bindings installed on the host.
- Headless AT-SPI smoke can expose a visible accessibility tree while omitting
  the currently focused node. The helper treats that as a caveat only when the
  expected focus target remains visible in the same tree.
- GTK accessible announcements are verified through widget-level emission and
  throttling hooks in automated tests. The headless AT-SPI helper records tree,
  focus, text, and warning artifacts, but it does not capture Orca speech
  output; changed announcement behavior still needs a manual Orca check in a
  normal GNOME session before release.
- GtkSourceView owns the text editing implementation. LushText proves the
  app-owned editor identity, editability state, focus path, and supported text
  behavior around it, but it does not reimplement GTK text accessibility.
- High contrast, text scale, reduced motion, and screenshot capture vary by
  desktop and renderer. Unsupported variants must skip explicitly and cannot be
  counted as passing evidence.
- Flatpak confinement diagnostics are separate from accessibility. LushText's
  current Flatpak keeps full filesystem access; portal checks are diagnostic,
  not an accessibility or sandbox migration claim.

## Reporting Accessibility Bugs

Please include:

- LushText version and whether it was installed from Flatpak, Snap, or a source
  checkout.
- Operating system, desktop session, display backend, theme, text scale, and
  high-contrast or reduced-motion settings if relevant.
- Screen reader and version, for example Orca in a GNOME session.
- Whether the issue happens by keyboard, pointer, screen reader, or a
  combination.
- Steps to reproduce, expected result, and actual result.
- The surface involved, such as editor, command palette, Open popover,
  workspace search, sidebar file tree, notes, local history, properties,
  preferences, Markdown preview, or a dialog.
- Any relevant bounded smoke artifact summary, such as
  `build/smoke/accessibility/summary.json`, if you were running from a source
  checkout.

Do not include private document text unless it is necessary and you are
comfortable sharing it. A short synthetic fixture that reproduces the behavior
is usually better.
