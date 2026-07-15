# Manual Orca Accessibility Checklist

Use this template for release-grade accessibility validation in a normal GNOME
session. Automated widget, AT-SPI, visual, and policy lanes are required, but a
skipped or headless-only result does not prove user-facing speech behavior.

Copy this file into the release validation notes or fill it in as a separate
artifact. Keep all text bounded. Prefer synthetic fixtures and matrix row ids
over private document contents.

## Environment

| Field | Value |
| --- | --- |
| LushText build | |
| Commit or tag | |
| Install mode | Flatpak / source checkout / other |
| Operating system | |
| GNOME session | |
| Display backend | Wayland / X11 / headless / other |
| Theme | light / dark / high contrast / custom |
| Text scale | |
| Reduced motion | enabled / disabled / unknown |
| Orca version | |
| AT-SPI bridge status | |
| Matrix rows | |
| Automated artifacts | `build/smoke/accessibility/summary.json`, `build/smoke/visual/summary.json`, `build/smoke/visual-geometry/summary.json`, widget test output, policy output |

## Privacy Boundary

| Field | Value |
| --- | --- |
| Synthetic fixture | yes / no |
| Private user data | no |
| Bounded text reviewed | roles, names, counts, short fixture strings, status summaries |
| Excluded content | private document text, note bodies, draft bodies, local-history bodies, complete search results, private sidecar identifiers |

## Workflow Results

Use `pass`, `fail`, `caveat`, or `not run` for Outcome. A caveat should name
the exact host, GTK, AT-SPI, Orca, compositor, or fixture limitation.

| Matrix rows | Workflow | Expected Orca behavior | Outcome | Caveats | Automated artifacts |
| --- | --- | --- | --- | --- | --- |
| A11Y-SHELL-NO-CONTEXT, A11Y-SHELL-REPRESENTATIVE | Shell navigation, header controls, tab strip, status bar | Orca reports product-facing control names, active document identity, toggle states, and status metadata without document body text. | | | |
| A11Y-EDITOR-REPRESENTATIVE | Editor focus, typing, caret feedback, selection feedback | Orca reaches the editable GtkSourceView region and reports text editing feedback from GTK/GtkSourceView. | | | |
| A11Y-EDITOR-BUSY, A11Y-EDITOR-ERROR, A11Y-EDITOR-LARGE-READONLY | Editor loading, saving, failed load/save, large-file readonly policy | Orca reports readonly, busy, error, or policy state and the safe next action without repeated noise. | | | |
| A11Y-EDITOR-SEARCH | In-tab search and replace | Orca reports query/replacement fields, result counts, no-result or invalid state, next/previous controls, and editor focus after dismissal. | | | |
| A11Y-PALETTE-FILES, A11Y-PALETTE-COMMANDS, A11Y-PALETTE-NOTES, A11Y-PALETTE-NO-RESULTS, A11Y-PALETTE-DISMISS | Command palette | Orca reports query, mode, selected result rows, shortcuts where available, no-results state, and focus restoration. | | | |
| A11Y-OPEN-EMPTY, A11Y-OPEN-DENSE-FILTERED, A11Y-OPEN-HIDDEN | Open recent popover | Orca reports search, empty state, recent rows, remove buttons, open-another-file action, and focus after dismissal. | | | |
| A11Y-WORKSPACE-SEARCH-NO-CONTEXT, A11Y-WORKSPACE-SEARCH-REPRESENTATIVE, A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS, A11Y-WORKSPACE-SEARCH-REPLACE | Workspace search and Replace All | Orca reports query/options, result count, selected rows, replace preview, confirmation/completion, and undo availability with bounded counts. | | | |
| A11Y-WORKSPACE-NO-CONTEXT, A11Y-WORKSPACE-ZERO-FOLDER, A11Y-WORKSPACE-REPRESENTATIVE, A11Y-WORKSPACE-DENSE-DEEP, A11Y-WORKSPACE-BUSY-ERROR, A11Y-WORKSPACE-PEEK | Workspace sidebar and file tree | Orca reports workspace selector, section headers, file/folder rows, expansion/selection, refresh state, file peek, and focused-folder behavior. | | | |
| A11Y-WORKSPACE-CONTEXT, A11Y-WORKSPACE-DRAG-DROP, A11Y-CONTEXT-MENUS-GENERAL | Context menus and pointer-convenience fallbacks | Orca reaches menu actions through keyboard paths such as Menu, Shift+F10, command palette, or equivalent fallback without pointer coordinates. | | | |
| A11Y-PROPERTIES-NORMAL, A11Y-PROPERTIES-COMPACT | Document properties wide pane and compact bottom sheet | Orca reports one active properties surface, row labels and values, line ending/encoding controls, file health, and hidden-state cleanup. | | | |
| A11Y-MARKDOWN-REPRESENTATIVE, A11Y-MARKDOWN-CONSTRAINED, A11Y-EDITOR-FOCUS-PREVIEW | Markdown preview, preview-only mode, focus mode | Orca reports readonly preview content, explicit pending/limited/failure or image-fallback descriptions where applicable, preview mode versus editing mode, focus-mode state, and focus restoration after exit. | | | |
| A11Y-NOTES-EMPTY, A11Y-NOTES-POPULATED, A11Y-BOOKMARKS | Notes and bookmarks | Orca reports empty states, search fields, rows, previews, open/copy/edit/delete actions, and no private note body unless visible fixture text is expected. | | | |
| A11Y-LOCAL-HISTORY-EMPTY, A11Y-LOCAL-HISTORY-POPULATED | Local history | Orca reports empty state, snapshot rows, read-only preview, Copy, Restore, restore confirmation, and completion/caveats without dumping snapshot bodies. | | | |
| A11Y-PREFERENCES-PAGES, A11Y-PREFERENCES-DATA-SCAN | Preferences and Data page scan/migration surfaces | Orca reports page navigation, rows, switches, combo/spin values, scan progress, repair or retry actions, and migration warnings. | | | |
| A11Y-DIALOG-SAVE-CLOSE, A11Y-DIALOG-DESTRUCTIVE | Save, close, discard, delete, restore, migration, and format-upgrade dialogs | Orca reports alert title/body, affected item count or per-document checkbox labels, Cancel first, destructive/suggested responses, and keyboard cancellation. | | | |
| A11Y-SHELL-ERROR-STATUS, A11Y-RECOVERY-STARTUP, A11Y-ERROR-SURFACES | Recovery, startup, inline alerts, and error surfaces | Orca reports concise recovery/error state and available next actions; skipped automation remains unverified until covered here or on another runner. | | | |

## Sample Bounded Row

| Matrix rows | Workflow | Expected Orca behavior | Outcome | Caveats | Automated artifacts |
| --- | --- | --- | --- | --- | --- |
| A11Y-DIALOG-SAVE-CLOSE | Synthetic unsaved close dialog for `accessibility-smoke.txt` | Orca reports `Save Changes?`, `Save accessibility-smoke.txt`, `Cancel`, `Discard`, and `Save`. | pass | Synthetic fixture only; no private user document opened. | `build/smoke/accessibility/assertions/unsaved-close-dialog-manifest.json` |

## Current Host Unsupported Record

| Field | Value |
| --- | --- |
| Date | 2026-06-19 |
| Reason manual Orca was not run | `command -v orca` exited 1; Orca is not installed or not on `PATH` in this GNOME/Wayland session. |
| Host session evidence | `WAYLAND_DISPLAY=wayland-0`, `DISPLAY=:0`, `XDG_CURRENT_DESKTOP=GNOME` |
| Outcome | caveat |
| Alternate evidence plan | Treat screen-reader speech behavior as unverified on this host; require a later normal GNOME session with Orca installed before release-grade speech sign-off. Current automated alternate evidence is `make accessibility-smoke`, `make visual-smoke`, `make visual-geometry-smoke`, `make test-widget-headless`, and `make check-policy`. |
| Privacy boundary | Synthetic fixtures and bounded artifact summaries only; no private document, note, draft, local-history, or sidecar body text reviewed. |

## Sign-Off

| Field | Value |
| --- | --- |
| Reviewer | |
| Date | |
| Overall Outcome | pass / fail / caveat |
| Caveats | |
| Follow-up issues | |
