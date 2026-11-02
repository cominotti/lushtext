## Why

`long-line-protected-view` (C2) ships the core of Protected View: a document whose longest paragraph exceeds `L_PROTECT_EDIT` (50,000 characters) opens view-only, caret-free and folded at insertion; find, copy and save read an immutable text source; recovery runs through the fold; and an edit entry has one bounded undo. The maintainer chose (2026-10-31) to ship that core before its reading surfaces. Until this change lands, the folded tails cannot be read or copied by an ordinary user: clicks, drags and select-all are claimed so no selection can be made, and there is no surface that shows a hidden tail. Find can locate text in a hidden tail but only scrolls to its paragraph's start. This change adds the surfaces that make the folded content reachable and the remaining secondary views consistent.

## What Changes

- **Line Inspector** (C2 tasks 5.2–5.5): a Blueprint dialog over a window of at most `N_inspect` characters of one paragraph, read on a worker from `ProtectedTextSource`, with Previous Window, Next Window and Go to Column; the paragraph index and character checkpoints in the source; a lazy whole-line `GdkContentProvider` ("Copy Whole Line") writing one chunk per dispatch; the Inspect Line action in the action catalogue, the command palette and a shortcut. Protected find results in a hidden tail open the inspector on their window (C2 6.3), and the inspector holds a readiness blocker (`line-inspector` or an existing one) with `line_inspector_visible` in the surface snapshot.
- **Protected View menu** (C2 4.4): the secondary-press and keyboard menu bindings open a menu with Inspect Line, Copy Whole Line and Find, the disabled entries insensitive.
- **Fold gutter** (C2 4.5): a gutter renderer bounded by visible rows marking each folded paragraph, with the hidden-character count in its tooltip and accessible label; activation opens the inspector.
- **Notice and Undo button**: one bounded Protected View inline alert (view-only because of very long lines, folded paragraph count, Inspect Line, and Undo while the entry undo is available), merged with the restored-draft alert for recovered drafts, replacing C1's reduced-features message for the same document; pre-empted user requests (paste, Ctrl+H, cycling invisibles, toggling a bookmark, an unavailable Ctrl+Z) re-surface it or publish one rate-limited status message. The alert copy must not promise to undo "the last keystroke": the entry undo removes the whole entering user action (C2 design D2).
- **Accessibility and Orca** (C2 8.3–8.4): the accessible description of the protected view naming the Inspect Line shortcut, inspector labels and announcements, gutter labels, the P4 fallback (main view non-focusable, inspector as the reading path) if the lane's AT case requires it, `docs/accessibility.md`, the matrix, the Orca checklist entry and the accessibility-smoke inspector path.
- **Remaining surfaces** (C2 7.5–7.8): local-history preview through the fold executor with a caret-free preview and no capture for installed-protected documents; the Markdown preview limited terminal, the file-peek `head_cap` sample bound and read-only note editors; bookmark toggles and line-ending normalization disabled, bookmark navigation through `navigation_target`; session restore scrolling through the navigation mark without moving the insert mark and keeping the persisted cursor offset.
- **Navigation from other workflows**: workspace-search hits, bookmarks, local-history targets and session restore positions scroll through the navigation mark, never moving the insert mark (today they place the cursor, which in a protected document is invisible but moves the mark).
- **Save As of a non-round-trippable decode** (C2 7.1 rest): the round-trip exactness fact in `LoadMetadata`, and Save As of an unmodified protected document refused with a notice when its decode does not round-trip. Not a regression meanwhile: ordinary Save As re-encodes the same way.
- **Remaining input-gate tests** (C2 4.7 rest): the class-shortcut enumeration, touch drag, DnD drop refusal, and the notice appearing once, if C2 did not land them.

## Review notes carried from C2

- Copy is served from the source but needs a selection, which today only an assistive technology can make; Copy Whole Line and the inspector are the ordinary user's copy path.
- A protected save writes the source verbatim (C2 design D9): an edit-entered document that gained LF lines inside a CRLF file is saved with mixed endings, not normalized. The alert or Save As path should say so if the maintainer wants it visible.
- The inspector's paragraph index is built on the worker over C2's source, whichever entry produced it (the install body, or the edit entry's iterator capture, `capture_execution.rs`).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `long-line-document-presentation`: the Line Inspector, secondary views and fold gutter requirements, and the menu, navigation and find-activation text C2 narrowed.
- `editor-inline-alerts`: the Protected View inline alert.
- `gtk-accessibility-spine`: the accessible reading path.
- `local-history`, `line-bookmarks`, `draft-session-recovery` (session restore): the surfaces above.
- `main-thread-responsiveness`, `dbus-automation-spine`, `live-editor-memory-budget`, `workflow-readability-boundaries`, `document-save-safety`, `invisible-character-visualization`, `search-replace-safety`: the inspector, notice and copy text C2 narrowed, restored.

## Impact

- **Depends on** `long-line-protected-view` (C2) being archived: its MODIFIED deltas restore the text C2 narrowed when it deferred these surfaces.
- **Code:** `ui/editor_page/protected_view/` (`inspector_execution.rs`, `inspector.rs` and `fold_gutter.rs` as called presentation surfaces), `resources/ui/line-inspector.blp`, the action catalogue, `ui/info_bar/`, `ui/editor_page/local_history.rs`, bookmarks, `ui/window/session_restore/`, `ui/window/search.rs`, the automation snapshot, `scripts/lushtext-automation.py`.
- **Docs:** `docs/automation.md`, `docs/automation-reference.md`, `docs/accessibility*.md`, the `WFR-PROTECTED-VIEW` matrix row.
- **Budgets:** inspector navigation and fold-marker activation are interaction dispatches within `B_interact`; whole-line clipboard writes are settle dispatches within `B_settle`; the verdict re-runs C2's lane cases with the inspector and Copy Whole Line added.

## Decisions taken at apply (2026-11-01)

- **Protected heads keep C5's wrap gate** (measured; spec requirement "Protected heads follow the per-paragraph wrap gate"). Interleaved `main` / exempt, plain, 3 + 3 runs each: with the gate the 1.9M wrap-on open phase's worst dispatch outside the declared hidden-tail insert is 8.7–12.5 ms (head paint 3.3–3.7 ms) and the tail insert 13.7–13.9 ms; wrapping the protected head instead costs 12.2–16.2 ms (paint 13.3–13.9 ms) and 18.7–19.0 ms. The gate keeps C2's 17.6 ms head-frame exception from returning; the Line Inspector's window, which wraps, is where a long line is read.
- **`N_INSPECT` stays 500.** It predates the power-of-two rule and is tied to `L_PROTECT` by a compile-time and Kani bound; `power-of-two-thresholds` converts both together. Every new threshold is a power of two: `PARAGRAPH_CHECKPOINT_STRIDE` 1,024 and `CHAR_CHECKPOINT_STRIDE` 64 Ki (caps on a worker walk).
- **Copy Whole Line shares the source.** The provider holds a reference to the editor's `ProtectedTextSource` allocation (no copy) and writes one `COPY_CHUNK_BYTES` chunk per awaited write; no `GType` is offered, so no reader can make GTK materialize the line.
- **Navigation without a column.** Workspace-search hits and bookmarks carry a line only (search excerpts are bounded), so they scroll to their paragraph through the navigation mark; protected find, which has the column, opens the inspector on a hidden match, but only on an explicit next/previous step so typing a query never pops a dialog.
- **Two-button alert.** The inline alert has two action buttons, so the merged restored-draft alert keeps Discard and Save and names Ctrl+Alt+I in its body; an available entry undo takes the primary button ("Undo Edit"), otherwise Inspect Line does.
- **Gutter accessibility.** A GtkSourceGutter draws its rows and has no per-row accessible object, so the renderer carries a label and description and the per-line hidden count is in its tooltip and in the inspector's window label.
- **Pre-existing gaps fixed here:** the protected find held the `editor-search` blocker only in the workflow observation, not in readiness (`search-complete` could settle before the worker answered); the class-shortcut enumeration found `move-focus` (classified viewport-safe) and `text.clear` (the Clear key; pre-empted, although GTK's own handler leaves a non-editable buffer unchanged) unclassified; the file-peek sample could show a 16 KiB line, above `HEAD_CAP`.
