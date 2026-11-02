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

The completion source of truth is
[`docs/accessibility-matrix.md`](accessibility-matrix.md). That matrix owns the
row ids for each surface/state extreme, the mapping to smoke and visual proof
lanes, stable AT-SPI anchors, manual Orca expectations, and any explicit gaps.
When this guide changes a keyboard path, accessible name, announcement,
screen-reader expectation, visual accessibility rule, or release claim, update
the matrix in the same change.

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
| Switch tabs | `Ctrl+Page Up` / `Ctrl+Page Down`, `Alt+1`…`Alt+9`; `Ctrl+Home` / `Ctrl+End` (with `Shift`, select to) stay with the document's start and end, never the first or last tab, with any number of tabs open |
| Print | `Ctrl+P` |
| In-tab find / replace | `Ctrl+F` / `Ctrl+H` |
| Inspect Line (view-only documents with very long lines) | `Ctrl+Alt+I`; `Page Down`/`Page Up` move the window, `Alt+Down`/`Alt+Up` go to the next or previous folded line; Tab reaches Previous Window, Next Window, Previous Folded Line, Next Folded Line, Line, Column, Go, Copy Window and Copy Whole Line; `Escape` closes |
| Protected View menu | `Shift+F10` or `Menu` in a view-only document |
| Next / previous in-tab find match | `Ctrl+G` / `Ctrl+Shift+G` |
| Command palette | `Ctrl+Shift+P` |
| Workspace search | `Ctrl+Shift+F` |
| Workspace search next / previous result | `F4` / `Shift+F4` |
| Show / hide hidden files everywhere | `Ctrl+Shift+H` |
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

- The hidden-files view mode is one check item in the main menu, one switch in
  `Preferences > Workspace`, and one per-search `Hidden files` toggle in the
  workspace search panel; all report pressed/checked state. The excluded-names
  editor exposes one row per name with a labelled remove button, an add entry
  whose invalid state names the rejection reason, a reset control, and an
  explicit `No excluded names` row. Toggling the mode refreshes the tree without
  a manual-refresh announcement.
- `Preferences > Data` shows a **Preserved Drafts** group only when unsaved
  changes were set aside. Each row reads the original file (or `Untitled
  document` / `Unknown file`), when the changes were kept, and their size, with
  its position in the group; its `Open` and icon-only `Delete` buttons are
  labelled with the draft they act on. Delete confirms through a destructive
  alert whose default and Escape response is Cancel; when the confirmed draft
  changed or went away first, or could not be removed, an alert announcement
  says nothing was deleted. The group opens with a
  summary row that reads the count, the total size, whether the area is over
  the suggested limit, and, when not every body is listed, `showing the newest
  256 of N`; then a `Delete All Preserved Drafts…` button row whose destructive
  confirmation states the exact count and size it deletes (Cancel is again the
  default and Escape response). The per-draft rows scroll in their own bounded
  list, so the summary and Delete All stay reachable however many drafts are
  kept. When the area passes the soft bound, one status-bar warning names the
  count and size and the **Review Preserved Drafts** command (command palette
  or `app.review-preserved-drafts`), which opens this page with focus on the
  summary row; the warning is not repeated within a launch unless the area
  grows materially, and never after the review. The preserved body itself is
  never read out.
- Returning to the window refreshes the workspace sidebar and the command-palette
  file index so externally created files become reachable. This refresh is
  **silent**: it emits no announcement, and it preserves focus, selection, scroll
  position, and expansion state. A collapsed directory that gained content on
  disk stops reporting its empty state, and its expander and `Focus Folder`
  affordances become available, but that correction is not announced — the user
  did not initiate it, and interrupting them for it would be worse than the
  stale affordance it replaces. The same is true in reverse for a directory
  emptied on disk.

- The main window exposes the tab strip, shell buttons, workspace sidebar
  toggle, document-properties toggle, status metadata, editor region, and
  transient surfaces with useful roles and names.
- The active GtkSourceView editor exposes an editing surface identified by the
  active document's bounded display name, not by document contents.
- Editor read-only states are surfaced when loading, saving, large-file policy,
  preview-only mode, or a failure state temporarily prevents editing.
- Text typed while a document loads, saves, or has its content replaced (draft
  recovery, a local-history restore) is kept and inserted at the caret once the
  document is ready, as one undo step (`keep-keystrokes-during-install`). The
  first held character publishes one status message ("Typed text will appear
  when “name” is ready."), and while text is held the editor's description adds
  "Text you type is kept and inserted when the document is ready." The buffer,
  which AT reads, changes only at that insertion, so the editor stays reported
  read-only and busy until then; the insertion is an ordinary text change.
  Backspace removes held characters; Delete, undo and edits over a selection
  are refused with a rate-limited warning. Held text that cannot be inserted (the
  load failed or was cancelled, or the document opened in Protected View) is
  discarded with a warning that says so.
- The editor's accessible description names the features the document scale
  policy reduced for this document, grouped by reason class: for example
  "Syntax highlighting, bracket matching, the minimap and search highlighting
  are off because of very long lines.", "Undo history and automatic local
  history are off because of the document's total size.", or "This document is
  marked for Protected View because of very long lines." (a paragraph over
  50,000 characters; the batch density reduces no feature). It names features
  and reasons only, never document content, and is bounded by the fixed feature
  set. A document refused by open admission keeps its `disabled` state and the
  sentence "This document is too large to open."; the former "Large document
  mode is active." sentence (the retired 1 MB tier) is gone. A paragraph
  longer than the wrap threshold (8,192 characters) is shown unwrapped while
  the rest of the document keeps wrapping; the list then names "word wrap of
  the longest lines" under "very long lines". The word-wrap preference is not
  changed.
- The input method receives a bounded window of text around the cursor
  (1,024 characters on each side, within the cursor's paragraph) when it asks
  for surrounding text, instead of the whole paragraph; input methods that
  read surrounding text (GNOME's IBus path) see what GTK's text-input protocol
  sends anyway.
- The reduced-features notice is an announced status-bar message (warning
  style) carrying the same list, published when the document's tab is the
  selected one, plus a persistent "Editor Features" row in the document
  properties (accessible name "Editor features", value text the same list).
  Both are bounded through `a11y::bounded_announcement_text` and contain no
  document text. It is announced once per document session and again only
  when an edit reduces an additional feature. It never uses the editor's
  inline alert slot, so recovery and error alerts keep that slot, and the
  text view is never reallocated by it.
- Search fields expose names, options, invalid/no-result states, match counts,
  and debounced result announcements without announcing every keystroke.
- Markdown preview exposes explicit pending, complete, complete-with-omissions
  (`Simplified`), limited, failed, and cancelled descriptions; budget limits and
  unsupported images resolve to a readable fallback instead of an unlabeled
  partial preview. `Simplified` and `Limited` are deliberately distinct: the
  first says the document rendered to its end with named holes, the second says
  a whole-document ceiling stopped rendering early.
- When the preview replaces a unit it cannot render, the omission count is
  announced **once** through the preview surface's accessible description at the
  terminal, not once per marker. The count reports user-visible omissions only;
  an oversized table or code block that the preview already replaces with its
  own in-place summary widget still reports `Complete`, because that widget
  already names the block and its true size in place.
- Omission markers have two affordances, and the distinction is intentional:
  - the marker that replaces one **table row** is a real accessible object — a
    full-width cell inside the table's `Table` role, labelled with the bare
    marker text rather than the `Table cell ...` prefix used for real data
    cells, because it reports a missing row instead of carrying table data — so
    it is reachable by object navigation. The table's own announced shape counts
    source rows only, so a marker row never inflates the reported row count;
  - the markers for a **list item**, **quoted paragraph**, **definition body**,
    **code-block line**, and a top-level omission past the placeholder-widget
    cap are plain preview buffer text with no accessible object of their own.
    They are reachable through the preview's text interface, in reading order at
    the position of the content they replaced, and each names both the budget and
    the omitted unit. Giving each one its own accessible object would be a
    presentation change and is deliberately out of scope.
  - a top-level omission within the placeholder-widget cap reuses the same
    labelled group fallback widget as the existing table/code/image fallbacks.
- Proof coverage for those markers is uneven on purpose, and the split is
  recorded rather than implied. The **table-row** marker is asserted end to end:
  a widget test checks the spanning row and its text, and the
  `markdown-preview-omissions` accessibility smoke case asserts it as a labelled
  AT-SPI cell. The **list-item** marker is asserted by a widget test plus one
  text-interface assertion in that same smoke case. The **quoted-paragraph**,
  **definition-body**, and **code-block-line** markers currently have unit
  coverage of their unit-naming projection only
  (`enclosing_unit_name_reads_the_live_continuation`), with no widget or smoke
  assertion; extending the smoke case to them would mean giving each its own
  accessible object, which is the presentation change held out of scope above.
- Open popover, command palette, workspace search, file tree, notes, bookmarks,
  local history, preferences, and properties rows refresh accessible metadata
  when GTK recycles row widgets.
- Dialogs, popovers, bottom sheets, file peek, search surfaces, and menus stop
  exposing hidden controls as visible focus targets after dismissal settles.
- Alerts, durability warnings, failed loads, destructive confirmations, Replace
  All completion, undo availability, recovery warnings, and user-initiated
  long-running operations use bounded announcements or alert semantics.
- An inline alert (a load error, an external change, a restored draft, a
  line-ending warning, a local-history restore) is announced **once**, at the
  first moment it becomes perceivable: when it appears on the selected
  editor, when preview-only mode is entered while it is still unannounced, or
  when the background tab it was raised on is first selected. An alert raised
  on a background tab is not announced while nobody can see it. Re-rendering,
  switching tabs or layouts back, and other tabs' notifications announce
  nothing; a changed alert, or one that comes back after being cleared, is
  new. Errors keep the high-priority alert lane and warnings the status-update
  lane.
- In Markdown preview-only mode (`Alt+P`) the editor, and its own inline alert,
  is hidden, so the selected document's alert is shown again directly above
  the preview, with the same title, body, style, and buttons, over rendered
  Markdown and over every preview placeholder. It is a sibling above the
  preview's `document` accessible, not inside it, and keeps the alert's
  `Group` row and `Alert` title; the hidden editor's alert is not exposed. Tab
  reaches its actions before the preview content. Its buttons run the
  editor's own handlers. When an action resolves the alert while focus is on
  one of its buttons, focus moves to the preview text when it is shown and
  otherwise to nothing, never to the workspace sidebar; the editor's own alert
  returns focus to the document in the same case. Escape does not dismiss
  either surface: inline alerts are persistent document state.
- A status-bar warning or error is announced when it is published and takes
  the status lane, through the status bar's throttled lanes. Re-rendering the
  lane for an expiry sweep, a resolve, a heartbeat, an inline alert, a tab
  switch, or a layout change does not announce it again.
- The stale-draft recovery alert ("Draft Not Restored") announces where the
  unsaved edits were kept — Local History, or the drafts set-aside folder —
  without reading any draft text. When they went to Local History, its
  primary button is named "Show in Local History" and is reachable with Tab
  like every inline-alert action; activating it opens the Local History
  browser for that file.

The current proof stack uses AT-SPI and Orca-oriented GNOME behavior as the
reference path. The AT-SPI smoke helper records tree, focus, and text-interface
evidence where the host exposes it. Some GTK or host combinations may omit a
focused node, expose a combo box by selected value instead of control label, or
limit automated caret and selection detail for GtkSourceView. Those cases must
be documented as caveats and covered by manual screen-reader checks before a
release claims that behavior.

### Staged window composition

A window is built whole and composed in stages (`first-frame-window-shell`):
its first frame shows the header bar, the tab strip and the status bar, and the
content, the workspace sidebar and the document-properties panel are revealed
over the next few frames. Invisible widgets are absent from the accessibility
tree, so their AT-SPI anchors appear when their stage reveals them; the
children of concealed revealers (search panel, command palette, Focus Mode
affordance) are absent until their revealer opens. Accessible metadata is set
when the window is constructed, so nothing changes about a surface's name,
role or description when it appears. A keyboard path that needs a staged
surface (`F9`, the sidebar toggle, editor focus) reveals it first, so no key
press is lost during composition. A document opened from the command line, a file
manager or the restored session takes keyboard focus when it opens: GTK gives
a window its initial focus when it is shown, before any document page exists,
and that pick (the status bar's sidebar toggle) used to keep the keyboard, so
the first characters typed after opening a document went nowhere. The handoff
happens only when the open added or selected a page in that window, and only
while the focus is still where GTK put it: unset, or on the window's own
chrome (header bar, tab strip, status bar). It never takes focus from a dialog,
an open transient surface, a popover or menu (the Open popover, `Ctrl+K`, and
the tab menus are the user's surfaces even though the chrome owns them), or a
widget the user moved focus to, and once the window has received any key or
button press since it was mapped it moves nothing: a screen-reader user
tabbing through the chrome while a session restores keeps their place
(`investigate-first-keys-without-frames`). The handoff happens before the
document's inline alerts are announced, so the focus change never cuts off a
one-time announcement such as Protected View's "View-Only: Very Long Lines". `make accessibility-smoke` waits on
`accessibility-settled`, which includes the `window-composition` blocker.

### Protected View and the Line Inspector

A document with a paragraph over 50,000 characters opens view-only
(Protected View). Assistive technologies read the **folded view**: each giant
paragraph's first characters, never its hidden tail. The editor's accessible
description says so and names the reading path: "View-only: very long lines are
shortened in this view. Press Ctrl+Alt+I to inspect or copy a whole line." One
inline alert explains the view-only state and offers Inspect Line (or Undo Edit
when an edit made the document view-only); a recovered draft keeps its Discard
and Save actions and names the shortcut in its body. A blocked request (paste,
typing, Ctrl+H, bookmark toggles, invisible characters, line endings) publishes
one status message, at most once every four seconds per reason.

**While such a document opens** (or is recovered, reloaded or restored), each
giant paragraph's hidden tail goes into the buffer in slices of at most
256 KiB, and the paragraph's visible head goes in last
(`sliced-protected-install`). Each slice is one accessible `TextChanged`
insertion event whose payload is that slice's text (the event is emitted before
the slice is hidden), so a listener receives the tail in bounded pieces rather
than as one event the size of the whole paragraph; the clear before a reload
or eviction emits bounded deletion events that carry no hidden characters
(ledger A78). Assistive technologies should treat these events as the document
loading, as they already do for any large document; the tab reports the
document as loading until the install completes.

The **Line Inspector** is the accessible path to folded content. It is a dialog
holding one window of at most 500 characters of one line, with word wrap on,
read-only, with a visible caret and selection, so screen-reader reading and
selection stay bounded. Its text area's accessible label is the window's
position and, for a folded line, what the editor hides ("Line 2, columns
150001–150500 of 300000; 299500 characters hidden in the editor"), which is
also announced politely each time the window moves. Previous Window, Next
Window, Previous Folded Line and Next Folded Line (`Alt+Up`/`Alt+Down`), Line,
Column, Go to Line and Column, Copy Window and Copy Whole Line are labelled
controls, so every shortened line and its hidden count are reachable from the
keyboard. Copy Whole Line copies the whole line (it is written to the
clipboard in bounded chunks when another application pastes) and says so in
the status bar and to assistive technology: "Line 2 copied (300000
characters)". The fold-marker gutter shows `⋯` beside each shortened line
while the gutter shows line numbers or marks; its tooltip gives the
hidden-character count, and the renderer is labelled "Folded lines" with a
description naming the Inspect Line shortcut (a gutter has no per-line
accessible objects, so the count is spoken in the inspector's label instead).

The **local-history preview and the note editors** show a body with a
paragraph over 50,000 characters folded and read-only (a note says so in a
line above its body, and its text view is labelled "Note body, read-only").
Selecting and copying there (Ctrl+C, the context menu, the primary selection)
copies the selected text in full, hidden tails included, from the snapshot or
note itself; a drag of such a selection does not start, because it would carry
only the visible characters.

A paste of more than 16 KiB into a single-line field (Find, Replace, the
command palette, the Open popover, names, the inspector's position fields) is
cut at 16 KiB, and a status message (announced) says so: a much longer line
would stall the window every time it is laid out again.

The main view stays focusable: C2's lane AT case measured the folded view's
text and caret queries within budget, so the P4 fallback (making the main view
non-focusable) is not applied.

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
| Responsiveness smoke | `make responsiveness-smoke` | Cost of the accessibility bridge itself on long-line documents: every case also runs with the AT bridge on and a `pyatspi` listener for text-changed, caret-moved and selection-changed events. It adds no anchors and asserts no accessible names; it is the evidence for ledger axiom A35. |

Every scenario in the accessibility, visual, and visual-geometry lanes should
declare the relevant matrix row ids from
[`docs/accessibility-matrix.md`](accessibility-matrix.md). Focused runs are
diagnostic evidence; release-grade accessibility coverage needs unfiltered
current-tree summaries or an explicit scoped release note that names any matrix
rows intentionally left uncovered.

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
scripts/run-accessibility-smoke.sh --case preview-mode-document-alert
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
   and destructive or close dialogs affected by the release. Use
   [`docs/accessibility-orca-checklist.md`](accessibility-orca-checklist.md)
   as the reviewable artifact template.
5. Record any host limitation, skipped smoke lane, or screen-reader caveat in
   the release validation notes along with the runner or manual environment
   that covered it. Use the matrix row ids when recording coverage or caveats
   so skipped rows cannot be mistaken for verified behavior.

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
