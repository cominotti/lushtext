## Context

LushText already has a critical UI rule that Edit/Render dialog surfaces must not change geometry on first activation. The current note editor implementation partially follows it: existing non-empty notes pre-render the hidden Render page before presentation. That misses the reported path where the user opens a new empty range note, types text, and then clicks Render for the first time.

The shared note editor surface lives in `crates/lushtext-core/src/ui/window/notes.rs::build_note_editor_surface()`. Its Edit page is a fixed `GtkTextView` inside a `GtkScrolledWindow`. Its Render page is `LushtextMarkdownPreview`, which can switch internally between an `AdwStatusPage` placeholder and a rendered `GtkTextView` scroller. Even when the outer stack has size requests, replacing the visible child inside the hidden Render page can still alter the Render page's natural request after the modal has settled.

This change also raises the standard from "within one pixel" to exact visual stability. One-pixel modal drift is still a user-visible geometry inconsistency and should fail the contract.

## Goals / Non-Goals

**Goals:**

- Fix the new empty-note -> typed text -> first Render shrink in the shared note editor.
- Make document-note, workspace-note, and range-note editor mode switches follow the same geometry contract.
- Audit modal and popup surfaces that can change content after presentation, then either prove they are already fixed-size or stabilize their dynamic pages.
- Add widget coverage that measures real presented modal geometry and text origins with no tolerance for outer-size drift.
- Keep the implementation compatible with GTK4/Libadwaita measurement rules rather than relying on incidental current sizes.

**Non-Goals:**

- Do not change note storage, annotation sidecars, document-note/workspace-note files, or bookmark behavior.
- Do not redesign modal content, workflow copy, or GNOME HIG action layout beyond what geometry stability requires.
- Do not introduce screenshot testing or a new E2E harness.
- Do not convert static confirmation dialogs into fixed-size browser dialogs unless an actual dynamic-content risk is found.

## Decisions

### 1. Stabilize modal geometry at the page-host boundary

Dynamic modal pages should expose the same geometry before and after their content changes. For the note editor, this means the Render page host must advertise the same min, natural, and allocated size whether it is showing an empty placeholder or rendered Markdown.

The implementation should make the note editor's Render page structurally stable instead of relying on "render before switching" as the only guard. Viable implementation options include:

- keeping the preview scroller visible in the embedded note-editor mode and rendering empty-state copy inside the same fixed text surface;
- wrapping placeholder and rendered content in a fixed-size host that owns the geometry contract;
- or adding a note-editor-specific preview mode to `LushtextMarkdownPreview` where placeholder and content both report the same fixed content dimensions.

Alternative considered: render the Markdown preview every time the edit buffer changes, while Render is still hidden. That still changes hidden page content after the modal is presented and can move the resize to typing time instead of the Render click. It also does unnecessary rendering work for users who never switch to Render.

### 2. Keep fixed-size modal browsers fixed; audit rather than rewrite

The Notes browser and Local History viewer already use explicit `content_width` / `content_height` and `follows_content_size(false)` for their main populated states. Those should not be rewritten as part of this change. Instead, the implementation should add or preserve focused tests that prove selection changes, filtering, preview swaps, and empty states do not alter the browser shell.

Alternative considered: make every modal in the application fixed-size. That would reduce geometry drift risk, but it would also make simple confirmation and rename dialogs feel oversized and less GNOME-native.

### 3. Test real presented geometry, not only natural requests

Existing note geometry helpers compare natural sizes and allow a one-pixel tolerance. The new contract should use presented widget geometry as the acceptance surface: record the modal's outer allocated size and relevant child bounds, perform the user interaction, flush/wait until layout settles, then assert exact equality for the modal shell.

Natural-size checks can remain useful diagnostics, but they should not be the only acceptance condition. For text-origin parity, compare actual `compute_bounds()` positions of the editable and rendered text surfaces within the dialog content.

Alternative considered: keep the one-pixel tolerance to avoid flaky tests. That would codify the exact bug class the user reported. If GTK timing makes exact tests flaky, the implementation should wait for stable allocation rather than relax the requirement.

### 4. Inventory dynamic modal surfaces before patching broadly

The first implementation step should classify existing modal and popup surfaces:

- shared note editor dialogs using `build_note_editor_surface()`;
- fixed-size modal browsers such as Notes and Local History;
- content-following alert dialogs such as save changes, encoding choices, file health, bookmark label, and workspace rename;
- popovers or popup-like secondary surfaces with dynamic content.

Only surfaces with content changes after presentation need code changes. Static dialogs can be marked covered by audit unless they mutate visible child trees while open.

Alternative considered: grep for every `AlertDialog` and add fixed dimensions. That is broad, visually heavy, and would obscure the real dynamic-content issue.

## Modal Surface Audit Results

**Stabilized content-following dynamic surfaces:**

- Document note, workspace note, and range note editors share `build_note_editor_surface()`. Their Render page can change from empty placeholder copy to rendered Markdown after presentation, so the implementation keeps placeholder copy inside the same rendered text surface and preserves the existing non-empty pre-render path.

**Fixed-size dynamic surfaces covered by widget tests:**

- Notes browser: populated browser uses `AdwDialog` with explicit width/height and `follows_content_size(false)`. Coverage now asserts exact shell allocation stability across row activation, collapsed preview navigation, note preview selection, filtering, and empty filtered results.
- Local History browser: populated viewer uses explicit width/height derived from the parent window and `follows_content_size(false)`. Coverage now asserts exact shell allocation stability across initial async preview load, selection loading, selected preview load, and empty-snapshot preview state.

**Static or intentionally content-following surfaces requiring no code change:**

- Empty Notes and empty Local History dialogs are static `AdwStatusPage` dialogs. They follow content size, but the visible child tree does not mutate while open.
- Save As/Open/Export file dialogs are delegated to GTK file dialogs and do not own in-app dynamic content.
- Save-changes, encoding, line-ending, bookmark-label, save-search, workspace rename/remove, and file delete confirmations are `AdwAlertDialog` surfaces whose extra children are fixed at presentation time. Validation/status feedback is routed through status messages or unchanged widgets, not through child-tree swaps.
- Header-bar menu popovers, zoom/theme popovers, sidebar context menus, and the Notes menu are menu-model popovers. Their models are refreshed outside popup construction and do not perform placeholder-to-content swaps while a modal dialog is open.
- Search history and sidebar file-peek popovers are non-modal popup surfaces with intentionally content-following list/preview behavior anchored to their invoking widget. Their size can follow explicit user selection/search content, and they do not block a modal workflow or mutate a hidden dialog page.

## Risks / Trade-offs

- [Exact geometry assertions can be timing-sensitive] -> Use the existing headless widget runner, present real windows, wait for stable allocation predicates, and compare after GTK layout has settled.
- [Placeholder and rendered Markdown may have different semantic content] -> Give both states the same host geometry while preserving clear empty-state copy.
- [Prewarming rendered content can add work on every keystroke] -> Prefer stable host geometry over continuous hidden rendering.
- [Fixing the shared preview widget could affect full-document Markdown preview] -> Keep any note-editor-specific geometry mode explicit and scoped, or verify full-preview widget tests if shared preview internals change.
- [Modal audit can become open-ended] -> Limit the audit to surfaces reachable from current window/sidebar/search workflows and only require code changes for dynamic content transitions.

## Migration Plan

No user-data migration is required. The change affects UI layout behavior and tests only. Rollback is the normal code rollback for the note editor geometry and modal tests; persisted notes, workspaces, sessions, drafts, and sidecar files remain compatible.

## Resolved Questions

- `LushtextMarkdownPreview` owns a scoped `show_content_placeholder()` helper so note editors can reuse the same rendered text surface without changing full-document Markdown preview placeholder behavior.
- Empty-state browser dialogs remain static content-following dialogs because their visible content does not mutate while open; populated dynamic browsers keep their fixed-size contracts.
